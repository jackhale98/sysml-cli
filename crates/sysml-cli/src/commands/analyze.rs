//! Analyze command: list, run, and compare analysis cases.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;
use sysml_core::sim::analysis::{
    evaluate_analysis, evaluate_trade_study, extract_analysis_cases_from_model,
    format_analysis_list, AnalysisCaseModel,
};

use crate::cli::AnalyzeCommand;
use crate::Cli;

pub fn run(cli: &Cli, kind: &AnalyzeCommand) -> ExitCode {
    match kind {
        AnalyzeCommand::List { files } => run_list(cli, files),
        AnalyzeCommand::Run {
            files,
            name,
            bindings,
        } => run_execute(cli, files, name.as_deref(), bindings),
        AnalyzeCommand::Trade { files, name } => run_trade(cli, files, name.as_deref()),
    }
}

fn parse_models(files: &[PathBuf]) -> Option<sysml_core::model::Model> {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return None;
    }
    let mut merged = sysml_core::model::Model::new("merged".to_string());
    for file_path in &files {
        let path_str = file_path.to_string_lossy().to_string();
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                return None;
            }
        };
        let model = sysml_parser::parse_file(&path_str, &source);
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

fn run_list(cli: &Cli, files: &[PathBuf]) -> ExitCode {
    let Some(model) = parse_models(files) else {
        return ExitCode::FAILURE;
    };
    let cases = extract_analysis_cases_from_model(&model);

    match cli.format.as_str() {
        "json" => {
            let items: Vec<_> = cases
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "subject": c.subject.as_ref().map(|s| serde_json::json!({
                            "name": s.name,
                            "type": s.type_ref,
                            "binding": s.value_binding,
                        })),
                        "objective": c.objective.as_ref().map(|o| serde_json::json!({
                            "name": o.name,
                            "kind": format!("{:?}", o.kind),
                        })),
                        "parameters": c.parameters.iter().map(|p| serde_json::json!({
                            "name": p.name,
                            "type": p.type_ref,
                            "direction": format!("{:?}", p.direction),
                        })).collect::<Vec<_>>(),
                        "return": c.return_decl.as_ref().map(|r| serde_json::json!({
                            "name": r.name,
                            "type": r.type_ref,
                            "value_expr": r.value_expr,
                        })),
                        "alternatives": c.alternatives.iter().map(|a| &a.name).collect::<Vec<_>>(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
        }
        _ => {
            print!("{}", format_analysis_list(&cases));
        }
    }

    ExitCode::SUCCESS
}

fn run_execute(cli: &Cli, files: &[PathBuf], name: Option<&str>, bindings: &[String]) -> ExitCode {
    let Some(model) = parse_models(files) else {
        return ExitCode::FAILURE;
    };
    let cases = extract_analysis_cases_from_model(&model);

    let case = match select_case(&cases, name) {
        Some(c) => c,
        None => return ExitCode::FAILURE,
    };

    let env = crate::parse_bindings(bindings);

    // Evaluate the analysis case
    let eval_result = evaluate_analysis(&model, case, &env);

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({
                "analysis": case.name,
                "subject": case.subject.as_ref().map(|s| &s.name),
                "subject_type": case.subject.as_ref().and_then(|s| s.type_ref.as_ref()),
                "objective": case.objective.as_ref().map(|o| format!("{:?}", o.kind)),
                "parameters": case.parameters.iter().map(|p| {
                    let val = env.get(&p.name).map(|v| format!("{}", v));
                    serde_json::json!({
                        "name": p.name,
                        "type": p.type_ref,
                        "bound_value": val,
                    })
                }).collect::<Vec<_>>(),
                "computed_bindings": eval_result.bindings.iter().map(|(n, v)| {
                    serde_json::json!({"name": n, "value": v})
                }).collect::<Vec<_>>(),
                "return_value": eval_result.return_value,
                "return": case.return_decl.as_ref().map(|r| serde_json::json!({
                    "name": r.name,
                    "type": r.type_ref,
                    "expression": r.value_expr,
                })),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            println!("Analysis: {}", case.name);
            if let Some(ref subj) = case.subject {
                println!(
                    "  Subject: {}{}{}",
                    subj.name,
                    subj.type_ref
                        .as_ref()
                        .map(|t| format!(" : {}", t))
                        .unwrap_or_default(),
                    subj.value_binding
                        .as_ref()
                        .map(|v| format!(" = {}", v))
                        .unwrap_or_default(),
                );
            }
            if let Some(ref obj) = case.objective {
                println!("  Objective: {} {:?}", obj.name, obj.kind);
            }
            for param in &case.parameters {
                let val = env.get(&param.name).map(|v| format!(" = {}", v));
                println!(
                    "  {:?} {} {}{}",
                    param.direction,
                    param.name,
                    param
                        .type_ref
                        .as_ref()
                        .map(|t| format!(": {}", t))
                        .unwrap_or_default(),
                    val.unwrap_or_default()
                );
            }
            for binding in &case.local_bindings {
                println!("  {} = {}", binding.name, binding.value_expr);
            }
            if !eval_result.bindings.is_empty() {
                println!("  Computed:");
                for (name, val) in &eval_result.bindings {
                    println!("    {} = {:.4}", name, val);
                }
            }
            if let Some(ref ret) = case.return_decl {
                let computed = eval_result
                    .return_value
                    .map(|v| format!(" => {:.4}", v))
                    .unwrap_or_default();
                println!(
                    "  Return: {}{}{}{}",
                    ret.name,
                    ret.type_ref
                        .as_ref()
                        .map(|t| format!(" : {}", t))
                        .unwrap_or_default(),
                    ret.value_expr
                        .as_ref()
                        .map(|e| format!(" = {}", e))
                        .unwrap_or_default(),
                    computed,
                );
            }
        }
    }

    ExitCode::SUCCESS
}

fn run_trade(cli: &Cli, files: &[PathBuf], name: Option<&str>) -> ExitCode {
    let Some(model) = parse_models(files) else {
        return ExitCode::FAILURE;
    };
    let cases = extract_analysis_cases_from_model(&model);

    let case = match select_case(&cases, name) {
        Some(c) => c,
        None => return ExitCode::FAILURE,
    };

    if case.alternatives.is_empty() {
        eprintln!(
            "error: analysis case `{}` has no alternatives defined for trade study",
            case.name
        );
        return ExitCode::FAILURE;
    }

    let trade = evaluate_trade_study(&model, case);

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({
                "analysis": trade.name,
                "objective": format!("{:?}", trade.objective),
                "winner": trade.winner,
                "alternatives": trade.alternatives.iter().map(|a| {
                    serde_json::json!({
                        "name": a.name,
                        "score": a.score,
                        "overrides": a.overrides.iter().map(|(k, v)| {
                            serde_json::json!({"attribute": k, "value": v})
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            println!("Trade Study: {}", trade.name);
            println!("  Objective: {:?}", trade.objective);
            if let Some(ref winner) = trade.winner {
                println!("  Winner: {}", winner);
            }
            println!();
            for alt in &trade.alternatives {
                let score_str = alt
                    .score
                    .map(|s| format!(" (score: {:.4})", s))
                    .unwrap_or_default();
                println!("  Alternative: {}{}", alt.name, score_str);
                for (attr, val) in &alt.overrides {
                    println!("    {} = {}", attr, val);
                }
            }
        }
    }

    ExitCode::SUCCESS
}

fn select_case<'a>(
    cases: &'a [AnalysisCaseModel],
    name: Option<&str>,
) -> Option<&'a AnalysisCaseModel> {
    if cases.is_empty() {
        eprintln!("error: no analysis cases found in the model");
        return None;
    }

    if let Some(n) = name {
        cases.iter().find(|c| c.name == n).or_else(|| {
            eprintln!("error: analysis case `{}` not found", n);
            eprintln!(
                "  available: {}",
                cases
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            None
        })
    } else if cases.len() == 1 {
        Some(&cases[0])
    } else {
        let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
        match crate::select_item("analysis case", &names) {
            Some(idx) => Some(&cases[idx]),
            None => None,
        }
    }
}
