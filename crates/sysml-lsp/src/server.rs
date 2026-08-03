use std::path::PathBuf;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use sysml_core::model::qualify_model;
use sysml_core::parser;

use crate::code_actions;
use crate::code_lens;
use crate::completion;
use crate::convert::span_to_range;
use crate::diagnostics;
use crate::document_highlight;
use crate::document_link;
use crate::document_symbols;
use crate::folding;
use crate::formatting;
use crate::goto_definition;
use crate::hover;
use crate::inlay_hints;
use crate::references;
use crate::rename;
use crate::semantic_tokens as sem_tok;
use crate::state::{FileState, WorldState};
use crate::type_hierarchy;
use crate::workspace_symbols;

pub struct SysmlLanguageServer {
    client: Client,
    state: WorldState,
}

impl SysmlLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: WorldState::new(),
        }
    }

    fn workspace_def_names(&self) -> Vec<String> {
        self.state
            .workspace_defs
            .iter()
            .map(|entry| entry.value().name.clone())
            .collect()
    }

    /// Simple names referenced by files other than `uri` — used to
    /// suppress cross-file unused-definition false positives (and the
    /// dangerous "Remove unused" quickfix they would offer).
    fn workspace_external_refs(&self, uri: &str) -> Vec<String> {
        let mut refs = std::collections::HashSet::new();
        for entry in self.state.files.iter() {
            if entry.key() == uri {
                continue;
            }
            for name in entry.value().model.referenced_names() {
                refs.insert(name.to_string());
            }
        }
        refs.into_iter().collect()
    }

    async fn on_change(&self, uri: Url, text: String, version: i32) {
        let uri_str = uri.to_string();
        let mut model = parser::parse_file(&uri_str, &text);
        qualify_model(&mut model);

        // Update index first so workspace names are current
        self.state.index_model_defs(&uri_str, &model);

        // Compute diagnostics with cross-file awareness
        let workspace_names = self.workspace_def_names();
        let external_refs = self.workspace_external_refs(&uri_str);
        let diags =
            diagnostics::compute_diagnostics(&model, &text, &workspace_names, &external_refs);
        self.state.files.insert(
            uri_str,
            FileState {
                source: text,
                model,
                version,
            },
        );

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri, diags, Some(version))
            .await;
    }

    fn scan_workspace(&self, root: &PathBuf) {
        let mut files = Vec::new();
        collect_sysml_files(root, &mut files);
        for path in files {
            let path_str = path.to_string_lossy().to_string();
            if let Ok(source) = std::fs::read_to_string(&path) {
                let uri = Url::from_file_path(&path)
                    .unwrap_or_else(|_| Url::parse(&format!("file://{}", path_str)).unwrap());
                let uri_str = uri.to_string();
                let mut model = parser::parse_file(&path_str, &source);
                qualify_model(&mut model);
                self.state.index_model_defs(&uri_str, &model);
                // Keep the parsed file available so workspace-wide
                // operations (rename, references, type hierarchy) see
                // closed files too. did_open replaces this entry.
                if !self.state.files.contains_key(&uri_str) {
                    self.state.files.insert(
                        uri_str.clone(),
                        FileState {
                            source,
                            model,
                            version: 0,
                        },
                    );
                }
            }
        }
    }
}

