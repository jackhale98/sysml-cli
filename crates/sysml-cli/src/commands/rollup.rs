/// Rollup command: compute attribute rollups over the part hierarchy.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::model::Model;
use sysml_core::parser as sysml_parser;
use sysml_core::sim::rollup::{
    evaluate_rollup, format_rollup_text, AggregationMethod, RollupResult,
};

use crate::cli::RollupCommand;
use crate::Cli;

pub fn run(cli: &Cli, kind: &RollupCommand) -> ExitCode {
    match kind {
        RollupCommand::Compute {
            files,
            root,
            attr,
            method,
        } => run_compute(cli, files, root, attr, method),
        RollupCommand::Budget {
            files,
            root,
            attr,
            limit,
            method,
        } => run_budget(cli, files, root, attr, *limit, method),
        RollupCommand::Sensitivity {
            files,
            root,
            attr,
        } => run_sensitivity(cli, files, root, attr),
        RollupCommand::Sweep {
            files, root, attr, param, from, to, steps,
        } => run_sweep(cli, files, root, attr, param, *from, *to, *steps),
        RollupCommand::WhatIf {
            files, root, attr, scenarios,
        } => run_what_if(cli, files, root, attr, scenarios),
        RollupCommand::Query { files, attr } => run_query(cli, files, attr),
    }
}

fn parse_and_merge(files: &[PathBuf]) -> Option<Model> {
    let mut merged = Model::new("merged".to_string());
    for file_path in files {
        let path_str = file_path.to_string_lossy().to_string();
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                return None;
            }
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        // Merge into combined model
        merged.definitions.extend(model.definitions);
        merged.usages.extend(model.usages);
        merged.connections.extend(model.connections);
        merged.flows.extend(model.flows);
        merged.satisfactions.extend(model.satisfactions);
        merged.verifications.extend(model.verifications);
        merged.allocations.extend(model.allocations);
        merged.type_references.extend(model.type_references);
        merged.imports.extend(model.imports);
        merged.comments.extend(model.comments);
        merged.views.extend(model.views);
    }
    Some(merged)
}

fn run_compute(cli: &Cli, files: &[PathBuf], root: &str, attr: &str, method: &str) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };
    let Some(agg) = AggregationMethod::from_str(method) else {
        eprintln!("error: unknown aggregation method `{}`. Use: sum, rss, product, min, max", method);
        return ExitCode::FAILURE;
    };
    if model.find_def(root).is_none() {
        eprintln!("error: definition `{}` not found", root);
        return ExitCode::FAILURE;
    }

    let result = evaluate_rollup(&model, root, attr, agg);

    match cli.format.as_str() {
        "json" => println!("{}", format_rollup_json(&result)),
        _ => print!("{}", format_rollup_text(&result)),
    }

    ExitCode::SUCCESS
}

