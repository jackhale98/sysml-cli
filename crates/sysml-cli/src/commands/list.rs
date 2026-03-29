use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, read_source};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    cli: &Cli,
    files: &[PathBuf],
    kind: Option<&str>,
    name: Option<&str>,
    parent: Option<&str>,
    unused: bool,
    abstract_only: bool,
    visibility: Option<&str>,
    view: Option<&str>,
) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }
    let files = &files[..];

    use sysml_core::model::{DefKind, Visibility};
    use sysml_core::query::{self, KindFilter, ListFilter, parse_kind_filter};

    let kind_filter = kind.and_then(|k| parse_kind_filter(k).or_else(|| {
        // Fallback for extra aliases not in parse_kind_filter
        Some(match k {
            "use-cases" => KindFilter::DefKind(DefKind::UseCase),
            "verifications" => KindFilter::DefKind(DefKind::Verification),
            "allocations" => KindFilter::DefKind(DefKind::Allocation),
            "ref" => KindFilter::UsageKind("ref".to_string()),
            other => KindFilter::UsageKind(other.to_string()),
        })
    }));

    let vis_filter = visibility.map(|v| match v {
        "public" | "pub" => Visibility::Public,
        "private" | "priv" => Visibility::Private,
        "protected" | "prot" => Visibility::Protected,
        _ => {
            eprintln!("warning: unknown visibility `{}`, expected: public, private, protected", v);
            Visibility::Public
        }
    });

    let mut filter = ListFilter {
        kind: kind_filter,
        name_pattern: name.map(|s| s.to_string()),
        parent: parent.map(|s| s.to_string()),
        unused_only: unused,
        abstract_only,
        visibility: vis_filter,
    };

    // --view flag: will be applied per-file after parsing (view defs are in the model)
    let view_name = view.map(|s| s.to_string());

    // Collect into owned data to avoid lifetime issues across files
    struct ListRow {
        file: String,
        name: String,
        kind: String,
        line: usize,
        parent: Option<String>,
        type_ref: Option<String>,
        short_name: Option<String>,
        doc: Option<String>,
    }

    let mut rows = Vec::new();
    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);

        // Apply view filter if --view is specified
        if let Some(ref vn) = view_name {
            if let Some(view_filter) = query::filter_from_view(&model, vn) {
                // Merge view filter with CLI filter (CLI flags override view)
                if filter.kind.is_none() {
                    filter.kind = view_filter.kind;
                }
                if filter.parent.is_none() {
                    filter.parent = view_filter.parent;
                }
            } else {
                eprintln!("warning: view definition `{}` not found in `{}`", vn, path_str);
            }
        }

        let elements = query::list_elements(&model, &filter);
        for el in elements {
            rows.push(ListRow {
                file: path_str.clone(),
                name: el.name().to_string(),
                kind: el.kind_label().to_string(),
                line: el.span().start_row,
                parent: el.parent_def().map(|s| s.to_string()),
                type_ref: el.type_ref().map(|s| s.to_string()),
                short_name: el.short_name().map(|s| s.to_string()),
                doc: el.doc().map(|s| s.to_string()),
            });
        }
    }

    if cli.format == "json" {
        let json: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let mut obj = serde_json::json!({
                    "file": r.file,
                    "name": r.name,
                    "kind": r.kind,
                    "line": r.line,
                });
                if let Some(ref p) = r.parent {
                    obj["parent"] = serde_json::json!(p);
                }
                if let Some(ref t) = r.type_ref {
                    obj["type"] = serde_json::json!(t);
                }
                if let Some(ref sn) = r.short_name {
                    obj["short_name"] = serde_json::json!(sn);
                }
                if let Some(ref doc) = r.doc {
                    obj["doc"] = serde_json::json!(doc);
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if rows.is_empty() {
            println!("No matching elements found.");
            return ExitCode::SUCCESS;
        }
        for r in &rows {
            let loc = format!("{}:{}", r.file, r.line);
            let parent_str = r
                .parent
                .as_ref()
                .map(|p| format!(" (in {})", p))
                .unwrap_or_default();
            let type_str = r
                .type_ref
                .as_ref()
                .map(|t| format!(" : {}", t))
                .unwrap_or_default();
            println!(
                "  {:<14} {}{}{} [{}]",
                r.kind, r.name, type_str, parent_str, loc,
            );
        }
        if !cli.quiet {
            eprintln!("{} element(s) found.", rows.len());
        }
    }

    ExitCode::SUCCESS
}
