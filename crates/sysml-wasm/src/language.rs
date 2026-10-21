//! Editor language features for the browser: completion, hover, go to
//! definition, references, rename, outline, formatting and quick fixes.
//!
//! The logic lives in `sysml_core::lang` (shared with the language
//! server); this module speaks JSON and UTF-16 offsets. Every project-wide
//! function takes the project as JSON `[{ path, text }]`.
//!
//! Parsed files are cached by path and text hash, and the last workspace
//! (with its name-resolution caches) by the hash of the whole project, so
//! repeated calls while the user moves the cursor or types in one file only
//! re-index what changed.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use serde::Serialize;
use sysml_core::diagnostic::Diagnostic;
use sysml_core::lang::{
    self, actions, completion, definition, format, hover, references, rename, symbols, FileIndex,
    Src, TextChange, Workspace,
};
use wasm_bindgen::prelude::*;

use crate::editor::{Change, SourceFile, Utf16Index};
use crate::to_json;

struct CachedFile {
    hash: u64,
    index: Arc<FileIndex>,
    utf16: Rc<Utf16Index>,
}

struct Project {
    hash: u64,
    ws: Rc<Workspace>,
    utf16: Rc<HashMap<String, Rc<Utf16Index>>>,
    diagnostics: Option<Rc<Vec<Vec<Diagnostic>>>>,
}

thread_local! {
    static FILES: RefCell<HashMap<String, CachedFile>> = RefCell::new(HashMap::new());
    static PROJECT: RefCell<Option<Project>> = const { RefCell::new(None) };
}

fn hash_of(value: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

/// A project indexed for the language features.
pub struct Indexed {
    pub ws: Rc<Workspace>,
    utf16: Rc<HashMap<String, Rc<Utf16Index>>>,
    hash: u64,
}

impl Indexed {
    /// Index `files`, reusing cached work.
    pub fn new(files: &[SourceFile]) -> Self {
        let hashes: Vec<u64> = files.iter().map(|f| hash_of(&f.text)).collect();
        let project_hash = hash_of(
            files
                .iter()
                .zip(&hashes)
                .map(|(f, h)| (&f.path, h))
                .collect::<Vec<_>>(),
        );
        if let Some(found) = PROJECT.with_borrow(|p| {
            p.as_ref().filter(|p| p.hash == project_hash).map(|p| Self {
                ws: p.ws.clone(),
                utf16: p.utf16.clone(),
                hash: p.hash,
            })
        }) {
            return found;
        }
        let (indexes, utf16) = FILES.with_borrow_mut(|cache| {
            let mut indexes = Vec::with_capacity(files.len());
            let mut utf16 = HashMap::new();
            for (f, &h) in files.iter().zip(&hashes) {
                let entry = match cache.get(&f.path) {
                    Some(c) if c.hash == h => c,
                    _ => {
                        cache.insert(
                            f.path.clone(),
                            CachedFile {
                                hash: h,
                                index: Arc::new(FileIndex::new(&f.path, &f.text)),
                                utf16: Rc::new(Utf16Index::new(&f.text)),
                            },
                        );
                        &cache[&f.path]
                    }
                };
                indexes.push(entry.index.clone());
                utf16.insert(f.path.clone(), entry.utf16.clone());
            }
            // Forget files no longer in the project.
            cache.retain(|path, _| utf16.contains_key(path));
            (indexes, utf16)
        });
        let ws = Rc::new(Workspace::new(indexes));
        let utf16 = Rc::new(utf16);
        PROJECT.set(Some(Project {
            hash: project_hash,
            ws: ws.clone(),
            utf16: utf16.clone(),
            diagnostics: None,
        }));
        Self {
            ws,
            utf16,
            hash: project_hash,
        }
    }

    fn src(&self, path: &str) -> Result<Src, String> {
        self.ws
            .find_file(path)
            .ok_or_else(|| format!("no file {path:?} in the project"))
    }

    fn index(&self, path: &str) -> &Utf16Index {
        &self.utf16[path]
    }

    /// UTF-16 offset in `path` to a byte offset.
    fn byte(&self, path: &str, unit: u32) -> usize {
        self.index(path).byte(unit)
    }

    fn unit(&self, path: &str, byte: usize) -> u32 {
        self.index(path).at(byte)
    }

    fn change(&self, c: TextChange) -> FileChange {
        FileChange {
            from: self.unit(&c.path, c.start),
            to: self.unit(&c.path, c.end),
            path: c.path,
            insert: c.insert,
        }
    }

    /// Project diagnostics per file (cached with the workspace).
    fn diagnostics(&self) -> Rc<Vec<Vec<Diagnostic>>> {
        if let Some(d) = PROJECT.with_borrow(|p| {
            p.as_ref()
                .filter(|p| p.hash == self.hash)
                .and_then(|p| p.diagnostics.clone())
        }) {
            return d;
        }
        let models = self.ws.files.iter().map(|f| f.model.clone()).collect();
        let diags: Rc<Vec<Vec<Diagnostic>>> = Rc::new(
            lang::analyze_models(models)
                .into_iter()
                .map(|(_, d)| d)
                .collect(),
        );
        PROJECT.with_borrow_mut(|p| {
            if let Some(p) = p.as_mut().filter(|p| p.hash == self.hash) {
                p.diagnostics = Some(diags.clone());
            }
        });
        diags
    }
}

fn parse_files(files: &str) -> Result<Vec<SourceFile>, JsError> {
    serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))
}