fn run_budget(
    cli: &Cli,
    files: &[PathBuf],
    root: &str,
    attr: &str,
    limit: f64,
    method: &str,
) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };
    let Some(agg) = AggregationMethod::from_str(method) else {
        eprintln!("error: unknown aggregation method `{}`", method);
        return ExitCode::FAILURE;
    };
    if model.find_def(root).is_none() {
        eprintln!("error: definition `{}` not found", root);
        return ExitCode::FAILURE;
    }

    let result = evaluate_rollup(&model, root, attr, agg);
    let margin = limit - result.total;
    let margin_pct = if limit > 0.0 {
        (margin / limit) * 100.0
    } else {
        0.0
    };
    let pass = result.total <= limit;

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({
                "root": result.root,
                "attribute": result.attribute,
                "method": result.method.label(),
                "total": result.total,
                "limit": limit,
                "margin": margin,
                "margin_pct": margin_pct,
                "pass": pass,
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            println!("Budget: {} for {}", attr, root);
            println!("  Total:  {:.4}", result.total);
            println!("  Limit:  {:.4}", limit);
            println!("  Margin: {:.4} ({:.1}%)", margin, margin_pct);
            println!("  Status: {}", if pass { "PASS" } else { "FAIL" });
            if !result.contributions.is_empty() {
                println!("  Top contributors:");
                let mut sorted = result.contributions.clone();
                sorted.sort_by(|a, b| b.subtotal.partial_cmp(&a.subtotal).unwrap());
                for c in sorted.iter().take(5) {
                    println!(
                        "    {:20} {:.4} ({:.1}%)",
                        c.path.last().unwrap_or(&"?".to_string()),
                        c.subtotal,
                        c.percentage
                    );
                }
            }
        }
    }

    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_sensitivity(cli: &Cli, files: &[PathBuf], root: &str, attr: &str) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };
    if model.find_def(root).is_none() {
        eprintln!("error: definition `{}` not found", root);
        return ExitCode::FAILURE;
    }

    let result = evaluate_rollup(&model, root, attr, AggregationMethod::Sum);

    // Flatten and sort contributions by subtotal descending
    let mut flat = Vec::new();
    flatten_contributions(&result.contributions, &mut flat);
    flat.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    match cli.format.as_str() {
        "json" => {
            let items: Vec<_> = flat
                .iter()
                .map(|(path, subtotal, pct)| {
                    serde_json::json!({
                        "path": path,
                        "subtotal": subtotal,
                        "percentage": pct,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
        }
        _ => {
            println!("Sensitivity: {} for {} (total: {:.4})", attr, root, result.total);
            println!("{:30} {:>12} {:>8}", "Component", "Value", "%");
            println!("{}", "-".repeat(52));
            if result.own_value != 0.0 {
                let pct = if result.total > 0.0 {
                    (result.own_value / result.total) * 100.0
                } else {
                    0.0
                };
                println!("{:30} {:>12.4} {:>7.1}%", format!("{} (own)", root), result.own_value, pct);
            }
            for (path, subtotal, pct) in &flat {
                println!("{:30} {:>12.4} {:>7.1}%", path, subtotal, pct);
            }
        }
    }

    ExitCode::SUCCESS
}

fn flatten_contributions(
    contributions: &[sysml_core::sim::rollup::Contribution],
    out: &mut Vec<(String, f64, f64)>,
) {
    for c in contributions {
        out.push((c.path.join("."), c.subtotal, c.percentage));
    }
}

fn run_query(cli: &Cli, files: &[PathBuf], attr: &str) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };

    // Find all usages with this attribute name
    let mut results: Vec<(String, String, Option<String>)> = Vec::new();
    for usage in &model.usages {
        if usage.name == attr && matches!(usage.kind.as_str(), "attribute" | "feature") {
            results.push((
                usage.parent_def.clone().unwrap_or_default(),
                usage.type_ref.clone().unwrap_or_default(),
                usage.value_expr.clone(),
            ));
        }
    }

    if results.is_empty() {
        eprintln!("No attributes named `{}` found.", attr);
        return ExitCode::SUCCESS;
    }

    match cli.format.as_str() {
        "json" => {
            let items: Vec<_> = results
                .iter()
                .map(|(parent, type_ref, value)| {
                    serde_json::json!({
                        "parent": parent,
                        "type": type_ref,
                        "value": value,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
        }
        _ => {
            println!("{:30} {:>15} {:>12}", "Definition", "Type", "Value");
            println!("{}", "-".repeat(59));
            for (parent, type_ref, value) in &results {
                println!(
                    "{:30} {:>15} {:>12}",
                    parent,
                    type_ref,
                    value.as_deref().unwrap_or("-")
                );
            }
        }
    }

    ExitCode::SUCCESS
}

fn format_rollup_json(result: &RollupResult) -> String {
    let json = serde_json::json!({
        "root": result.root,
        "attribute": result.attribute,
        "method": result.method.label(),
        "total": result.total,
        "own_value": result.own_value,
        "contributions": result.contributions.iter().map(|c| {
            contribution_to_json(c)
        }).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&json).unwrap()
}

fn contribution_to_json(c: &sysml_core::sim::rollup::Contribution) -> serde_json::Value {
    serde_json::json!({
        "name": c.path.last().unwrap_or(&"?".to_string()),
        "definition": c.definition,
        "quantity": c.quantity,
        "own_value": c.own_value,
        "subtotal": c.subtotal,
        "percentage": c.percentage,
        "children": c.children.iter().map(contribution_to_json).collect::<Vec<_>>(),
    })
}

fn run_sweep(
    cli: &Cli,
    files: &[PathBuf],
    root: &str,
    attr: &str,
    param: &str,
    from: f64,
    to: f64,
    steps: usize,
) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };
    if model.find_def(root).is_none() {
        eprintln!("error: definition `{}` not found", root);
        return ExitCode::FAILURE;
    }

    use sysml_core::sim::what_if::{evaluate_sweep, SweepConfig};
    let config = SweepConfig {
        parameter: param.to_string(),
        start: from,
        end: to,
        steps,
    };
    let result = evaluate_sweep(&model, root, attr, AggregationMethod::Sum, &config);

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({
                "root": result.root,
                "attribute": result.attribute,
                "parameter": result.parameter,
                "sensitivity": result.sensitivity,
                "points": result.points.iter().map(|(p, t)| {
                    serde_json::json!({"param": p, "total": t})
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            println!("Sweep: {} for {} (varying {})", attr, root, param);
            println!("  Sensitivity: {:.4} (d_total / d_param)", result.sensitivity);
            println!();
            println!("{:>12} {:>12}", param, format!("total_{}", attr));
            println!("{}", "-".repeat(26));
            for (p, t) in &result.points {
                println!("{:>12.4} {:>12.4}", p, t);
            }
        }
    }

    ExitCode::SUCCESS
}

fn run_what_if(
    cli: &Cli,
    files: &[PathBuf],
    root: &str,
    attr: &str,
    scenario_strs: &[String],
) -> ExitCode {
    let Some(model) = parse_and_merge(files) else {
        return ExitCode::FAILURE;
    };
    if model.find_def(root).is_none() {
        eprintln!("error: definition `{}` not found", root);
        return ExitCode::FAILURE;
    }

    use sysml_core::sim::what_if::{evaluate_what_if, Scenario};

    // Parse scenario strings: "name:path=value,path=value"
    let scenarios: Vec<Scenario> = scenario_strs
        .iter()
        .filter_map(|s| {
            let (name, overrides_str) = s.split_once(':')?;
            let overrides: Vec<(String, f64)> = overrides_str
                .split(',')
                .filter_map(|o| {
                    let (path, val) = o.split_once('=')?;
                    Some((path.trim().to_string(), val.trim().parse().ok()?))
                })
                .collect();
            Some(Scenario {
                name: name.trim().to_string(),
                overrides,
            })
        })
        .collect();

    if scenarios.is_empty() {
        eprintln!("error: no valid scenarios. Format: --scenario 'name:path=value'");
        return ExitCode::FAILURE;
    }

    let result = evaluate_what_if(&model, root, attr, AggregationMethod::Sum, &scenarios);

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({
                "root": result.root,
                "attribute": result.attribute,
                "baseline": result.baseline,
                "scenarios": result.scenarios.iter().map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "total": s.total,
                        "delta": s.delta,
                        "delta_pct": s.delta_pct,
                    })
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            println!("What-if: {} for {} (baseline: {:.4})", attr, root, result.baseline);
            println!();
            println!("{:20} {:>12} {:>12} {:>8}", "Scenario", "Total", "Delta", "%");
            println!("{}", "-".repeat(54));
            println!("{:20} {:>12.4} {:>12} {:>8}", "(baseline)", result.baseline, "-", "-");
            for s in &result.scenarios {
                println!(
                    "{:20} {:>12.4} {:>+12.4} {:>+7.1}%",
                    s.name, s.total, s.delta, s.delta_pct
                );
            }
        }
    }

    ExitCode::SUCCESS
}
