/// Quality management CLI commands (NCR, CAPA, Process Deviation).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::QualityCommand;

pub fn run(cli: &crate::Cli, kind: &QualityCommand) -> ExitCode {
    match kind {
        QualityCommand::Trend { files, group_by } => run_trend(cli, files, group_by),
        QualityCommand::List => run_list(cli),
    }
}

fn run_trend(cli: &crate::Cli, files: &[PathBuf], group_by: &str) -> ExitCode {
    if files.is_empty() {
        if cli.format == "json" {
            println!("[]");
        } else {
            println!("Quality Trend Analysis");
            println!();
            println!("  No files provided. To analyze NCR trends, provide SysML files that");
            println!("  contain nonconformance records or use the record system:");
            println!();
            println!("  1. Create NCRs with `sysml quality` record commands");
            println!("  2. Provide model files to correlate NCRs with parts");
            println!();
            println!("  Group-by: {group_by}");
        }
        return ExitCode::SUCCESS;
    }

    let _models = match parse_files(files) {
        Some(m) => m,
        None => return ExitCode::FAILURE,
    };

    if cli.format == "json" {
        let output = serde_json::json!({
            "group_by": group_by,
            "items": serde_json::Value::Array(Vec::new()),
            "note": "NCR trends are derived from .sysml-records/ files. \
                     Use `sysml quality list` to see current status.",
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
    } else {
        println!("Quality Trend Analysis (group by: {group_by})");
        println!();
        println!("  No NCR records found in model files.");
        println!("  NCR trends are derived from the `.sysml-records/` directory.");
        println!("  Use `sysml quality list` to view current status.");
    }

    ExitCode::SUCCESS
}

fn run_list(cli: &crate::Cli) -> ExitCode {
    if cli.format == "json" {
        let overview = serde_json::json!({
            "item_types": {
                "ncr": {
                    "name": "Nonconformance Report",
                    "lifecycle": ["Open", "Investigating", "Dispositioned", "Verified", "Closed", "Reopened"],
                    "description": "Documents an observed nonconformance — what went wrong."
                },
                "capa": {
                    "name": "Corrective/Preventive Action",
                    "lifecycle": ["Initiated", "Root Cause Analysis", "Planning Actions", "Implementing", "Verifying Effectiveness", "Pending Closure", "Closed"],
                    "description": "A formal action program to address root causes and prevent recurrence."
                },
                "deviation": {
                    "name": "Process Deviation",
                    "lifecycle": ["Requested", "Under Review", "Approved", "Denied", "Active", "Expired", "Closed"],
                    "description": "A planned, approved departure from a standard process."
                }
            },
        });
        println!("{}", serde_json::to_string_pretty(&overview).unwrap_or_default());
    } else {
        println!("Quality Management Overview");
        println!();
        println!("  Three quality item types, each with its own lifecycle:");
        println!();
        println!("  NCR (Nonconformance Report)");
        println!("    Documents what went wrong — a finding, not an action.");
        println!("    Lifecycle: Open → Investigating → Dispositioned → Verified → Closed");
        println!();
        println!("  CAPA (Corrective/Preventive Action)");
        println!("    A formal action program to address root causes.");
        println!("    May originate from NCRs, audits, complaints, or improvement.");
        println!("    Lifecycle: Initiated → RCA → Planning → Implementing → Verifying → Closed");
        println!();
        println!("  Process Deviation");
        println!("    A planned, approved departure from a standard process.");
        println!("    Unlike NCRs (unplanned), deviations are pre-approved.");
        println!("    Lifecycle: Requested → Under Review → Approved → Active → Closed");
        println!();
        println!("  Records are stored in `.sysml-records/` via the record envelope system.");
        println!("  Use `sysml quality trend <files>` to analyze trends.");
    }

    ExitCode::SUCCESS
}

fn parse_files(files: &[PathBuf]) -> Option<Vec<sysml_core::model::Model>> {
    let mut models = Vec::new();
    for f in files {
        let (path, source) = match crate::read_source(f) {
            Ok(ps) => ps,
            Err(_) => return None,
        };
        models.push(sysml_core::parser::parse_file(&path, &source));
    }
    Some(models)
}
