use std::path::PathBuf;
use std::process::ExitCode;
use sysml_core::parser as sysml_parser;
use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf], check: bool, min_score: f64) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }

    use sysml_core::query;
    let mut merged = sysml_core::model::Model::new("(merged)".to_string());
    for file_path in &files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        merged.definitions.extend(model.definitions);
        merged.usages.extend(model.usages);
        merged.satisfactions.extend(model.satisfactions);
        merged.verifications.extend(model.verifications);
    }

    let report = query::coverage_report(&merged);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Model Coverage Report");
        println!("{}", "=".repeat(50));

        if !report.undocumented_defs.is_empty() {
            println!();
            println!("Undocumented definitions ({}):", report.undocumented_defs.len());
            for item in &report.undocumented_defs {
                println!("  {} ({}) line {}", item.name, item.kind, item.line);
            }
        }

        if !report.untyped_usages.is_empty() {
            println!();
            println!("Untyped usages ({}):", report.untyped_usages.len());
            for item in &report.untyped_usages {
                println!("  {} ({}) line {}", item.name, item.kind, item.line);
            }
        }

        if !report.empty_body_defs.is_empty() {
            println!();
            println!("Empty definitions ({}):", report.empty_body_defs.len());
            for item in &report.empty_body_defs {
                println!("  {} ({}) line {}", item.name, item.kind, item.line);
            }
        }

        if !report.no_member_defs.is_empty() {
            println!();
            println!("Definitions with no members ({}):", report.no_member_defs.len());
            for item in &report.no_member_defs {
                println!("  {} ({}) line {}", item.name, item.kind, item.line);
            }
        }

        if !report.unsatisfied_reqs.is_empty() {
            println!();
            println!("Unsatisfied requirements ({}):", report.unsatisfied_reqs.len());
            for item in &report.unsatisfied_reqs {
                println!("  {}", item.name);
            }
        }

        if !report.unverified_reqs.is_empty() {
            println!();
            println!("Unverified requirements ({}):", report.unverified_reqs.len());
            for item in &report.unverified_reqs {
                println!("  {}", item.name);
            }
        }

        println!();
        println!("Summary:");
        println!("  Documentation:       {:.0}%", report.summary.documented_pct);
        println!("  Typed usages:        {:.0}%", report.summary.typed_usages_pct);
        println!("  Populated defs:      {:.0}%", report.summary.populated_defs_pct);
        println!("  Req satisfaction:    {:.0}%", report.summary.req_satisfaction_pct);
        println!("  Req verification:    {:.0}%", report.summary.req_verification_pct);
        println!("  Overall score:       {:.0}%", report.summary.overall_score);
    }

    if check && report.summary.overall_score < min_score {
        eprintln!(
            "error: coverage score {:.0}% is below minimum {:.0}%",
            report.summary.overall_score, min_score
        );
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