fn collect_sysml_files(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sysml_files(&path, files);
        } else if let Some(ext) = path.extension() {
            if ext == "sysml" || ext == "kerml" {
                files.push(path);
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SysmlLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Discover workspace root
        if let Some(root_uri) = params.root_uri {
            if let Ok(root_path) = root_uri.to_file_path() {
                self.scan_workspace(&root_path);
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![":".to_string(), ".".to_string()]),
                    ..Default::default()
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: sem_tok::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                // NOTE: lsp-types 0.94 (pinned by tower-lsp 0.20) has no
                // type_hierarchy_provider field, so the implemented type
                // hierarchy cannot be advertised until tower-lsp is
                // upgraded. Clients that probe the methods anyway get
                // working responses.
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "sysml-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("sysml-lsp initialized");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.on_change(doc.uri, doc.text, doc.version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        // FULL sync: last content change has the full text
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(uri, change.text, version).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let uri_str = uri.to_string();

        if let Some(file_state) = self.state.files.get(&uri_str) {
            let workspace_names = self.workspace_def_names();
            let external_refs = self.workspace_external_refs(&uri_str);
            let diags = diagnostics::compute_diagnostics(
                &file_state.model,
                &file_state.source,
                &workspace_names,
                &external_refs,
            );
            self.client
                .publish_diagnostics(uri, diags, Some(file_state.version))
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri_str = params.text_document.uri.to_string();
        // Replace the buffer state with the on-disk content so the file
        // remains visible to workspace-wide operations after closing.
        self.state.files.remove(&uri_str);
        if let Ok(path) = params.text_document.uri.to_file_path() {
            if let Ok(source) = std::fs::read_to_string(&path) {
                let mut model = parser::parse_file(&path.to_string_lossy(), &source);
                qualify_model(&mut model);
                self.state.index_model_defs(&uri_str, &model);
                self.state.files.insert(
                    uri_str.clone(),
                    FileState {
                        source,
                        model,
                        version: 0,
                    },
                );
            }
        }
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };
        let symbols =
            document_symbols::document_symbols(&file_state.model, &file_state.source);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let uri_str = uri.to_string();

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        let Some(name) = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        ) else {
            return Ok(None);
        };

        let Some((target_uri, span)) =
            goto_definition::goto_definition(&file_state.model, &name, &uri_str, &self.state)
        else {
            return Ok(None);
        };

        let target_url = if target_uri == uri_str {
            uri
        } else {
            Url::parse(&target_uri).unwrap_or(uri)
        };

        let target_source = self
            .state
            .files
            .get(&target_uri)
            .map(|fs| fs.source.clone())
            .unwrap_or_default();
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_url,
            range: span_to_range(&span, &target_source),
        })))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let uri_str = uri.to_string();
        let include_declaration = params.context.include_declaration;

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        // Find the identifier at cursor
        let name = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        )
        .or_else(|| find_element_name_at_offset(&file_state.model, &file_state.source, offset));

        let Some(name) = name else {
            return Ok(None);
        };

        // Collect all open file models
        let models: Vec<_> = self
            .state
            .files
            .iter()
            .map(|entry| (entry.key().clone(), entry.key().clone()))
            .collect();

        let mut all_refs = Vec::new();
        for (uri_key, _) in &models {
            if let Some(fs) = self.state.files.get(uri_key) {
                let file_refs = references::find_references_in_model(&fs.model, uri_key, &name);
                for r in file_refs {
                    if let Ok(loc_uri) = Url::parse(&r.uri) {
                        all_refs.push(Location {
                            uri: loc_uri,
                            range: span_to_range(&r.span, &fs.source),
                        });
                    }
                }

                // Include declaration if requested
                if include_declaration {
                    if let Some(def) = fs.model.find_def(sysml_core::model::simple_name(&name)) {
                        if let Ok(loc_uri) = Url::parse(uri_key) {
                            all_refs.push(Location {
                                uri: loc_uri,
                                range: span_to_range(&def.span, &fs.source),
                            });
                        }
                    }
                }
            }
        }

        if all_refs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_refs))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let uri_str = uri.to_string();

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        let name = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        );

        let markdown = if let Some(ref name) = name {
            hover::hover_info(&file_state.model, name)
                .or_else(|| hover::hover_usage_info(&file_state.model, name))
        } else {
            find_element_name_at_offset(&file_state.model, &file_state.source, offset).and_then(
                |n| {
                    hover::hover_info(&file_state.model, &n)
                        .or_else(|| hover::hover_usage_info(&file_state.model, &n))
                },
            )
        };

        let Some(markdown) = markdown else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        // Detect completion context from text before cursor
        let filter =
            if let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) {
                completion::detect_context(&file_state.source, offset)
            } else {
                completion::CompletionFilter::All
            };

        let workspace_defs: Vec<_> = self
            .state
            .workspace_defs
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        let items = completion::completions(&file_state.model, &workspace_defs, &filter);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let defs: Vec<_> = self
            .state
            .workspace_defs
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        let symbols = workspace_symbols::workspace_symbols(&params.query, &defs);
        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let tokens = sem_tok::semantic_tokens(&file_state.source);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let uri_str = uri.to_string();
        let source = self.state.files.get(&uri_str).map(|fs| fs.source.clone());
        let workspace_names = self.workspace_def_names();
        let actions = code_actions::code_actions(
            &uri,
            &params.context.diagnostics,
            source.as_deref(),
            Some(&workspace_names),
        );
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                actions
                    .into_iter()
                    .map(CodeActionOrCommand::CodeAction)
                    .collect(),
            ))
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        Ok(formatting::format_document(
            &file_state.source,
            Some(&params.options),
        ))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let uri_str = uri.to_string();

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        let name = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        )
        .or_else(|| find_element_name_at_offset(&file_state.model, &file_state.source, offset));

        let Some(name) = name else {
            return Ok(None);
        };

        let highlights =
            document_highlight::document_highlights(&file_state.model, &file_state.source, &name);
        if highlights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(highlights))
        }
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let ranges = folding::folding_ranges(&file_state.model);
        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &params.position)
        else {
            return Ok(None);
        };

        let Some((name, range)) =
            rename::prepare_rename(&file_state.model, &file_state.source, offset)
        else {
            return Ok(None);
        };

        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range,
            placeholder: name,
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let uri_str = uri.to_string();
        let new_name = params.new_name;

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        // Find the name to rename
        let name = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        )
        .or_else(|| find_element_name_at_offset(&file_state.model, &file_state.source, offset));

        let Some(old_name) = name else {
            return Ok(None);
        };

        // Collect all open files
        // We need to drop the DashMap ref before iterating
        let file_keys: Vec<String> = self
            .state
            .files
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        let mut models_data: Vec<(String, String, sysml_core::model::Model)> = Vec::new();
        for key in &file_keys {
            if let Some(fs) = self.state.files.get(key) {
                models_data.push((key.clone(), fs.source.clone(), fs.model.clone()));
            }
        }

        let models_refs: Vec<(&str, &str, &sysml_core::model::Model)> = models_data
            .iter()
            .map(|(uri, src, model)| (uri.as_str(), src.as_str(), model))
            .collect();

        Ok(rename::rename_symbol(&models_refs, &old_name, &new_name))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };
        let hints = inlay_hints::inlay_hints(&file_state.model, &file_state.source);
        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let uri_str = uri.to_string();

        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };

        let Some(offset) = crate::convert::position_to_offset(&file_state.source, &pos) else {
            return Ok(None);
        };

        // Find name at cursor (definition name or type reference)
        let name = goto_definition::find_identifier_at_offset(
            &file_state.model,
            &file_state.source,
            offset,
        )
        .or_else(|| find_element_name_at_offset(&file_state.model, &file_state.source, offset));

        let Some(name) = name else {
            return Ok(None);
        };

        let item = type_hierarchy::prepare_type_hierarchy(
            &file_state.model,
            &file_state.source,
            &uri,
            &name,
        );
        Ok(item.map(|i| vec![i]))
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let name = &params.item.name;
        let models_data: Vec<_> = self
            .state
            .files
            .iter()
            .map(|e| (e.key().clone(), e.value().source.clone(), e.value().model.clone()))
            .collect();
        let models_refs: Vec<(&str, &str, &sysml_core::model::Model)> = models_data
            .iter()
            .map(|(uri, source, model)| (uri.as_str(), source.as_str(), model))
            .collect();

        let result = type_hierarchy::supertypes(&models_refs, name);
        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let name = &params.item.name;
        let models_data: Vec<_> = self
            .state
            .files
            .iter()
            .map(|e| (e.key().clone(), e.value().source.clone(), e.value().model.clone()))
            .collect();
        let models_refs: Vec<(&str, &str, &sysml_core::model::Model)> = models_data
            .iter()
            .map(|(uri, source, model)| (uri.as_str(), source.as_str(), model))
            .collect();

        let result = type_hierarchy::subtypes(&models_refs, name);
        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };
        let lenses = code_lens::code_lenses(&file_state.model, &file_state.source);
        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri_str = params.text_document.uri.to_string();
        let Some(file_state) = self.state.files.get(&uri_str) else {
            return Ok(None);
        };
        // Build a simple-name → file mapping over every workspace file
        // we know about, so the link provider can resolve qualified
        // names like `Lib::Foo` to the file declaring `Foo`.
        let mut name_to_file: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for entry in self.state.files.iter() {
            let model = &entry.value().model;
            for d in &model.definitions {
                name_to_file
                    .entry(d.name.clone())
                    .or_insert_with(|| model.file.clone());
            }
        }
        let links =
            document_link::document_links(&file_state.model, &file_state.source, &name_to_file);
        if links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(links))
        }
    }
}

/// Try to find a definition or usage name at the given byte offset
/// (for hovering on the name itself, not a reference).
fn find_element_name_at_offset(
    model: &sysml_core::model::Model,
    source: &str,
    offset: usize,
) -> Option<String> {
    for def in &model.definitions {
        if def.span.start_byte <= offset && offset < def.span.end_byte {
            let def_text = &source[def.span.start_byte..def.span.end_byte];
            if let Some(pos) = def_text.find(&def.name) {
                let abs_start = def.span.start_byte + pos;
                let abs_end = abs_start + def.name.len();
                if abs_start <= offset && offset < abs_end {
                    return Some(def.name.clone());
                }
            }
        }
    }
    for usage in &model.usages {
        if usage.span.start_byte <= offset && offset < usage.span.end_byte {
            let usage_text = &source[usage.span.start_byte..usage.span.end_byte];
            if let Some(pos) = usage_text.find(&usage.name) {
                let abs_start = usage.span.start_byte + pos;
                let abs_end = abs_start + usage.name.len();
                if abs_start <= offset && offset < abs_end {
                    return Some(usage.name.clone());
                }
            }
        }
    }
    None
}
