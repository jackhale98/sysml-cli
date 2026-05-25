/// Enforces canonical SysML v2 naming conventions:
///   - Definition names start with an uppercase letter (`PartDef`, `Vehicle`).
///   - Usage names start with a lowercase letter (`myCar`, `engine`).
///   - Package names start with an uppercase letter.
///
/// Skipped for names containing only digits, only underscores, or short
/// names like single-letter generics (`T`). Also skipped when the first
/// character is a single underscore (a convention for "intentionally
/// anonymous").

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{DefKind, Model};

pub struct NamingConventionCheck;

fn first_alpha(name: &str) -> Option<char> {
    name.chars().find(|c| c.is_alphabetic())
}

fn is_intentionally_skipped(name: &str) -> bool {
    name.starts_with('_')
        || name.is_empty()
        || name.chars().all(|c| !c.is_alphabetic())
}

impl Check for NamingConventionCheck {
    fn name(&self) -> &'static str {
        "naming"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            if is_intentionally_skipped(&def.name) {
                continue;
            }
            // Packages and all defs: PascalCase (starts uppercase).
            if let Some(c) = first_alpha(&def.name) {
                if !c.is_uppercase() {
                    diagnostics.push(
                        Diagnostic::note(
                            &model.file,
                            def.span.clone(),
                            codes::NAMING_CONVENTION,
                            format!(
                                "{} name `{}` should start with an uppercase letter (PascalCase)",
                                def.kind.label(),
                                def.name
                            ),
                        )
                        .with_suggestion(format!(
                            "rename to `{}`",
                            capitalize(&def.name)
                        )),
                    );
                }
            }
        }

        for u in &model.usages {
            if is_intentionally_skipped(&u.name) {
                continue;
            }
            // Skip if name is also a known definition (shadowing is fine).
            if model.find_def(&u.name).is_some() {
                continue;
            }
            // Skip parameters of constraint/calc defs — those follow their
            // own conventions (often single-letter).
            if let Some(parent_name) = &u.parent_def {
                if let Some(parent) = model.find_def(parent_name) {
                    if matches!(parent.kind, DefKind::Constraint | DefKind::Calc) {
                        continue;
                    }
                }
            }
            if let Some(c) = first_alpha(&u.name) {
                if !c.is_lowercase() {
                    diagnostics.push(
                        Diagnostic::note(
                            &model.file,
                            u.span.clone(),
                            codes::NAMING_CONVENTION,
                            format!(
                                "usage `{}` should start with a lowercase letter (camelCase)",
                                u.name
                            ),
                        )
                        .with_suggestion(format!(
                            "rename to `{}`",
                            decapitalize(&u.name)
                        )),
                    );
                }
            }
        }
        diagnostics
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn decapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn well_named_part_does_not_warn() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let diags = NamingConventionCheck.run(&model);
        assert!(
            diags.is_empty(),
            "well-named def should not warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lowercase_def_warns() {
        let source = "part def vehicle;\n";
        let model = parse_file("test.sysml", source);
        let diags = NamingConventionCheck.run(&model);
        assert!(
            diags.iter().any(|d| d.code == codes::NAMING_CONVENTION
                && d.message.contains("vehicle")),
            "lowercase definition name should warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn uppercase_usage_warns() {
        let source = r#"
            part def Car {
                part Engine : Engine;
            }
            part def Engine;
        "#;
        let model = parse_file("test.sysml", source);
        let diags = NamingConventionCheck.run(&model);
        // Engine usage is named Engine (matches a def name) — should NOT warn
        // because shadowing exception applies.
        assert!(
            diags.iter().all(|d| !d.message.contains("Engine`")),
            "usage name that matches a def name should be exempt"
        );
    }

    #[test]
    fn truly_uppercase_usage_warns() {
        let source = r#"
            part def Vehicle {
                part Wheels : Wheel;
            }
            part def Wheel;
        "#;
        let model = parse_file("test.sysml", source);
        let diags = NamingConventionCheck.run(&model);
        assert!(
            diags.iter().any(|d| d.code == codes::NAMING_CONVENTION
                && d.message.contains("Wheels")
                && d.message.contains("lowercase")),
            "uppercase usage name (not shadowing a def) should warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn underscore_names_are_skipped () {
        let source = "part def _Anonymous;\n";
        let model = parse_file("test.sysml", source);
        let diags = NamingConventionCheck.run(&model);
        assert!(
            diags.is_empty(),
            "underscore-prefixed names are exempt from convention check"
        );
    }
}