fn js(e: String) -> JsError {
    JsError::new(&e)
}

/// A change to one file, in UTF-16 offsets.
#[derive(Serialize, Debug, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

#[derive(Serialize)]
pub struct Completions {
    pub from: u32,
    pub to: u32,
    pub items: Vec<completion::CompletionItem>,
}

/// Completions at `offset`.
pub fn complete_in(ix: &Indexed, path: &str, offset: u32) -> Result<Completions, String> {
    let src = ix.src(path)?;
    let c = completion::complete(&ix.ws, src, ix.byte(path, offset));
    Ok(Completions {
        from: ix.unit(path, c.from),
        to: ix.unit(path, c.to),
        items: c.items,
    })
}

#[derive(Serialize, Debug)]
pub struct Hover {
    pub from: u32,
    pub to: u32,
    pub markdown: String,
}

pub fn hover_in(ix: &Indexed, path: &str, offset: u32) -> Result<Option<Hover>, String> {
    let src = ix.src(path)?;
    Ok(
        hover::hover(&ix.ws, src, ix.byte(path, offset)).map(|h| Hover {
            from: ix.unit(path, h.start),
            to: ix.unit(path, h.end),
            markdown: h.markdown,
        }),
    )
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum Definition {
    File { path: String, from: u32, to: u32 },
    Library { library: LibraryLocation },
}

#[derive(Serialize, Debug, PartialEq)]
pub struct LibraryLocation {
    pub path: String,
    pub line: usize,
}

pub fn definition_in(ix: &Indexed, path: &str, offset: u32) -> Result<Option<Definition>, String> {
    let src = ix.src(path)?;
    Ok(
        definition::definition(&ix.ws, src, ix.byte(path, offset)).map(|t| match t {
            definition::Target::Project { path, start, end } => Definition::File {
                from: ix.unit(&path, start),
                to: ix.unit(&path, end),
                path,
            },
            definition::Target::Library { path, line } => Definition::Library {
                library: LibraryLocation { path, line },
            },
        }),
    )
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Location {
    pub path: String,
    pub from: u32,
    pub to: u32,
}

pub fn references_in(ix: &Indexed, path: &str, offset: u32) -> Result<Vec<Location>, String> {
    let src = ix.src(path)?;
    Ok(references::references(&ix.ws, src, ix.byte(path, offset))
        .into_iter()
        .map(|l| Location {
            from: ix.unit(&l.path, l.start),
            to: ix.unit(&l.path, l.end),
            path: l.path,
        })
        .collect())
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum RenameResult {
    Changes { changes: Vec<FileChange> },
    Error { error: String },
}

pub fn rename_in(
    ix: &Indexed,
    path: &str,
    offset: u32,
    new_name: &str,
) -> Result<RenameResult, String> {
    let src = ix.src(path)?;
    Ok(
        match rename::rename(&ix.ws, src, ix.byte(path, offset), new_name) {
            Ok(changes) => RenameResult::Changes {
                changes: changes.into_iter().map(|c| ix.change(c)).collect(),
            },
            Err(error) => RenameResult::Error { error },
        },
    )
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub from: u32,
    pub to: u32,
    pub selection_from: u32,
    pub selection_to: u32,
    pub children: Vec<OutlineItem>,
}

fn outline_item(index: &Utf16Index, item: symbols::OutlineItem) -> OutlineItem {
    OutlineItem {
        name: item.name,
        kind: item.kind,
        detail: item.detail,
        from: index.at(item.start),
        to: index.at(item.end),
        selection_from: index.at(item.selection_start),
        selection_to: index.at(item.selection_end),
        children: item
            .children
            .into_iter()
            .map(|c| outline_item(index, c))
            .collect(),
    }
}

pub fn outline_in(ix: &Indexed, path: &str) -> Result<Vec<OutlineItem>, String> {
    let src = ix.src(path)?;
    let index = ix.index(path);
    Ok(symbols::outline(&ix.ws, src)
        .into_iter()
        .map(|i| outline_item(index, i))
        .collect())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    pub changes: Vec<FileChange>,
}

pub fn code_actions_in(
    ix: &Indexed,
    path: &str,
    from: u32,
    to: u32,
) -> Result<Vec<CodeAction>, String> {
    let src = ix.src(path)?;
    let Src::Project(f) = src else {
        unreachable!("project files only")
    };
    let diagnostics = ix.diagnostics();
    let (start, end) = (ix.byte(path, from), ix.byte(path, to));
    Ok(
        actions::code_actions(&ix.ws, src, start, end, &diagnostics[f as usize])
            .into_iter()
            .map(|a| CodeAction {
                title: a.title,
                diagnostic_code: a.diagnostic_code,
                changes: a.changes.into_iter().map(|c| ix.change(c)).collect(),
            })
            .collect(),
    )
}

#[derive(Serialize, Debug)]
pub struct Formatted {
    pub changes: Vec<Change>,
}

/// Formatting changes for one source.
pub fn format_changes(source: &str) -> Formatted {
    let index = Utf16Index::new(source);
    Formatted {
        changes: format::format_edits(source)
            .into_iter()
            .map(|e| Change {
                from: index.at(e.start),
                to: index.at(e.end),
                insert: e.insert,
            })
            .collect(),
    }
}

/// Completions at UTF-16 `offset` of file `path`: `{ from, to, items:
/// [{ label, kind, detail?, documentation?, insertText?, isSnippet? }] }`,
/// `kind` one of `keyword`, `definition`, `usage`, `package`, `library`,
/// `snippet`; `from..to` is the word being completed. Snippets use
/// `${1:Name}` / `$0` placeholders.
#[wasm_bindgen]
pub fn complete(files: &str, path: &str, offset: u32) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&complete_in(&ix, path, offset).map_err(js)?))
}

