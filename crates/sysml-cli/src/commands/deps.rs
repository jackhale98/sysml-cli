use crate::Cli;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn run(
    cli: &Cli,
    files: &[PathBuf],
    target: &str,
    reverse_only: bool,
    forward_only: bool,
    transitive: bool,
) -> ExitCode {
    use sysml_core::query;
    let Some(merged) = crate::load_model(cli, files) else {
        return ExitCode::FAILURE;
    };

    let target_exists = merged.definitions.iter().any(|d| d.name == target)
        || merged.usages.iter().any(|u| u.name == target);
    if !target_exists {
        eprintln!("error: element `{}` not found", target);
        return ExitCode::from(1);
    }

    let deps = query::dependency_analysis(&merged, target);

    // For transitive deps, follow chains
    let (referenced_by, depends_on) = if transitive {
        let mut all_refs = deps.referenced_by.clone();
        let mut all_deps = deps.depends_on.clone();
        let mut visited = HashSet::new();
        visited.insert(target.to_string());

        // Transitive forward: follow depends_on chains
        let mut queue: Vec<String> = deps.depends_on.iter().map(|d| d.name.clone()).collect();
        while let Some(next) = queue.pop() {
            if visited.insert(next.clone()) {
                let sub = query::dependency_analysis(&merged, &next);
                for d in &sub.depends_on {
                    if !visited.contains(&d.name) {
                        queue.push(d.name.clone());
                    }
                    if !all_deps.iter().any(|x| x.name == d.name) {
                        all_deps.push(d.clone());
                    }
                }
            }
        }

        // Transitive reverse: follow referenced_by chains
        visited.clear();
        visited.insert(target.to_string());
        let mut queue: Vec<String> = deps.referenced_by.iter().map(|d| d.name.clone()).collect();
        while let Some(next) = queue.pop() {
            if visited.insert(next.clone()) {
                let sub = query::dependency_analysis(&merged, &next);
                for r in &sub.referenced_by {
                    if !visited.contains(&r.name) {
                        queue.push(r.name.clone());
                    }
                    if !all_refs.iter().any(|x| x.name == r.name) {
                        all_refs.push(r.clone());
                    }
                }
            }
        }

        (all_refs, all_deps)
    } else {
        (deps.referenced_by, deps.depends_on)
    };

    if cli.format == "json" {
        let json = serde_json::json!({
            "target": target,
            "transitive": transitive,
            "referenced_by": referenced_by.iter().map(|r| serde_json::json!({
                "name": r.name, "kind": r.kind, "relationship": r.relationship,
            })).collect::<Vec<_>>(),
            "depends_on": depends_on.iter().map(|r| serde_json::json!({
                "name": r.name, "kind": r.kind, "relationship": r.relationship,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        let label = if transitive { " (transitive)" } else { "" };
        println!("Dependency Analysis: {}{}", target, label);
        println!("{}", "=".repeat(40));

        if !forward_only {
            println!();
            println!("Referenced by ({}):", referenced_by.len());
            if referenced_by.is_empty() {
                println!("  (none)");
            } else {
                for r in &referenced_by {
                    println!("  {} ({}) via {}", r.name, r.kind, r.relationship);
                }
            }
        }

        if !reverse_only {
            println!();
            println!("Depends on ({}):", depends_on.len());
            if depends_on.is_empty() {
                println!("  (none)");
            } else {
                for r in &depends_on {
                    println!("  {} ({}) via {}", r.name, r.kind, r.relationship);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
