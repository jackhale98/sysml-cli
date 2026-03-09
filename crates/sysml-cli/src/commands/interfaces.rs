use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf], unconnected_only: bool) -> ExitCode {
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
    }

    let ports = if unconnected_only {
        query::unconnected_ports(&merged)
    } else {
        query::list_ports(&merged)
    };

    if cli.format == "json" {
        let json: Vec<serde_json::Value> = ports
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "owner": p.owner,
                    "type": p.type_ref,
                    "direction": p.direction.map(|d| d.label()),
                    "conjugated": p.is_conjugated,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if ports.is_empty() {
            if unconnected_only {
                println!("All ports are connected.");
            } else {
                println!("No ports found.");
            }
            return ExitCode::SUCCESS;
        }

        let header = if unconnected_only {
            "Unconnected Ports:"
        } else {
            "Ports:"
        };
        println!("{}", header);
        println!(
            "  {:<15} {:<15} {:<15} {:<10}",
            "Name", "Owner", "Type", "Direction"
        );
        println!("  {}", "-".repeat(55));
        for p in &ports {
            let dir = p
                .direction
                .map(|d| d.label().to_string())
                .unwrap_or_else(|| "-".to_string());
            let t = p.type_ref.as_deref().unwrap_or("-");
            println!(
                "  {:<15} {:<15} {:<15} {:<10}",
                p.name, p.owner, t, dir
            );
        }
        if !cli.quiet {
            eprintln!("{} port(s) found.", ports.len());
        }
    }

    ExitCode::SUCCESS
}
