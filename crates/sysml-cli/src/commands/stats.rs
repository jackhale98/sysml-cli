use std::path::PathBuf;
use std::process::ExitCode;
use sysml_core::parser as sysml_parser;
use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf]) -> ExitCode {
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
        merged.imports.extend(model.imports);
        merged.comments.extend(model.comments);
    }
    let stats = query::model_stats(&merged);
    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
    } else {
        println!("Model Statistics");
        println!("{}", "=".repeat(40));
        println!("Definitions: {}", stats.total_definitions);
        println!("Usages:      {}", stats.total_usages);
        println!();
        if !stats.def_counts.is_empty() {
            println!("Definitions by kind:");
            for (kind, count) in &stats.def_counts {
                println!("  {:<20} {}", kind, count);
            }
            println!();
        }
        if !stats.usage_counts.is_empty() {
            println!("Usages by kind:");
            for (kind, count) in &stats.usage_counts {
                println!("  {:<20} {}", kind, count);
            }
            println!();
        }
        println!("Relationships:");
        println!("  Connections:    {}", stats.connection_count);
        println!("  Flows:          {}", stats.flow_count);
        println!("  Satisfactions:  {}", stats.satisfaction_count);
        println!("  Verifications:  {}", stats.verification_count);
        println!("  Allocations:    {}", stats.allocation_count);
        println!();
        println!("Packages:         {}", stats.package_count);
        println!("Abstract defs:    {}", stats.abstract_def_count);
        println!("Imports:          {}", stats.import_count);
        println!("Max nesting:      {}", stats.max_nesting_depth);
        println!();
        println!("Documentation:    {}/{} ({:.0}%)", stats.doc_coverage.documented, stats.doc_coverage.total, stats.doc_coverage.percentage);
    }
    ExitCode::SUCCESS
}
