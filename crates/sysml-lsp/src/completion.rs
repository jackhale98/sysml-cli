use sysml_core::model::{DefKind, Model};
use sysml_core::stdlib;
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use std::collections::HashSet;

use crate::state::DefLocation;

/// What kind of completion context we're in.
#[derive(Debug, PartialEq)]
pub enum CompletionFilter {
    /// No special context — show everything.
    All,
    /// After `:` in a type position — show only definitions (type names).
    TypePosition,
    /// After `:>` in a specialization — show only definitions of matching kind.
    Specialization,
    /// After `.` — show members of the preceding element.
    MemberAccess(String),
    /// After `Pkg::` — show members of that (stdlib or workspace) package.
    QualifiedAccess(String),
}

/// Determine completion context from the text before the cursor.
pub fn detect_context(source: &str, offset: usize) -> CompletionFilter {
    let before = &source[..offset.min(source.len())];
    let trimmed = before.trim_end();

    // Check for qualified access: "Pkg::" — scoped package members
    if let Some(prefix) = trimmed.strip_suffix("::") {
        let name: String = prefix
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if !name.is_empty() {
            return CompletionFilter::QualifiedAccess(name);
        }
    }

    // Check for member access: "something."
    if let Some(prefix) = trimmed.strip_suffix('.') {
        // Extract the name before the dot
        let name: String = prefix
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if !name.is_empty() {
            return CompletionFilter::MemberAccess(name);
        }
    }

    // Check for specialization: ":>" with optional whitespace
    if trimmed.ends_with(":>") || trimmed.ends_with(":> ") {
        return CompletionFilter::Specialization;
    }

    // Check for type position: ":" followed by optional whitespace
    // But not "::" (qualified name) or ":>" or ":=" (assignment)
    let last_non_ws = trimmed.as_bytes().last().copied();
    if last_non_ws == Some(b':') {
        let before_colon = &trimmed[..trimmed.len() - 1];
        let prev = before_colon.as_bytes().last().copied();
        if prev != Some(b':') && prev != Some(b'>') {
            return CompletionFilter::TypePosition;
        }
    }

    CompletionFilter::All
}

fn def_kind_to_completion_kind(kind: DefKind) -> CompletionItemKind {
    match kind {
        DefKind::Package | DefKind::Namespace => CompletionItemKind::MODULE,
        DefKind::Part | DefKind::Item | DefKind::Class | DefKind::Struct | DefKind::Datatype => {
            CompletionItemKind::CLASS
        }
        DefKind::Port | DefKind::Interface | DefKind::Connector => CompletionItemKind::INTERFACE,
        DefKind::Action | DefKind::Calc | DefKind::Function | DefKind::Behavior | DefKind::Step => {
            CompletionItemKind::FUNCTION
        }
        DefKind::State | DefKind::Enum => CompletionItemKind::ENUM,
        DefKind::Attribute | DefKind::Feature => CompletionItemKind::PROPERTY,
        DefKind::Requirement | DefKind::Concern | DefKind::Verification | DefKind::Analysis => {
            CompletionItemKind::EVENT
        }
        DefKind::Constraint | DefKind::Predicate => CompletionItemKind::OPERATOR,
        _ => CompletionItemKind::VALUE,
    }
}

