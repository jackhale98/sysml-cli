/// Check for circular import dependencies.

use std::collections::{HashMap, HashSet};

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{DefKind, Model};

pub struct ImportCycleCheck;

impl Check for ImportCycleCheck {
    fn name(&self) -> &'static str {
        "import-cycles"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Build package containment: which packages are defined in this file
        let packages: HashSet<&str> = model
            .definitions
            .iter()
            .filter(|d| d.kind == DefKind::Package)
            .map(|d| d.name.as_str())
            .collect();

        // Build import graph: package → imported package names
        let mut package_imports: HashMap<&str, Vec<&str>> = HashMap::new();

        for import in &model.imports {
            let target = import.path.split("::").next().unwrap_or("");
            if target.is_empty() {
                continue;
            }

            // Find the enclosing package of this import
            let enclosing = model
                .definitions
                .iter()
                .filter(|d| d.kind == DefKind::Package && d.span.contains(&import.span))
                .last()
                .map(|d| d.name.as_str())
                .unwrap_or("");

            // Self-import: package imports itself
            if !enclosing.is_empty() && enclosing == target {
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

            if !enclosing.is_empty() {
                package_imports
                    .entry(enclosing)
                    .or_default()
                    .push(target);
            }
        }

        // Check for A→B→A cycles within the file
        for (&pkg, targets) in &package_imports {
            for &target in targets {
                if target == pkg {
                    continue; // self-import already reported
                }
                // Check if target imports pkg back
                if let Some(back) = package_imports.get(target) {
                    if back.contains(&pkg) {
                        // Only report once (alphabetical order)
                        if pkg < target {
                            diagnostics.push(Diagnostic::warning(
                                &model.file,
                                crate::model::Span::default(),
                                codes::IMPORT_CYCLE,
                                format!(
                                    "circular import between `{}` and `{}`",
                                    pkg, target
                                ),
                            ));
                        }
                    }
                }
            }
        }

        // Check for longer cycles (A→B→C→A) using DFS
        for &start in packages.iter() {
            if has_cycle(&package_imports, start) {
                // Only report if not already reported as a 2-cycle
                let already = diagnostics.iter().any(|d| {
                    d.code == codes::IMPORT_CYCLE && d.message.contains(start)
                });
                if !already {
                    diagnostics.push(Diagnostic::warning(
                        &model.file,
                        crate::model::Span::default(),
                        codes::IMPORT_CYCLE,
                        format!("package `{}` is part of a circular import chain", start),
                    ));
                }
            }
        }

        diagnostics
    }
}

/// DFS cycle detection: does `start` eventually reach itself?
fn has_cycle(graph: &HashMap<&str, Vec<&str>>, start: &str) -> bool {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    visited.insert(start);

    while let Some(current) = stack.pop() {
        if let Some(targets) = graph.get(current) {
            for &target in targets {
                if target == start && current != start {
                    return true;
                }
                if visited.insert(target) {
                    stack.push(target);
                }
            }
        }
    }

    false
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
        assert!(cycles.is_empty(), "got: {:?}", cycles.iter().map(|d| &d.message).collect::<Vec<_>>());
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

    #[test]
    fn detects_bidirectional_cycle() {
        let source = r#"
            package A {
                import B::*;
            }
            package B {
                import A::*;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let check = ImportCycleCheck;
        let diags = check.run(&model);
        let cycles: Vec<_> = diags.iter().filter(|d| d.code == codes::IMPORT_CYCLE).collect();
        assert!(!cycles.is_empty(), "should detect A→B→A cycle");
    }
}
