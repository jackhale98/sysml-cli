/// Check for circular import dependencies.

use std::collections::{HashMap, HashSet};

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, Model};

pub struct ImportCycleCheck;

impl Check for ImportCycleCheck {
    fn name(&self) -> &'static str {
        "import-cycles"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        // Build a graph: package → set of imported package names
        let mut package_imports: HashMap<String, Vec<String>> = HashMap::new();
        let mut current_package = String::new();

        for def in &model.definitions {
            if def.kind == crate::model::DefKind::Package {
                current_package = def.name.clone();
            }
        }

        // Map imports to their source package context
        for import in &model.imports {
            // The imported path's first segment is the target package
            let target_pkg = simple_name(&import.path);
            if !target_pkg.is_empty() && target_pkg != current_package {
                package_imports
                    .entry(current_package.clone())
                    .or_default()
                    .push(target_pkg.to_string());
            }
        }

        let mut diagnostics = Vec::new();

        // Check for self-imports: a package importing its own namespace
        for import in &model.imports {
            let target = import.path.split("::").next().unwrap_or("");
            // Find the enclosing package of this import (by span containment)
            for def in &model.definitions {
                if def.kind == crate::model::DefKind::Package
                    && def.span.contains(&import.span)
                    && def.name == target
                {
                    diagnostics.push(Diagnostic::warning(
                        &model.file,
                        import.span.clone(),
                        codes::IMPORT_CYCLE,
                        format!(
                            "package `{}` imports itself (via `{}`)",
                            target, import.path
                        ),
                    ));
                }
            }
        }

        // Check for A→B→A cycles within the same file
        for (pkg, targets) in &package_imports {
            for target in targets {
                if let Some(back_imports) = package_imports.get(target) {
                    if back_imports.contains(pkg) {
                        diagnostics.push(Diagnostic::warning(
                            &model.file,
                            crate::model::Span::default(),
                            codes::IMPORT_CYCLE,
                            format!(
                                "circular import: `{}` imports `{}` which imports `{}`",
                                pkg, target, pkg
                            ),
                        ));
                    }
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
    fn no_cycles_in_normal_model() {
        let source = r#"
            package A { import B::*; }
            package B { part def X; }
        "#;
        let model = parse_file("test.sysml", source);
        let check = ImportCycleCheck;
        let diags = check.run(&model);
        let cycles: Vec<_> = diags.iter().filter(|d| d.code == codes::IMPORT_CYCLE).collect();
        assert!(cycles.is_empty());
    }

    #[test]
    fn detects_self_import() {
        let source = r#"
            package A {
                import A::*;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let check = ImportCycleCheck;
        let diags = check.run(&model);
        let cycles: Vec<_> = diags.iter().filter(|d| d.code == codes::IMPORT_CYCLE).collect();
        assert!(!cycles.is_empty(), "should detect self-import");
    }
}