/// Generate completion items from the current file, workspace defs, and stdlib.
/// `filter` restricts which items are returned based on cursor context.
pub fn completions(
    model: &Model,
    workspace_defs: &[DefLocation],
    filter: &CompletionFilter,
) -> Vec<CompletionItem> {
    // For member access, return members of the named element
    if let CompletionFilter::MemberAccess(ref parent) = filter {
        return member_completions(model, parent);
    }
    // For qualified access, return the package's members (stdlib first,
    // then workspace definitions inside a matching package).
    if let CompletionFilter::QualifiedAccess(ref pkg) = filter {
        return qualified_completions(model, workspace_defs, pkg);
    }
    let is_type_position = matches!(
        filter,
        CompletionFilter::TypePosition | CompletionFilter::Specialization
    );
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    // Current file definitions (in type position, skip packages)
    for def in &model.definitions {
        if is_type_position && def.kind == DefKind::Package {
            continue;
        }
        if seen.insert(def.name.clone()) {
            items.push(CompletionItem {
                label: def.name.clone(),
                kind: Some(def_kind_to_completion_kind(def.kind)),
                detail: Some(def.kind.label().to_string()),
                documentation: def
                    .doc
                    .as_ref()
                    .map(|d| tower_lsp_server::ls_types::Documentation::String(d.clone())),
                ..Default::default()
            });
        }
    }

    // Workspace definitions (cross-file) — skip packages in type position
    for loc in workspace_defs {
        if is_type_position && loc.kind == DefKind::Package {
            continue;
        }
        if seen.insert(loc.name.clone()) {
            items.push(CompletionItem {
                label: loc.name.clone(),
                kind: Some(def_kind_to_completion_kind(loc.kind)),
                detail: Some(loc.kind.label().to_string()),
                documentation: loc
                    .doc
                    .as_ref()
                    .map(|d| tower_lsp_server::ls_types::Documentation::String(d.clone())),
                ..Default::default()
            });
        }
    }

    // Standard library definitions — always available, ranked below
    // workspace items via sort_text so `attribute mass : ` offers Real,
    // MassValue, ISQ quantities, ... without drowning local types.
    for stdlib_model in stdlib::parse_stdlib() {
        for def in &stdlib_model.definitions {
            if is_type_position && def.kind == DefKind::Package {
                continue;
            }
            if seen.insert(def.name.clone()) {
                items.push(CompletionItem {
                    label: def.name.clone(),
                    kind: Some(def_kind_to_completion_kind(def.kind)),
                    detail: Some(format!("{} (stdlib)", def.kind.label())),
                    sort_text: Some(format!("1{}", def.name)),
                    documentation: def
                        .doc
                        .as_ref()
                        .map(|d| tower_lsp_server::ls_types::Documentation::String(d.clone())),
                    ..Default::default()
                });
            }
        }
    }

    // Package members that are usages in the stdlib source (KerML
    // `datatype Real`, `attribute mass`, ...) — exposed via the enriched
    // package index.
    for defs in stdlib::stdlib_package_defs().values() {
        for def in defs {
            if is_type_position && def.kind == DefKind::Package {
                continue;
            }
            if seen.insert(def.name.clone()) {
                items.push(CompletionItem {
                    label: def.name.clone(),
                    kind: Some(def_kind_to_completion_kind(def.kind)),
                    detail: Some(format!("{} (stdlib)", def.kind.label())),
                    sort_text: Some(format!("1{}", def.name)),
                    documentation: def
                        .doc
                        .as_ref()
                        .map(|d| tower_lsp_server::ls_types::Documentation::String(d.clone())),
                    ..Default::default()
                });
            }
        }
    }

    // Rank workspace/file items above stdlib
    for item in &mut items {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("0{}", item.label));
        }
    }

    items
}

