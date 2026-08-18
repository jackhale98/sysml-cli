use crate::Cli;
use std::path::PathBuf;
use std::process::ExitCode;

/// Resolve which gate constraint def `--check` evaluates:
/// CLI flag > `[gates]` config > conventional name.
pub(crate) fn resolve_gate_name(flag: Option<&str>, config_gate: Option<String>, conventional: &str) -> String {
    flag.map(|s| s.to_string())
        .or(config_gate)
        .unwrap_or_else(|| conventional.to_string())
}

pub(crate) fn run(cli: &Cli, files: &[PathBuf], check: bool, gate: Option<&str>) -> ExitCode {
    use sysml_core::query;
    // Score the user's files, not the include path.
    let Some((merged, _context)) = crate::helpers::load_target_and_context(cli, files) else {
        return ExitCode::FAILURE;
    };

    let report = query::coverage_report(&merged);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Model Coverage Report");
        println!("{}", "=".repeat(50));

        if !report.undocumented_defs.is_empty() {
            println!();
            println!(
                "Undocumented definitions ({}):",
                report.undocumented_defs.len()
            );
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
            println!(
                "Definitions with no members ({}):",
                report.no_member_defs.len()
            );
            for item in &report.no_member_defs {
                println!("  {} ({}) line {}", item.name, item.kind, item.line);
            }
        }

        if !report.unsatisfied_reqs.is_empty() {
            println!();
            println!(
                "Unsatisfied requirements ({}):",
                report.unsatisfied_reqs.len()
            );
            for item in &report.unsatisfied_reqs {
                println!("  {}", item.name);
            }
        }

        if !report.unverified_reqs.is_empty() {
            println!();
            println!(
                "Unverified requirements ({}):",
                report.unverified_reqs.len()
            );
            for item in &report.unverified_reqs {
                println!("  {}", item.name);
            }
        }

        println!();
        println!("Summary:");
        println!(
            "  Documentation:       {:.0}%",
            report.summary.documented_pct
        );
        println!(
            "  Typed usages:        {:.0}%",
            report.summary.typed_usages_pct
        );
        println!(
            "  Populated defs:      {:.0}%",
            report.summary.populated_defs_pct
        );
        println!(
            "  Req satisfaction:    {:.0}%",
            report.summary.req_satisfaction_pct
        );
        println!(
            "  Req verification:    {:.0}%",
            report.summary.req_verification_pct
        );
        println!(
            "  Overall score:       {:.0}%{}",
            report.summary.overall_score,
            if report.summary.score_source == "built-in" {
                String::new()
            } else {
                format!("  ({})", report.summary.score_source)
            }
        );
    }

    if check {
        // The gate threshold lives in the model: a constraint usage typed
        // `QualityGate` decides pass/fail. No gate declared = strict.
        let s = &report.summary;
        let gate_name = resolve_gate_name(
            gate,
            crate::helpers::discovered_gates().coverage,
            "QualityGate",
        );
        match query::evaluate_gate(
            &merged,
            &gate_name,
            &[
                ("score", s.overall_score),
                ("documented", s.documented_pct),
                ("typedUsages", s.typed_usages_pct),
                ("reqSatisfied", s.req_satisfaction_pct),
                ("reqVerified", s.req_verification_pct),
            ],
        ) {
            Some(gate) if !gate.passed => {
                for f in &gate.failed {
                    eprintln!("error: {} failed: {}", gate_name, f);
                }
                return ExitCode::from(1);
            }
            Some(_) => {}
            None => {
                if s.overall_score < 100.0 {
                    eprintln!(
                        "error: coverage score {:.0}% is below 100% \
                         (declare a {} constraint to set a threshold)",
                        s.overall_score, gate_name
                    );
                    return ExitCode::from(1);
                }
            }
        }
    }

    ExitCode::SUCCESS
}
