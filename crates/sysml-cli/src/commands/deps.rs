use std::path::PathBuf;
use std::process::ExitCode;
use sysml_core::parser as sysml_parser;
use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf], target: &str, reverse_only: bool, forward_only: bool) -> ExitCode {
    use sysml_core::query;
    let mut merged = sysml_core::model::Model::new("(merged)".to_string());
    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        merged.definitions.extend(model.definitions);
        merged.usages.extend(model.usages);
        merged.connections.extend(model.connections);
        merged.flows.extend(model.flows);
        merged.satisfactions.extend(model.satisfactions);
        merged.verifications.extend(model.verifications);
        merged.allocations.extend(model.allocations);
    }

    // Verify target exists
    let target_exists = merged.definitions.iter().any(|d| d.name == target)
        || merged.usages.iter().any(|u| u.name == target);
    if !target_exists {
        eprintln!("error: element `{}` not found", target);
        let available: Vec<&str> = merged.definitions.iter().map(|d| d.name.as_str()).collect();
        if !available.is_empty() {
            eprintln!("  available definitions: {}", available.join(", "));
        }
        return ExitCode::from(1);
    }

    let deps = query::dependency_analysis(&merged, target);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&deps).unwrap());
    } else {
        println!("Dependency Analysis: {}", target);
        println!("{}", "=".repeat(40));

        if !forward_only {
            println!();
            println!("Referenced by ({}):", deps.referenced_by.len());
            if deps.referenced_by.is_empty() {
                println!("  (none)");
            } else {
                for r in &deps.referenced_by {
                    println!("  {} ({}) via {}", r.name, r.kind, r.relationship);
                }
            }
        }

        if !reverse_only {
            println!();
            println!("Depends on ({}):", deps.depends_on.len());
            if deps.depends_on.is_empty() {
                println!("  (none)");
            } else {
                for r in &deps.depends_on {
                    println!("  {} ({}) via {}", r.name, r.kind, r.relationship);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