/// Members of `Pkg::` — stdlib package members plus workspace definitions
/// whose enclosing package matches.
fn qualified_completions(
    model: &Model,
    workspace_defs: &[DefLocation],
    pkg: &str,
) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    if let Some(defs) = stdlib::stdlib_package_defs().get(pkg) {
        for def in defs {
            if seen.insert(def.name.clone()) {
                items.push(CompletionItem {
                    label: def.name.clone(),
                    kind: Some(def_kind_to_completion_kind(def.kind)),
                    detail: Some(format!("{} ({})", def.kind.label(), pkg)),
                    documentation: def
                        .doc
                        .as_ref()
                        .map(|d| tower_lsp_server::ls_types::Documentation::String(d.clone())),
                    ..Default::default()
                });
            }
        }
    }

    // Definitions in the current file under a package with this name
    for def in &model.definitions {
        if def
            .parent_def
            .as_deref()
            .map(sysml_core::model::unquote_name)
            == Some(pkg)
            && seen.insert(def.name.clone())
        {
            items.push(CompletionItem {
                label: def.name.clone(),
                kind: Some(def_kind_to_completion_kind(def.kind)),
                detail: Some(format!("{} ({})", def.kind.label(), pkg)),
                ..Default::default()
            });
        }
    }

    // Workspace definitions whose qualified name lives under the package
    for loc in workspace_defs {
        if let Some(ref qn) = loc.qualified_name {
            let in_pkg = qn
                .rsplit_once("::")
                .map(|(prefix, _)| prefix == pkg || prefix.ends_with(&format!("::{}", pkg)))
                .unwrap_or(false);
            if in_pkg && seen.insert(loc.name.clone()) {
                items.push(CompletionItem {
                    label: loc.name.clone(),
                    kind: Some(def_kind_to_completion_kind(loc.kind)),
                    detail: Some(format!("{} ({})", loc.kind.label(), pkg)),
                    ..Default::default()
                });
            }
        }
    }

    items
}

