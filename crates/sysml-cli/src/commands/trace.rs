use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, files: &[PathBuf], check: bool, min_coverage: f64) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }

    use sysml_core::query;

    // Parse all files into a merged model
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

    let rows = query::trace_requirements(&merged);
    let coverage = query::trace_coverage(&rows);

    if cli.format == "json" {
        let json = serde_json::json!({
            "requirements": rows.iter().map(|r| {
                serde_json::json!({
                    "name": r.requirement,
                    "satisfied_by": r.satisfied_by,
                    "verified_by": r.verified_by,
                })
            }).collect::<Vec<_>>(),
            "coverage": {
                "total": coverage.total_requirements,
                "satisfied": coverage.satisfied_count,
                "verified": coverage.verified_count,
                "fully_traced": coverage.fully_traced_count,
            },
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if rows.is_empty() {
            println!("No requirements found.");
            return ExitCode::SUCCESS;
        }

        // Print RTM table
        println!(
            "{:<20} {:<20} {:<20}",
            "Requirement", "Satisfied By", "Verified By"
        );
        println!("{}", "-".repeat(60));
        for row in &rows {
            let sat = if row.satisfied_by.is_empty() {
                "-".to_string()
            } else {
                row.satisfied_by.join(", ")
            };
            let ver = if row.verified_by.is_empty() {
                "-".to_string()
            } else {
                row.verified_by.join(", ")
            };
            println!("{:<20} {:<20} {:<20}", row.requirement, sat, ver);
        }

        // Print coverage summary
        if coverage.total_requirements > 0 {
            let sat_pct =
                100.0 * coverage.satisfied_count as f64 / coverage.total_requirements as f64;
            let ver_pct =
                100.0 * coverage.verified_count as f64 / coverage.total_requirements as f64;
            println!();
            println!(
                "Coverage: {}/{} satisfied ({:.0}%), {}/{} verified ({:.0}%)",
                coverage.satisfied_count,
                coverage.total_requirements,
                sat_pct,
                coverage.verified_count,
                coverage.total_requirements,
                ver_pct,
            );
        }
    }

    if check {
        let total = coverage.total_requirements;
        if total == 0 {
            return ExitCode::SUCCESS;
        }
        let traced_pct = 100.0 * coverage.fully_traced_count as f64 / total as f64;
        if traced_pct < min_coverage {
            eprintln!(
                "error: trace coverage {:.0}% is below minimum {:.0}%",
                traced_pct, min_coverage
            );
            return ExitCode::from(1);
        }
        if coverage.satisfied_count < total || coverage.verified_count < total {
            eprintln!(
                "error: {} requirement(s) missing satisfaction or verification",
                total - coverage.fully_traced_count
            );
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
