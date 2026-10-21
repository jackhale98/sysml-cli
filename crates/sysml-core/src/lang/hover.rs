//! Hover documentation.
//!
//! [`hover`] is project-wide and resolves the name under the cursor through
//! scopes, imports and the standard library; [`hover_info`] and
//! [`hover_usage_info`] are the per-file hovers of the language server.

use super::completion::short_doc;
use super::index::{ElemId, Src, Workspace};
use crate::model::{simple_name, Model};
use crate::sim::rollup::{evaluate_rollup, AggregationMethod};

/// Markdown for the name at bytes `start..end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hover {
    pub start: usize,
    pub end: usize,
    pub markdown: String,
}

/// Hover for the name at byte `offset` of file `src`.
pub fn hover(ws: &Workspace, src: Src, offset: usize) -> Option<Hover> {
    let (id, (start, end)) = ws.resolve_at(src, offset)?;
    Some(Hover {
        start,
        end,
        markdown: describe(ws, id),
    })
}

/// Markdown describing an element: its declaration, kind and qualified
/// name, doc, type and members.
pub fn describe(ws: &Workspace, id: ElemId) -> String {
    let e = ws.elem(id);
    let mut parts = vec![format!("```sysml\n{}\n```", ws.header(id))];

    let mut line = format!("**{}** `{}`", e.kind.label(), ws.qualified_name(id));
    if let Src::Library(f) = id.src {
        line.push_str(&format!(
            " — standard library, `{}`",
            ws.lib.files[f as usize].path
        ));
    }
    parts.push(line);

    if let Some(doc) = &e.doc {
        parts.push(doc.trim().to_owned());
    }

    let types = ws.types_of(id);
    if !e.kind.is_definition() {
        for &t in types.iter().take(2) {
            let te = ws.elem(t);
            let mut s = format!("Type: **{}** `{}`", te.kind.label(), ws.qualified_name(t));
            if let Some(doc) = &te.doc {
                if e.doc.is_none() {
                    s.push_str(&format!(" — {}", short_doc(doc)));
                }
            }
            parts.push(s);
        }
    }

    if e.kind.is_definition() && !e.kind.is_package() {
        let members: Vec<String> = ws
            .children(id)
            .map(|c| ws.elem(c))
            .filter(|m| !m.key.is_empty())
            .map(|m| match m.types.first() {
                Some(t) if !m.is_redefinition() => format!("`{}` : `{}`", m.name, t),
                _ => format!("`{}`", m.name),
            })
            .collect();
        if !members.is_empty() {
            let shown = members.len().min(12);
            let mut s = format!("**Members:** {}", members[..shown].join(", "));
            if members.len() > shown {
                s.push_str(&format!(", … ({} more)", members.len() - shown));
            }
            parts.push(s);
        }
    }
    parts.join("\n\n")
}

/// Hover for a standard-library symbol: kind, package, and doc.
fn stdlib_hover(name: &str) -> Option<String> {
    let simple = simple_name(name);
    // Prefer a package-qualified lookup when the reference names one
    for (pkg, defs) in crate::stdlib::stdlib_package_defs() {
        if let Some((ref_pkg, _)) = name.split_once("::") {
            if ref_pkg != pkg {
                continue;
            }
        }
        if let Some(def) = defs.iter().find(|d| d.name == simple) {
            let mut parts = vec![format!(
                "**{}** `{}` — `{}` (stdlib)",
                def.kind.label(),
                def.name,
                pkg
            )];
            if let Some(ref st) = def.super_type {
                parts[0].push_str(&format!(" : `{}`", st));
            }
            if let Some(ref doc) = def.doc {
                parts.push(String::new());
                parts.push(doc.clone());
            }
            return Some(parts.join("\n"));
        }
    }
    None
}

