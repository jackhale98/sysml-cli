//! Check for tree-sitter parse errors (ERROR / MISSING nodes).

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::Model;

pub struct SyntaxCheck;

impl Check for SyntaxCheck {
    fn name(&self) -> &'static str {
        "syntax"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        model
            .syntax_errors
            .iter()
            .map(|e| {
                let msg = if e.context.is_empty() {
                    e.message.clone()
                } else {
                    format!("{}: near `{}`", e.message, e.context)
                };
                Diagnostic::error(&model.file, e.span.clone(), codes::SYNTAX_ERROR, msg)
            })
            .collect()
    }
}