/// Hover for the name at `offset`: `{ from, to, markdown }` or `null`.
#[wasm_bindgen]
pub fn hover(files: &str, path: &str, offset: u32) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&hover_in(&ix, path, offset).map_err(js)?))
}

/// The declaration of the name at `offset`: `{ path, from, to }` (its name
/// in a project file), `{ library: { path, line } }` (a standard library
/// file, see `libraryFile`) or `null`.
#[wasm_bindgen]
pub fn definition(files: &str, path: &str, offset: u32) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&definition_in(&ix, path, offset).map_err(js)?))
}

/// Every occurrence in the project of the element named at `offset`,
/// declaration included: `[{ path, from, to }]`.
#[wasm_bindgen]
pub fn references(files: &str, path: &str, offset: u32) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&references_in(&ix, path, offset).map_err(js)?))
}

/// Rename the element named at `offset` everywhere in the project:
/// `{ changes: [{ path, from, to, insert }] }` or `{ error }`. Names that
/// are not basic names are quoted (`'Battery pack'`).
#[wasm_bindgen(js_name = renameSymbol)]
pub fn rename_symbol(
    files: &str,
    path: &str,
    offset: u32,
    new_name: &str,
) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(
        &rename_in(&ix, path, offset, new_name).map_err(js)?,
    ))
}

/// The outline of file `path`: nested `[{ name, kind, detail?, from, to,
/// selectionFrom, selectionTo, children }]`.
#[wasm_bindgen]
pub fn outline(files: &str, path: &str) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&outline_in(&ix, path).map_err(js)?))
}

