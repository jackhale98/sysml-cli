//! Document outline.

use serde::Serialize;

use super::index::{ElemId, Src, Workspace};
use crate::model::{DefKind, Model, Span};

/// One outline entry. Byte offsets: the whole element (`start..end`) and
/// its name (`selection_start..selection_end`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub name: String,
    /// `part def`, `attribute`, `package`, ...
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub start: usize,
    pub end: usize,
    pub selection_start: usize,
    pub selection_end: usize,
    pub children: Vec<OutlineItem>,
}

/// The nested outline of a project file.
pub fn outline(ws: &Workspace, src: Src) -> Vec<OutlineItem> {
    let file = ws.file(src);
    let mut out = Vec::new();
    for &r in &file.roots {
        push_item(ws, ElemId { src, elem: r }, &mut out);
    }
    out
}

fn push_item(ws: &Workspace, id: ElemId, out: &mut Vec<OutlineItem>) {
    let e = ws.elem(id);
    let mut children = Vec::new();
    for c in ws.children(id) {
        push_item(ws, c, &mut children);
    }
    if e.key.is_empty() {
        // Anonymous elements: show their named members in their place.
        out.extend(children);
        return;
    }
    let (selection_start, selection_end) = e.name_span.unwrap_or((e.start, e.start));
    let detail = if e.is_redefinition() {
        Some(format!(":>> {}", e.key))
    } else {
        e.types.first().cloned()
    };
    out.push(OutlineItem {
        name: e.key.clone(),
        kind: e.kind.label(),
        detail,
        start: e.start,
        end: e.end,
        selection_start,
        selection_end,
        children,
    });
}

/// What a language-server symbol is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolOrigin {
    Def(DefKind),
    /// A usage with this keyword.
    Usage(String),
}

/// A per-model document symbol (language server).
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub detail: Option<String>,
    pub origin: SymbolOrigin,
    pub span: Span,
    pub children: Vec<Symbol>,
}

/// Hierarchical symbols of a model: top-level definitions with their child
/// definitions and usages nested by `parent_def`.
pub fn document_symbols(model: &Model) -> Vec<Symbol> {
    model
        .definitions
        .iter()
        .filter(|d| d.parent_def.is_none())
        .map(|def| Symbol {
            name: def.name.clone(),
            detail: def.super_type.clone(),
            origin: SymbolOrigin::Def(def.kind),
            span: def.span.clone(),
            children: build_children(model, &def.name),
        })
        .collect()
}

fn build_children(model: &Model, parent_name: &str) -> Vec<Symbol> {
    let defs = model
        .definitions
        .iter()
        .filter(|d| d.parent_def.as_deref() == Some(parent_name))
        .map(|def| Symbol {
            name: def.name.clone(),
            detail: def.super_type.clone(),
            origin: SymbolOrigin::Def(def.kind),
            span: def.span.clone(),
            children: build_children(model, &def.name),
        });
    let usages = model
        .usages
        .iter()
        .filter(|u| u.parent_def.as_deref() == Some(parent_name))
        .map(|usage| Symbol {
            name: usage.name.clone(),
            detail: usage.type_ref.clone(),
            origin: SymbolOrigin::Usage(usage.kind.clone()),
            span: usage.span.clone(),
            children: Vec::new(),
        });
    defs.chain(usages).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn model_symbols_nest() {
        let source =
            "package Vehicles {\n    part def Vehicle {\n        part engine : Engine;\n    }\n}\n";
        let symbols = document_symbols(&parse_file("test.sysml", source));
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].children[0].name, "Vehicle");
        let engine = &symbols[0].children[0].children[0];
        assert_eq!(engine.name, "engine");
        assert_eq!(engine.detail.as_deref(), Some("Engine"));
        assert_eq!(engine.origin, SymbolOrigin::Usage("part".into()));
    }

    #[test]
    fn outline_nests_with_name_ranges() {
        let text = "package P {\n    part def 'Battery pack' :> Base {\n        attribute :>> mass = 1;\n    }\n    enum def E { enum a; }\n}\n";
        let ws = Workspace::from_sources([("a.sysml", text)]);
        let items = outline(&ws, Src::Project(0));
        assert_eq!(items.len(), 1);
        let p = &items[0];
        assert_eq!((p.name.as_str(), p.kind.as_str()), ("P", "package"));
        let bp = &p.children[0];
        assert_eq!(bp.name, "Battery pack");
        assert_eq!(bp.kind, "part def");
        assert_eq!(bp.detail.as_deref(), Some("Base"));
        assert_eq!(
            &text[bp.selection_start..bp.selection_end],
            "'Battery pack'"
        );
        assert_eq!(bp.children[0].detail.as_deref(), Some(":>> mass"));
        assert_eq!(p.children[1].children[0].name, "a");
    }
}
