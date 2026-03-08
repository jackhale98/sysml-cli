/// Checks for constraint definitions missing constraint expressions.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{DefKind, Model};

pub struct ConstraintCheck;

impl Check for ConstraintCheck {
    fn name(&self) -> &'static str {
        "constraints"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            if def.kind != DefKind::Constraint {
                continue;
            }

            // Only check constraint defs that have a body block
            if !def.has_body {
                continue;
            }

            // A constraint def with a body but no expression is likely incomplete
            if !def.has_constraint_expr {
                diagnostics.push(Diagnostic::warning(
                    &model.file,
                    def.span.clone(),
                    codes::EMPTY_CONSTRAINT,
                    format!(
                        "constraint def `{}` has a body but no constraint expression",
                        def.name,
                    ),
                ));
            }
        }

        diagnostics
    }
}
