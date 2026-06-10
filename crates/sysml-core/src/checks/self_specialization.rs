//! Flags definitions that name themselves as their own super-type
//! (e.g., `part def Vehicle :> Vehicle`).  This is always a typo / circular
//! reference and would cause infinite recursion during type resolution.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::Model;

pub struct SelfSpecializationCheck;

impl Check for SelfSpecializationCheck {
    fn name(&self) -> &'static str {
        "self-specialization"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for def in &model.definitions {
            if let Some(ref s) = def.super_type {
                // Only flag UNqualified self-references — a qualified name
                // like `ISQ::MassValue` may legitimately have the same simple
                // name as the local definition (stdlib aliasing pattern).
                if !s.contains("::") && s == &def.name {
                    diagnostics.push(
                        Diagnostic::error(
                            &model.file,
                            def.span.clone(),
                            codes::SELF_SPECIALIZATION,
                            format!(
                                "{} `{}` specializes itself: `:> {}` would cause infinite recursion",
                                def.kind.label(),
                                def.name,
                                s
                            ),
                        )
                        .with_suggestion(format!(
                            "remove the `:> {}` clause or rename either side",
                            s
                        )),
                    );
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn direct_self_specialization_errors() {
        let source = "part def Vehicle :> Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let diags = SelfSpecializationCheck.run(&model);
        assert!(
            diags
                .iter()
                .any(|d| d.code == codes::SELF_SPECIALIZATION && d.message.contains("Vehicle")),
            "self-specialization should error: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn legitimate_specialization_does_not_warn() {
        let source = r#"
            part def Vehicle;
            part def Car :> Vehicle;
        "#;
        let model = parse_file("test.sysml", source);
        let diags = SelfSpecializationCheck.run(&model);
        assert!(diags.is_empty(), "valid specialization should not warn");
    }

    #[test]
    fn qualified_alias_to_stdlib_does_not_warn() {
        // Common pattern: `attribute def MassValue :> ISQ::MassValue` —
        // legitimate stdlib re-export / specialization.
        let source = "attribute def MassValue :> ISQ::MassValue;\n";
        let model = parse_file("test.sysml", source);
        let diags = SelfSpecializationCheck.run(&model);
        assert!(
            diags.is_empty(),
            "qualified-name alias (not unqualified self) must not warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}