/// Generate completions for members of a named element.
fn member_completions(model: &Model, parent_name: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    // `drone.` where drone is a usage: enumerate the members of its TYPE
    // (feature chains are written through usage names in practice).
    let resolved: &str = model
        .usages
        .iter()
        .find(|u| u.name == parent_name)
        .and_then(|u| u.type_ref.as_deref())
        .map(sysml_core::model::simple_name)
        .unwrap_or(parent_name);
    let parent_name = resolved;
    // Find usages with this parent
    for usage in model.usages_in_def(parent_name) {
        items.push(CompletionItem {
            label: usage.name.clone(),
            kind: Some(match usage.kind.as_str() {
                "part" | "item" => CompletionItemKind::FIELD,
                "port" => CompletionItemKind::INTERFACE,
                "attribute" | "feature" => CompletionItemKind::PROPERTY,
                "action" | "calc" => CompletionItemKind::FUNCTION,
                _ => CompletionItemKind::VARIABLE,
            }),
            detail: usage.type_ref.clone(),
            ..Default::default()
        });
    }
    // Also check type_ref to show inherited members
    if let Some(def) = model.find_def(parent_name) {
        if let Some(ref st) = def.super_type {
            let st_name = sysml_core::model::simple_name(st);
            for usage in model.usages_in_def(st_name) {
                if !items.iter().any(|i| i.label == usage.name) {
                    items.push(CompletionItem {
                        label: usage.name.clone(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(format!(
                            "{} (inherited)",
                            usage.type_ref.as_deref().unwrap_or("")
                        )),
                        ..Default::default()
                    });
                }
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn completions_include_file_defs() {
        let source = "part def Vehicle;\npart def Engine;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[], &CompletionFilter::All);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"Vehicle"));
        assert!(names.contains(&"Engine"));
    }

    #[test]
    fn completions_kind_mapping() {
        let source = "part def Vehicle;\nport def PowerPort;\naction def Drive;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[], &CompletionFilter::All);

        let vehicle = items.iter().find(|i| i.label == "Vehicle").unwrap();
        assert_eq!(vehicle.kind, Some(CompletionItemKind::CLASS));

        let port = items.iter().find(|i| i.label == "PowerPort").unwrap();
        assert_eq!(port.kind, Some(CompletionItemKind::INTERFACE));

        let action = items.iter().find(|i| i.label == "Drive").unwrap();
        assert_eq!(action.kind, Some(CompletionItemKind::FUNCTION));
    }

    #[test]
    fn completions_include_stdlib() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded");
            return;
        }
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[], &CompletionFilter::All);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"ScalarValues"),
            "stdlib should provide ScalarValues, got: {:?}",
            names.iter().take(20).collect::<Vec<_>>()
        );
    }

    #[test]
    fn completions_dedup() {
        // If a name exists in both file and workspace, only one entry
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let workspace = vec![DefLocation {
            uri: "file:///other.sysml".to_string(),
            name: "Vehicle".to_string(),
            kind: DefKind::Part,
            span: sysml_core::model::Span::default(),
            doc: None,
            super_type: None,
            qualified_name: None,
        }];
        let items = completions(&model, &workspace, &CompletionFilter::All);
        let vehicle_count = items.iter().filter(|i| i.label == "Vehicle").count();
        assert_eq!(vehicle_count, 1, "should deduplicate");
    }

    // --- Context detection ---

    #[test]
    fn detect_type_position() {
        let source = "part engine : ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::TypePosition);
    }

    #[test]
    fn detect_specialization() {
        let source = "part def Sub :> ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::Specialization);
    }

    #[test]
    fn detect_member_access() {
        let source = "vehicle.";
        let ctx = detect_context(source, source.len());
        assert!(matches!(ctx, CompletionFilter::MemberAccess(ref n) if n == "vehicle"));
    }

    #[test]
    fn detect_normal_context() {
        let source = "part def ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::All);
    }

    #[test]
    fn type_position_excludes_packages() {
        let source = "package Pkg { part def Vehicle; }\n";
        let model = parse_file("test.sysml", source);
        let all = completions(&model, &[], &CompletionFilter::All);
        let typed = completions(&model, &[], &CompletionFilter::TypePosition);
        // All includes Pkg, typed does not
        assert!(all.iter().any(|i| i.label == "Pkg"));
        assert!(!typed.iter().any(|i| i.label == "Pkg"));
    }

    #[test]
    fn member_access_shows_children() {
        let source = "part def Vehicle { part engine : Engine; attribute mass : Real; }\n";
        let model = parse_file("test.sysml", source);
        let items = completions(
            &model,
            &[],
            &CompletionFilter::MemberAccess("Vehicle".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"engine"),
            "should include part member: {:?}",
            names
        );
        assert!(
            names.contains(&"mass"),
            "should include attribute member: {:?}",
            names
        );
    }

    #[test]
    fn detect_qualified_access() {
        let source = "attribute m :> ISQ::";
        let ctx = detect_context(source, source.len());
        assert!(matches!(ctx, CompletionFilter::QualifiedAccess(ref p) if p == "ISQ"));
    }

    #[test]
    fn qualified_access_lists_stdlib_members() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            return;
        }
        let model = parse_file("test.sysml", "part def X;\n");
        let items = completions(
            &model,
            &[],
            &CompletionFilter::QualifiedAccess("ISQ".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"mass"),
            "ISQ:: must offer mass, got {} items: {:?}",
            names.len(),
            names.iter().take(15).collect::<Vec<_>>()
        );
    }

    #[test]
    fn type_position_includes_stdlib_ranked_below() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            return;
        }
        let model = parse_file("test.sysml", "part def Vehicle;\n");
        let items = completions(&model, &[], &CompletionFilter::TypePosition);
        let real = items.iter().find(|i| i.label == "Real");
        assert!(real.is_some(), "type position must offer stdlib Real");
        let vehicle = items.iter().find(|i| i.label == "Vehicle").unwrap();
        assert!(
            vehicle.sort_text.as_deref() < real.unwrap().sort_text.as_deref(),
            "workspace types rank above stdlib"
        );
    }

    #[test]
    fn member_access_through_usage_name() {
        let source =
            "part def Drone { attribute mass : Real; }\npart def Fleet { part drone : Drone; }\n";
        let model = parse_file("test.sysml", source);
        let items = completions(
            &model,
            &[],
            &CompletionFilter::MemberAccess("drone".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"mass"),
            "usage-name member access must resolve through the type: {:?}",
            names
        );
    }
}
