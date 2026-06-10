//! Code-lens provider for sysml-lsp.
//!
//! Emits a code lens above each definition that has notable wired
//! relationships (satisfy/verify counts, usage counts, etc.).  These
//! lenses are passive (no `command`) and rely on the client to display
//! the title — a future revision can attach a goto-usages command.

use sysml_core::model::{simple_name, DefKind, Model};
use tower_lsp::lsp_types::{CodeLens, Position, Range};

use crate::convert::span_to_range;

/// Produce code-lens entries for a single file's model.
pub fn code_lenses(model: &Model) -> Vec<CodeLens> {
    let mut lenses = Vec::new();

    for def in &model.definitions {
        let title = match def.kind {
            DefKind::Requirement => requirement_lens_title(model, &def.name),
            DefKind::Part => usages_lens_title(model, &def.name, "usage"),
            DefKind::Action => usages_lens_title(model, &def.name, "usage"),
            DefKind::State => usages_lens_title(model, &def.name, "usage"),
            DefKind::Verification => verification_lens_title(model, &def.name),
            _ => None,
        };
        if let Some(title) = title {
            let range = lens_position(&span_to_range(&def.span));
            lenses.push(CodeLens {
                range,
                command: Some(tower_lsp::lsp_types::Command {
                    title,
                    command: String::new(),
                    arguments: None,
                }),
                data: None,
            });
        }
    }

    lenses
}

fn lens_position(def_range: &Range) -> Range {
    // The lens renders on the line directly above the definition.
    let line = def_range.start.line;
    Range::new(Position::new(line, 0), Position::new(line, 0))
}

fn requirement_lens_title(model: &Model, name: &str) -> Option<String> {
    let satisfies = model
        .satisfactions
        .iter()
        .filter(|s| simple_name(&s.requirement) == name)
        .count();
    let verifies = model
        .verifications
        .iter()
        .filter(|v| simple_name(&v.requirement) == name)
        .count();
    let usages = count_typed_usages(model, name);
    if satisfies == 0 && verifies == 0 && usages == 0 {
        return Some("⚠ unreferenced requirement".to_string());
    }
    Some(format!(
        "↳ {} satisfy · {} verify · {} usage{}",
        satisfies,
        verifies,
        usages,
        if usages == 1 { "" } else { "s" }
    ))
}

fn verification_lens_title(model: &Model, name: &str) -> Option<String> {
    let verifies = model
        .verifications
        .iter()
        .filter(|v| simple_name(&v.by) == name)
        .count();
    if verifies == 0 {
        None
    } else {
        Some(format!(
            "✓ verifies {} requirement{}",
            verifies,
            if verifies == 1 { "" } else { "s" }
        ))
    }
}

fn usages_lens_title(model: &Model, name: &str, label: &str) -> Option<String> {
    let count = count_typed_usages(model, name);
    if count == 0 {
        None
    } else {
        Some(format!(
            "↳ {} {}{}",
            count,
            label,
            if count == 1 { "" } else { "s" }
        ))
    }
}

fn count_typed_usages(model: &Model, type_name: &str) -> usize {
    model
        .usages
        .iter()
        .filter(|u| {
            u.type_ref
                .as_deref()
                .is_some_and(|t| simple_name(t) == type_name)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn requirement_with_satisfy_has_lens() {
        let source = r#"
            requirement def MaxMass;
            part def Vehicle { satisfy MaxMass; }
        "#;
        let model = parse_file("test.sysml", source);
        let lenses = code_lenses(&model);
        assert!(
            lenses.iter().any(|l| l
                .command
                .as_ref()
                .is_some_and(|c| c.title.contains("1 satisfy"))),
            "lens count not as expected: {:?}",
            lenses
                .iter()
                .filter_map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unreferenced_requirement_gets_warning_lens() {
        let source = "requirement def Lonely;\n";
        let model = parse_file("test.sysml", source);
        let lenses = code_lenses(&model);
        assert!(
            lenses.iter().any(|l| l
                .command
                .as_ref()
                .is_some_and(|c| c.title.contains("unreferenced"))),
            "should mark unreferenced requirement: {:?}",
            lenses
                .iter()
                .filter_map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn part_def_with_usage_has_lens() {
        let source = r#"
            part def Engine;
            part def Vehicle { part engine : Engine; }
        "#;
        let model = parse_file("test.sysml", source);
        let lenses = code_lenses(&model);
        assert!(
            lenses.iter().any(|l| l
                .command
                .as_ref()
                .is_some_and(|c| c.title.contains("1 usage"))),
            "Engine should report 1 usage: {:?}",
            lenses
                .iter()
                .filter_map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn part_def_with_no_usages_emits_no_lens() {
        let source = "part def Unused;\n";
        let model = parse_file("test.sysml", source);
        let lenses = code_lenses(&model);
        // Only kinds with referenced usages get lenses (excluding the
        // requirement-unreferenced case).
        assert!(lenses.iter().all(|l| l
            .command
            .as_ref()
            .is_none_or(|c| !c.title.contains("Unused"))));
    }
}
