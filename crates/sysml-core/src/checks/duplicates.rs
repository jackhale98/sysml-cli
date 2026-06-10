//! Check for duplicate definition names within the same file.

use std::collections::HashMap;

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::Model;

pub struct DuplicateCheck;

impl Check for DuplicateCheck {
    fn name(&self) -> &'static str {
        "duplicates"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut seen: HashMap<(&str, &str), &crate::model::Span> = HashMap::new();
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            let key = (def.kind.label(), def.name.as_str());
            if let Some(first_span) = seen.get(&key) {
                diagnostics.push(Diagnostic::error(
                    &model.file,
                    def.span.clone(),
                    codes::DUPLICATE_DEF,
                    format!(
                        "duplicate {} `{}` (first defined at line {})",
                        def.kind.label(),
                        def.name,
                        first_span.start_row,
                    ),
                ));
            } else {
                seen.insert(key, &def.span);
            }
        }

        diagnostics
    }
}
