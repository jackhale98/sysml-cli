/// Checks for calculation definitions missing return statements.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{DefKind, Model};

pub struct CalcReturnCheck;

impl Check for CalcReturnCheck {
    fn name(&self) -> &'static str {
        "calculations"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            if def.kind != DefKind::Calc {
                continue;
            }

            // Only check calc defs that have a body block
            if !def.has_body {
                continue;
            }

            // A calc def with a body but no return statement is likely incomplete
            if !def.has_return {
                diagnostics.push(Diagnostic::warning(
                    &model.file,
                    def.span.clone(),
                    codes::CALC_NO_RETURN,
                    format!(
                        "calc def `{}` has a body but no return statement",
                        def.name,
                    ),
                ));
            }
        }

        diagnostics
    }
}
