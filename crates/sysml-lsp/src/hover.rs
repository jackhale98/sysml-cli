use sysml_core::model::{simple_name, Model};

/// Hover for a standard-library symbol: kind, package, and doc.
fn stdlib_hover(name: &str) -> Option<String> {
    let simple = simple_name(name);
    // Prefer a package-qualified lookup when the reference names one
    for (pkg, defs) in sysml_core::stdlib::stdlib_package_defs() {
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

use sysml_core::sim::rollup::{evaluate_rollup, AggregationMethod};

/// Build a markdown hover string for a definition name in the model.
pub fn hover_info(model: &Model, name: &str) -> Option<String> {
    let simple = simple_name(name);
    let Some(def) = model.find_def(simple) else {
        // Fall back to the embedded standard library (Real, ISQ::mass, ...)
        return stdlib_hover(name);
    };

    let mut parts = Vec::new();

    // Header: kind + name + supertype
    let mut header = format!("**{}** `{}`", def.kind.label(), def.name);
    if let Some(ref st) = def.super_type {
        header.push_str(&format!(" : `{}`", st));
    }
    parts.push(header);

    // Doc comment
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
        // Find numeric attribute names on this def and its children
        let attr_names: Vec<String> = members
            .iter()
            .filter(|u| {
                matches!(u.kind.as_str(), "attribute" | "feature") && u.value_expr.is_some()
            })
            .filter(|u| {
                u.value_expr
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

/// Build a markdown hover string for a usage name in the model.
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
    use sysml_core::parser::parse_file;

    #[test]
    fn hover_def_with_doc_and_members() {
        let source =
            "part def Vehicle {\n    doc /* A motor vehicle. */\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Vehicle");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains("**part def**"), "should show kind: {}", text);
        assert!(text.contains("`Vehicle`"), "should show name: {}", text);
        assert!(
            text.contains("A motor vehicle"),
            "should show doc: {}",
            text
        );
        assert!(text.contains("`engine`"), "should show member: {}", text);
        assert!(
            text.contains("`Engine`"),
            "should show member type: {}",
            text
        );
    }

    #[test]
    fn hover_def_with_supertype() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Sub");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains(": `Base`"), "should show supertype: {}", text);
    }

    #[test]
    fn hover_usage() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let info = hover_usage_info(&model, "engine");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains("**part**"), "should show kind: {}", text);
        assert!(text.contains("`Engine`"), "should show type ref: {}", text);
        assert!(
            text.contains("In `Vehicle`"),
            "should show parent: {}",
            text
        );
    }

    #[test]
    fn hover_unknown_returns_none() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        assert!(hover_info(&model, "Unknown").is_none());
    }

    #[test]
    fn hover_shows_rollup() {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Vehicle {
                attribute mass : Real = 50;
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Vehicle");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(
            text.contains("Rollups:"),
            "should show rollup section: {}",
            text
        );
        assert!(
            text.contains("230"),
            "should show total 230 (50+180): {}",
            text
        );
    }
}