/// Format one source: `{ changes: [{ from, to, insert }] }`, minimal
/// (typically re-indented lines), empty when already formatted.
#[wasm_bindgen(js_name = format)]
pub fn format_source(source: &str) -> String {
    to_json(&format_changes(source))
}

/// Quick fixes for the diagnostics of `path` overlapping `from..to`:
/// `[{ title, diagnosticCode?, changes: [{ path, from, to, insert }] }]`.
#[wasm_bindgen(js_name = codeActions)]
pub fn code_actions(files: &str, path: &str, from: u32, to: u32) -> Result<String, JsError> {
    let ix = Indexed::new(&parse_files(files)?);
    Ok(to_json(&code_actions_in(&ix, path, from, to).map_err(js)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<SourceFile> {
        list.iter()
            .map(|(p, t)| SourceFile {
                path: p.to_string(),
                text: t.to_string(),
            })
            .collect()
    }

    /// UTF-16 offset of `needle` in `text`.
    fn at(text: &str, needle: &str) -> u32 {
        text[..text.find(needle).unwrap()].encode_utf16().count() as u32
    }

    #[test]
    fn positions_are_utf16() {
        let a = "package Ä { doc /* 😀 */ part def Battery; part b : Battery; }";
        let ix = Indexed::new(&files(&[("a.sysml", a)]));
        let h = hover_in(&ix, "a.sysml", at(a, "Battery;") + 1)
            .unwrap()
            .unwrap();
        assert_eq!((h.from, h.to), (at(a, "Battery;"), at(a, "Battery;") + 7));
        let refs = references_in(&ix, "a.sysml", at(a, "Battery;")).unwrap();
        assert_eq!(
            refs.iter().map(|r| r.from).collect::<Vec<_>>(),
            [at(a, "Battery;"), at(a, "Battery; }")]
        );
        assert!(hover_in(&ix, "missing.sysml", 0).is_err());
    }

    #[test]
    fn json_shapes() {
        let a = "package A { part def Battery; part b : Battery; }\n";
        let b = "package B { private import A::*; part c : Battery; }\n";
        let list = serde_json::to_string(&serde_json::json!([
            { "path": "a.sysml", "text": a },
            { "path": "b.sysml", "text": b },
        ]))
        .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&definition(&list, "b.sysml", at(b, "Battery")).unwrap()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "path": "a.sysml", "from": at(a, "Battery"), "to": at(a, "Battery") + 7 })
        );
        let v: serde_json::Value = serde_json::from_str(
            &rename_symbol(&list, "a.sysml", at(a, "Battery"), "Battery pack").unwrap(),
        )
        .unwrap();
        assert_eq!(v["changes"].as_array().unwrap().len(), 3);
        assert_eq!(v["changes"][0]["insert"], "'Battery pack'");
        let v: serde_json::Value =
            serde_json::from_str(&rename_symbol(&list, "a.sysml", 0, "X").unwrap()).unwrap();
        assert!(v["error"].is_string());
        let v: serde_json::Value =
            serde_json::from_str(&hover(&list, "a.sysml", 0).unwrap()).unwrap();
        assert!(v.is_null());
        let v: serde_json::Value =
            serde_json::from_str(&outline(&list, "a.sysml").unwrap()).unwrap();
        assert_eq!(v[0]["name"], "A");
        assert_eq!(v[0]["children"][0]["selectionFrom"], at(a, "Battery"));
        let v: serde_json::Value =
            serde_json::from_str(&format_source("part def A {\nattribute x;\n}\n")).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "changes": [{ "from": 13, "to": 13, "insert": "    " }] })
        );
    }

    #[test]
    fn cached_projects_reindex_only_changed_files() {
        let a = "part def A;";
        let ix1 = Indexed::new(&files(&[("a.sysml", a), ("b.sysml", "part x : A;")]));
        let ix2 = Indexed::new(&files(&[("a.sysml", a), ("b.sysml", "part y : A;")]));
        assert!(Arc::ptr_eq(&ix1.ws.files[0], &ix2.ws.files[0]));
        assert!(!Arc::ptr_eq(&ix1.ws.files[1], &ix2.ws.files[1]));
        let ix3 = Indexed::new(&files(&[("a.sysml", a), ("b.sysml", "part y : A;")]));
        assert!(Rc::ptr_eq(&ix2.ws, &ix3.ws));
    }
}