/// Markdown hover for a definition name in a single model.
pub fn hover_info(model: &Model, name: &str) -> Option<String> {
    let simple = simple_name(name);
    let Some(def) = model.find_def(simple) else {
        // Fall back to the embedded standard library (Real, ISQ::mass, ...)
        return stdlib_hover(name);
    };

    let mut parts = Vec::new();

    let mut header = format!("**{}** `{}`", def.kind.label(), def.name);
    if let Some(ref st) = def.super_type {
        header.push_str(&format!(" : `{}`", st));
    }
    parts.push(header);

    if let Some(ref doc) = def.doc {
        parts.push(String::new());
        parts.push(doc.clone());
    }

    // Members (usages in this def)
    let members = model.usages_in_def(&def.name);
    if !members.is_empty() {
        parts.push(String::new());
        parts.push("**Members:**".to_string());
        for u in &members {
            let mut member = format!("- `{}`", u.name);
            if let Some(ref tr) = u.type_ref {
                member.push_str(&format!(" : `{}`", tr));
            }
            if let Some(ref mult) = u.multiplicity {
                member.push_str(&format!(" `{}`", mult));
            }
            parts.push(member);
        }
    }

    // Computed rollups for numeric attributes (if this def has part children)
    let has_part_children = members.iter().any(|u| u.kind == "part" || u.kind == "item");
    if has_part_children {
        let mut rollups = Vec::new();
        let attr_names: Vec<String> = members
            .iter()
            .filter(|u| {
                matches!(u.kind.as_str(), "attribute" | "feature")
                    && u.value_expr
                        .as_ref()
                        .and_then(|e| e.trim().parse::<f64>().ok())
                        .is_some()
            })
            .map(|u| u.name.clone())
            .collect();
        for attr in &attr_names {
            let result = evaluate_rollup(model, &def.name, attr, AggregationMethod::Sum);
            if result.total != 0.0 && !result.contributions.is_empty() {
                rollups.push(format!("- `{}` = {:.4} (sum)", attr, result.total));
            }
        }
        if !rollups.is_empty() {
            parts.push(String::new());
            parts.push("**Rollups:**".to_string());
            parts.extend(rollups);
        }
    }

    Some(parts.join("\n"))
}

/// Markdown hover for a usage name in a single model.
pub fn hover_usage_info(model: &Model, name: &str) -> Option<String> {
    let usage = model.usages.iter().find(|u| u.name == name)?;

    let mut parts = Vec::new();
    let mut header = format!("**{}** `{}`", usage.kind, usage.name);
    if let Some(ref tr) = usage.type_ref {
        header.push_str(&format!(" : `{}`", tr));
    }
    parts.push(header);

    if let Some(ref parent) = usage.parent_def {
        parts.push(format!("In `{}`", parent));
    }

    Some(parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn hover_def_with_doc_and_members() {
        let source =
            "part def Vehicle {\n    doc /* A motor vehicle. */\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let text = hover_info(&model, "Vehicle").unwrap();
        assert!(text.contains("**part def**"), "{text}");
        assert!(text.contains("A motor vehicle"), "{text}");
        assert!(text.contains("`engine`"), "{text}");
        assert!(text.contains("`Engine`"), "{text}");
    }

    #[test]
    fn hover_shows_rollup() {
        let source = "part def Engine { attribute mass : Real = 180; }\npart def Vehicle {\n    attribute mass : Real = 50;\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let text = hover_info(&model, "Vehicle").unwrap();
        assert!(text.contains("Rollups:") && text.contains("230"), "{text}");
    }

    #[test]
    fn project_hover_shows_declaration_doc_and_type() {
        let a = "package A {\n    private import B::*;\n    part drone { part battery : Battery; }\n}\n";
        let b = "package B {\n    part def Battery {\n        doc /* Six-cell pack. */\n        attribute capacity;\n    }\n}\n";
        let ws = Workspace::from_sources([("a.sysml", a), ("b.sysml", b)]);
        let at = a.find("Battery;").unwrap() + 2;
        let h = hover(&ws, Src::Project(0), at).unwrap();
        assert_eq!(&a[h.start..h.end], "Battery");
        assert!(h.markdown.contains("part def Battery"), "{}", h.markdown);
        assert!(h.markdown.contains("`B::Battery`"), "{}", h.markdown);
        assert!(h.markdown.contains("Six-cell pack."), "{}", h.markdown);
        assert!(h.markdown.contains("`capacity`"), "{}", h.markdown);

        let usage = hover(&ws, Src::Project(0), a.find("battery").unwrap()).unwrap();
        assert!(
            usage.markdown.contains("part battery : Battery"),
            "{}",
            usage.markdown
        );
        assert!(usage
            .markdown
            .contains("Type: **part def** `B::Battery` — Six-cell pack."));
    }

    #[test]
    fn library_hover_names_the_library_file() {
        if crate::stdlib::stdlib_files().is_empty() {
            return;
        }
        let a = "part def P { attribute r : ScalarValues::Real; }";
        let ws = Workspace::from_sources([("a.sysml", a)]);
        let h = hover(&ws, Src::Project(0), a.find("Real").unwrap()).unwrap();
        assert!(h.markdown.contains("datatype Real"), "{}", h.markdown);
        assert!(h.markdown.contains("standard library"), "{}", h.markdown);
        assert!(h.markdown.contains("ScalarValues.kerml"), "{}", h.markdown);
    }
}
