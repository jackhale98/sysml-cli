//! Flags top-level public-visible definitions that lack a documentation comment.
//!
//! SysML v2 supports `doc /* ... */` blocks attached to any definition. For
//! large engineering models, missing documentation on public top-level
//! definitions makes the model harder for downstream readers and reviewers.
//!
//! Only fires for non-nested definitions of kinds that are typically the
//! "outward-facing" surface of a model: part, action, state, requirement,
//! interface, use case, port, item, calc, enumeration.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{DefKind, Model};

pub struct MissingDocCheck;

fn kind_is_documentable(kind: DefKind) -> bool {
    matches!(
        kind,
        DefKind::Part
            | DefKind::Action
            | DefKind::State
            | DefKind::Requirement
            | DefKind::Interface
            | DefKind::UseCase
            | DefKind::Port
            | DefKind::Item
            | DefKind::Calc
            | DefKind::Enum
            | DefKind::Constraint
    )
}

impl Check for MissingDocCheck {
    fn name(&self) -> &'static str {
        "missing-docs"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for def in &model.definitions {
            if def.parent_def.is_some() {
                continue;
            }
            if !kind_is_documentable(def.kind) {
                continue;
            }
            if def.doc.is_some() {
                continue;
            }
            diagnostics.push(
                Diagnostic::note(
                    &model.file,
                    def.span.clone(),
                    codes::MISSING_DOC,
                    format!(
                        "{} `{}` has no documentation comment",
                        def.kind.label(),
                        def.name
                    ),
                )
                .with_suggestion(format!(
                    "add a doc comment: `doc /* ... */` directly above `{} {} ...`",
                    def.kind.label(),
                    def.name
                )),
            );
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn documented_part_has_no_warning() {
        let source = r#"
            part def Vehicle {
                doc /* A car. */
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = MissingDocCheck.run(&model);
        assert!(
            diags.is_empty(),
            "documented part should not warn, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn undocumented_top_level_part_warns() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let diags = MissingDocCheck.run(&model);
        assert!(
            diags
                .iter()
                .any(|d| d.code == codes::MISSING_DOC && d.message.contains("Vehicle")),
            "undocumented part should warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nested_definition_does_not_warn() {
        let source = r#"
            part def Vehicle {
                doc /* Top */
                part def Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = MissingDocCheck.run(&model);
        let nested_warning = diags.iter().any(|d| d.message.contains("Engine"));
        assert!(
            !nested_warning,
            "nested undocumented part def should not warn (only top-level): {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignored_kinds_do_not_warn() {
        // Package is intentionally not a "documentable" kind here.
        let source = "package Sample;\n";
        let model = parse_file("test.sysml", source);
        let diags = MissingDocCheck.run(&model);
        assert!(
            diags.is_empty(),
            "packages are exempt from missing-doc check"
        );
    }

    #[test]
    fn requirement_def_without_doc_warns() {
        let source = "requirement def R1;\n";
        let model = parse_file("test.sysml", source);
        let diags = MissingDocCheck.run(&model);
        assert!(
            diags.iter().any(|d| d.message.contains("R1")),
            "requirement without doc should be flagged"
        );
    }
}
