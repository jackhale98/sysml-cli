//! Go to definition.
//!
//! [`definition`] resolves the name under the cursor project-wide (and into
//! the standard library); the per-file helpers below serve the language
//! server.

use super::index::{Src, Workspace};
use crate::model::{simple_name, Model, Span};

/// Where a definition is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// In a project file: the byte span of the declared name.
    Project {
        path: String,
        start: usize,
        end: usize,
    },
    /// In a standard library file (1-based line of the declaration).
    Library { path: String, line: usize },
}

/// The declaration of the name at byte `offset` of file `src`.
pub fn definition(ws: &Workspace, src: Src, offset: usize) -> Option<Target> {
    let (id, _) = ws.resolve_at(src, offset)?;
    let e = ws.elem(id);
    let file = ws.file(id.src);
    Some(match id.src {
        Src::Project(_) => {
            let (start, end) = e.name_span.unwrap_or((e.start, e.start));
            Target::Project {
                path: file.path.clone(),
                start,
                end,
            }
        }
        Src::Library(_) => Target::Library {
            path: file.path.clone(),
            line: ws.line_of(id),
        },
    })
}

/// Find the identifier name at a given byte offset in the source.
/// Checks type_references, usage type_refs, and definition super_types.
pub fn find_identifier_at_offset(model: &Model, source: &str, offset: usize) -> Option<String> {
    for tr in &model.type_references {
        if tr.span.start_byte <= offset && offset < tr.span.end_byte {
            return Some(tr.name.clone());
        }
    }

    // Usage type_refs: the type text is at the end of the usage span.
    for usage in &model.usages {
        if let Some(ref type_ref) = usage.type_ref {
            if usage.span.start_byte <= offset && offset < usage.span.end_byte {
                let usage_text = &source[usage.span.start_byte..usage.span.end_byte];
                if let Some(pos) = usage_text.rfind(simple_name(type_ref)) {
                    let abs_start = usage.span.start_byte + pos;
                    let abs_end = abs_start + simple_name(type_ref).len();
                    if abs_start <= offset && offset < abs_end {
                        return Some(simple_name(type_ref).to_string());
                    }
                }
            }
        }
    }

    // Definition super_types
    for def in &model.definitions {
        if let Some(ref super_type) = def.super_type {
            if def.span.start_byte <= offset && offset < def.span.end_byte {
                let def_text = &source[def.span.start_byte..def.span.end_byte];
                let st_name = simple_name(super_type);
                if let Some(pos) = def_text.rfind(st_name) {
                    let abs_start = def.span.start_byte + pos;
                    let abs_end = abs_start + st_name.len();
                    if abs_start <= offset && offset < abs_end {
                        return Some(st_name.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Resolve a name to a definition span within the same file.
pub fn goto_definition_in_file(model: &Model, name: &str) -> Option<Span> {
    model.find_def(simple_name(name)).map(|d| d.span.clone())
}

/// The definition or usage whose own name is at `offset` (hovering a
/// declaration rather than a reference).
pub fn find_element_name_at_offset(model: &Model, source: &str, offset: usize) -> Option<String> {
    let spans = model
        .definitions
        .iter()
        .map(|d| (&d.span, &d.name))
        .chain(model.usages.iter().map(|u| (&u.span, &u.name)));
    for (span, name) in spans {
        if span.start_byte <= offset && offset < span.end_byte {
            let text = &source[span.start_byte..span.end_byte];
            if let Some(pos) = text.find(name.as_str()) {
                let abs_start = span.start_byte + pos;
                if abs_start <= offset && offset < abs_start + name.len() {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn find_type_ref_at_cursor() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let name = find_identifier_at_offset(&model, source, source.rfind("Engine").unwrap());
        assert_eq!(name.as_deref(), Some("Engine"));
        assert_eq!(
            goto_definition_in_file(&model, "Engine").unwrap().start_row,
            1
        );
    }

    #[test]
    fn element_name_at_offset() {
        let source = "part def Engine;\npart e : Engine;\n";
        let model = parse_file("test.sysml", source);
        assert_eq!(
            find_element_name_at_offset(&model, source, 10).as_deref(),
            Some("Engine")
        );
        assert_eq!(
            find_element_name_at_offset(&model, source, source.find("e :").unwrap()).as_deref(),
            Some("e")
        );
    }

    #[test]
    fn definition_crosses_files() {
        let a = "package A { private import B::*; part d { part b : Battery; } }";
        let b = "package B {\n    part def Battery;\n}\n";
        let ws = Workspace::from_sources([("a.sysml", a), ("b.sysml", b)]);
        let target = definition(&ws, Src::Project(0), a.find("Battery").unwrap()).unwrap();
        let start = b.find("Battery").unwrap();
        assert_eq!(
            target,
            Target::Project {
                path: "b.sysml".into(),
                start,
                end: start + 7
            }
        );
    }

    #[test]
    fn definition_into_the_library() {
        if crate::stdlib::stdlib_files().is_empty() {
            return;
        }
        let a = "part def P { attribute r : ScalarValues::Real; }";
        let ws = Workspace::from_sources([("a.sysml", a)]);
        match definition(&ws, Src::Project(0), a.find("Real").unwrap()) {
            Some(Target::Library { path, line }) => {
                assert!(path.ends_with("ScalarValues.kerml"), "{path}");
                let text = crate::stdlib::stdlib_files()
                    .iter()
                    .find(|(p, _)| *p == path)
                    .unwrap()
                    .1;
                assert!(text
                    .lines()
                    .nth(line - 1)
                    .unwrap()
                    .contains("datatype Real"));
            }
            other => panic!("{other:?}"),
        }
    }
}
