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
        let mut seen: HashMap<(&str, Option<&str>, &str), &crate::model::Span> = HashMap::new();
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            // Scope by enclosing package/definition: the same name in two
            // different packages is not a duplicate.
            let key = (
                def.kind.label(),
                def.parent_def.as_deref(),
                def.name.as_str(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn same_name_in_different_packages_is_not_duplicate() {
        let source = r#"
            package WhileLoop { action def FindTarget; }
            package UntilLoop { action def FindTarget; }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = DuplicateCheck.run(&model);
        assert!(
            diags.is_empty(),
            "defs in different packages should not be duplicates: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn same_name_in_same_package_is_duplicate() {
        let source = r#"
            package P {
                action def FindTarget;
                action def FindTarget;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = DuplicateCheck.run(&model);
        assert_eq!(diags.len(), 1, "expected one duplicate diagnostic");
        assert_eq!(diags[0].code, codes::DUPLICATE_DEF);
    }
}
