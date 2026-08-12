use crate::Cli;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn run(cli: &Cli, files: &[PathBuf]) -> ExitCode {
    use sysml_core::query;
    let Some(merged) = crate::load_model(cli, files) else {
        return ExitCode::FAILURE;
    };
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
        println!(
            "Documentation:    {}/{} ({:.0}%)",
            stats.doc_coverage.documented, stats.doc_coverage.total, stats.doc_coverage.percentage
        );
    }
    ExitCode::SUCCESS
}
