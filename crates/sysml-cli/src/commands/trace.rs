use std::path::PathBuf;
use std::process::ExitCode;

use crate::Cli;

pub(crate) fn run(cli: &Cli, files: &[PathBuf], check: bool, gate: Option<&str>) -> ExitCode {
    use sysml_core::query;

    // Requirements come from the files the user named. Include paths
    // (stdlib, libraries) are resolution context: without this split,
    // `trace` listed the standard library's own requirements.
    let Some((mut merged, context)) = crate::helpers::load_target_and_context(cli, files) else {
        return ExitCode::FAILURE;
    };
    // Satisfy/verify declared elsewhere in the project still count.
    {
        let mut all = vec![merged.clone()];
        all.extend(context.iter().cloned());
        let (satisfied, verified) = sysml_core::resolver::traced_requirement_defs(all.iter());
        merged.external_satisfied = satisfied.into_iter().collect();
        merged.external_verified = verified.into_iter().collect();
    }

    let rows = query::trace_requirements(&merged);
    let coverage = query::trace_coverage(&rows);

    if cli.format == "json" {
        let json = serde_json::json!({
            "requirements": rows.iter().map(|r| {
                serde_json::json!({
                    "name": r.requirement,
                    "id": r.id,
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

        // The RTM table renders through the same table pipeline as
        // `view`, so text/csv/md come out identically formatted.
        let table = sysml_core::view_render::RenderedTable {
            view: "trace".to_string(),
            columns: vec![
                "Requirement".to_string(),
                "Satisfied By".to_string(),
                "Verified By".to_string(),
            ],
            rows: rows
                .iter()
                .map(|row| {
                    let join = |v: &[String]| {
                        if v.is_empty() {
                            "-".to_string()
                        } else {
                            v.join(", ")
                        }
                    };
                    let label = match &row.id {
                        Some(id) => format!("<{}> {}", id, row.requirement),
                        None => row.requirement.clone(),
                    };
                    vec![label, join(&row.satisfied_by), join(&row.verified_by)]
                })
                .collect(),
            warnings: Vec::new(),
        };
        crate::output::print_table(&cli.format, &table);

        // Print coverage summary (text only; csv/md stay pure tables)
        if cli.format == "text" && coverage.total_requirements > 0 {
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
        let pct = |n: usize| 100.0 * n as f64 / total as f64;
        // The gate threshold lives in the model: a constraint usage typed
        // `TraceGate` decides pass/fail. No gate declared = strict.
        let gate_name = super::coverage::resolve_gate_name(
            gate,
            crate::helpers::discovered_gates().trace,
            "TraceGate",
        );
        match query::evaluate_gate(
            &merged,
            &gate_name,
            &[
                ("satisfied", pct(coverage.satisfied_count)),
                ("verified", pct(coverage.verified_count)),
                ("traced", pct(coverage.fully_traced_count)),
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
                if coverage.satisfied_count < total || coverage.verified_count < total {
                    eprintln!(
                        "error: {} requirement(s) missing satisfaction or verification \
                         (declare a {} constraint to set a threshold)",
                        total - coverage.fully_traced_count,
                        gate_name
                    );
                    return ExitCode::from(1);
                }
            }
        }
    }

    ExitCode::SUCCESS
}
