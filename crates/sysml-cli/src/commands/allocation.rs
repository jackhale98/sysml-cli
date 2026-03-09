use std::path::PathBuf;
use std::process::ExitCode;
use sysml_core::parser as sysml_parser;
use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf], check: bool, unallocated_only: bool) -> ExitCode {
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
        merged.allocations.extend(model.allocations);
    }

    let report = query::allocation_report(&merged);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        if !unallocated_only {
            if report.rows.is_empty() {
                println!("No allocations found.");
            } else {
                println!("{:<25} {:<25}", "Source (Logical)", "Target (Physical)");
                println!("{}", "-".repeat(50));
                for row in &report.rows {
                    println!("{:<25} {:<25}", row.source, row.target);
                }
                println!();
                println!("Total allocations: {}", report.total_allocations);
            }
        }

        if !report.unallocated_sources.is_empty() {
            println!();
            println!("Unallocated actions/use-cases:");
            for name in &report.unallocated_sources {
                println!("  {}", name);
            }
        }
        if !report.unallocated_targets.is_empty() {
            println!();
            println!("Unallocated parts:");
            for name in &report.unallocated_targets {
                println!("  {}", name);
            }
        }
    }

    if check && (!report.unallocated_sources.is_empty() || !report.unallocated_targets.is_empty()) {
        eprintln!(
            "error: {} unallocated element(s) found",
            report.unallocated_sources.len() + report.unallocated_targets.len()
        );
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
