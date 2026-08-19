//! Analyze command: list, run, and compare analysis cases.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;
use sysml_core::sim::analysis::{
    evaluate_analysis, evaluate_trade_study, extract_analysis_cases_from_model, AnalysisCaseModel,
};

use crate::cli::AnalyzeCommand;
use crate::Cli;

pub fn run(cli: &Cli, kind: &AnalyzeCommand) -> ExitCode {
    match kind {
        AnalyzeCommand::Run {
            files,
            name,
            bindings,
            method,
            iterations,
            seed,
        } => {
            // Uncertainty analysis cases (typed by a specialization of
            // Uncertainty::UncertaintyAnalysis) take the propagation path;
            // everything else falls through to the classic evaluator.
            match try_run_uncertainty(
                cli,
                files,
                name.as_deref(),
                method.as_deref(),
                *iterations,
                *seed,
            ) {
                UncertaintyOutcome::Ran(code) => code,
                UncertaintyOutcome::NotApplicable => {
                    if method.is_some() || iterations.is_some() || seed.is_some() {
                        eprintln!(
                            "error: --method/--iterations/--seed apply to uncertainty \
                             analysis cases (types specializing Uncertainty::UncertaintyAnalysis), \
                             and no such case matched; is the Uncertainty library on the \
                             include path (-I)?"
                        );
                        return ExitCode::FAILURE;
                    }
                    run_execute(cli, files, name.as_deref(), bindings)
                }
            }
        }
        AnalyzeCommand::Trade { files, name } => run_trade(cli, files, name.as_deref()),
    }
}

enum UncertaintyOutcome {
    Ran(ExitCode),
    NotApplicable,
}

/// Load per-file models (files + resolved include paths) — the uncertainty
/// extractor needs real per-file byte spans, so no merging here.
fn load_per_file_models(cli: &Cli, files: &[PathBuf]) -> Option<Vec<sysml_core::model::Model>> {
    let (files, _) = crate::files_or_project(files, cli.quiet);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return None;
    }
    let mut all_files = files;
    for inc in crate::resolve_include_paths(cli) {
        if inc.is_dir() {
            crate::collect_files_recursive(&inc, &mut all_files);
        } else {
            all_files.push(inc);
        }
    }
    let mut models = Vec::new();
    for file_path in &all_files {
        let path_str = file_path.to_string_lossy().to_string();
        match std::fs::read_to_string(file_path) {
            Ok(source) => models.push(sysml_parser::parse_file(&path_str, &source)),
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                return None;
            }
        }
    }
    Some(models)
}

fn try_run_uncertainty(
    cli: &Cli,
    files: &[PathBuf],
    name: Option<&str>,
    method: Option<&str>,
    iterations: Option<u64>,
    seed: Option<u64>,
) -> UncertaintyOutcome {
    use sysml_core::sim::uncertainty_model::{extract_case, find_uncertainty_cases};

    let Some(models) = load_per_file_models(cli, files) else {
        return UncertaintyOutcome::Ran(ExitCode::FAILURE);
    };
    let cases = find_uncertainty_cases(&models, &models);
    if cases.is_empty() {
        return UncertaintyOutcome::NotApplicable;
    }

    // Pick the case: by name if given, otherwise unambiguous single case.
    let case_name = match name {
        Some(n) => {
            if !cases.iter().any(|(cn, _, _)| cn == n) {
                return UncertaintyOutcome::NotApplicable;
            }
            n.to_string()
        }
        None => {
            if cases.len() == 1 {
                cases[0].0.clone()
            } else {
                eprintln!("error: multiple uncertainty analysis cases found; pick one with -n:");
                for (n, file, ty) in &cases {
                    eprintln!("  {n} : {ty}  ({file})");
                }
                return UncertaintyOutcome::Ran(ExitCode::FAILURE);
            }
        }
    };

    let mut case = match extract_case(&models, &case_name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return UncertaintyOutcome::Ran(ExitCode::FAILURE);
        }
    };
    if let Some(it) = iterations {
        case.settings.iterations = it;
    }
    if let Some(s) = seed {
        case.settings.seed = Some(s);
    }

    let methods: Vec<&str> = match method.unwrap_or("all") {
        "all" => vec!["worst-case", "rss", "monte-carlo"],
        m @ ("worst-case" | "rss" | "monte-carlo") => vec![m],
        other => {
            eprintln!(
                "error: unknown method `{other}` (expected worst-case, rss, monte-carlo, or all)"
            );
            return UncertaintyOutcome::Ran(ExitCode::FAILURE);
        }
    };

    UncertaintyOutcome::Ran(run_uncertainty(cli, &case, &methods))
}

fn run_uncertainty(
    cli: &Cli,
    case: &sysml_core::sim::uncertainty_model::UncertaintyCase,
    methods: &[&str],
) -> ExitCode {
    use sysml_core::sim::uncertainty::{monte_carlo, rss, worst_case, PassFail};

    // A default seed from the clock keeps unseeded runs varied; the seed
    // actually used is always reported so any run can be replayed.
    let default_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED);

    let wc = methods
        .contains(&"worst-case")
        .then(|| worst_case(&case.inputs, &case.target, &case.settings));
    let rss_r = methods
        .contains(&"rss")
        .then(|| rss(&case.inputs, &case.target, &case.settings));
    let mc = methods
        .contains(&"monte-carlo")
        .then(|| monte_carlo(&case.inputs, &case.target, &case.settings, default_seed));

    let failed = wc.as_ref().is_some_and(|r| r.result == PassFail::Fail)
        || rss_r.as_ref().is_some_and(|r| r.result == PassFail::Fail)
        || mc.as_ref().is_some_and(|r| r.result == PassFail::Fail);

    if cli.format == "json" {
        let out = serde_json::json!({
            "case": case.name,
            "type": case.type_name,
            "file": case.file,
            "critical": case.critical,
            "unit": case.unit,
            "target": case.target,
            "inputs": case.inputs,
            "worst_case": wc,
            "rss": rss_r,
            "monte_carlo": mc,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        print_uncertainty_text(case, wc.as_ref(), rss_r.as_ref(), mc.as_ref());
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_uncertainty_text(
    case: &sysml_core::sim::uncertainty_model::UncertaintyCase,
    wc: Option<&sysml_core::sim::uncertainty::WorstCaseResult>,
    rss_r: Option<&sysml_core::sim::uncertainty::RssResult>,
    mc: Option<&sysml_core::sim::uncertainty::MonteCarloResult>,
) {
    let unit = case.unit.as_deref().unwrap_or("");
    let verdict = |r: &sysml_core::sim::uncertainty::PassFail| match r {
        sysml_core::sim::uncertainty::PassFail::Pass => "PASS",
        sysml_core::sim::uncertainty::PassFail::Marginal => "MARGINAL",
        sysml_core::sim::uncertainty::PassFail::Fail => "FAIL",
    };

    println!(
        "Uncertainty analysis: {} ({}){}",
        case.name,
        case.type_name,
        if case.critical { "  [critical]" } else { "" }
    );
    println!(
        "  Target: {:.4} in [{:.4}, {:.4}] {}",
        case.target.nominal, case.target.lower, case.target.upper, unit
    );
    println!("  Contributions:");
    for c in &case.inputs {
        println!(
            "    {} {:<22} {:>10.4} +{:.4}/-{:.4}  {:<10}{}",
            if c.sense >= 0.0 { "+" } else { "-" },
            c.name,
            c.nominal,
            c.plus,
            c.minus,
            format!("{:?}", c.distribution).to_lowercase(),
            c.source.as_deref().unwrap_or("")
        );
    }

    if let Some(r) = wc {
        println!("\n  Worst-case:");
        println!(
            "    Range: {:.4} .. {:.4}   margin: {:.4}   result: {}",
            r.min,
            r.max,
            r.margin,
            verdict(&r.result)
        );
    }
    if let Some(r) = rss_r {
        println!("\n  RSS:");
        if (r.shifted_mean - r.mean).abs() > f64::EPSILON {
            println!(
                "    Mean: {:.4} (shifted {:.4})   3\u{3c3}: {:.4}",
                r.mean, r.shifted_mean, r.sigma3
            );
        } else {
            println!("    Mean: {:.4}   3\u{3c3}: {:.4}", r.mean, r.sigma3);
        }
        println!(
            "    Cp: {:.2}   Cpk: {:.2}   Yield: {:.2}%",
            r.cp, r.cpk, r.yield_percent
        );
        let sens: Vec<String> = case
            .inputs
            .iter()
            .zip(&r.sensitivity)
            .map(|(c, s)| format!("{} {:.1}%", c.name, s))
            .collect();
        println!("    Sensitivity: {}", sens.join(" | "));
        println!("    Result: {}", verdict(&r.result));
    }
    if let Some(r) = mc {
        println!(
            "\n  Monte Carlo ({} iterations, seed {}):",
            r.iterations, r.seed
        );
        println!("    Mean: {:.4}   StdDev: {:.4}", r.mean, r.std_dev);
        println!(
            "    Range: {:.4} .. {:.4}   95% CI: [{:.4}, {:.4}]",
            r.min, r.max, r.percentile_2_5, r.percentile_97_5
        );
        println!(
            "    Pp: {:.2}   Ppk: {:.2}   Yield: {:.2}%",
            r.pp, r.ppk, r.yield_percent
        );
        print_histogram(&r.histogram, case);
        println!("    Result: {}", verdict(&r.result));
    }
}

/// ASCII histogram of the Monte Carlo samples, spec limits marked so the
/// tails are readable at a glance.
fn print_histogram(
    bins: &[sysml_core::sim::uncertainty::HistogramBin],
    case: &sysml_core::sim::uncertainty_model::UncertaintyCase,
) {
    if bins.len() < 2 {
        return;
    }
    let max_count = bins.iter().map(|b| b.count).max().unwrap_or(1).max(1);
    let (lsl, usl) = (case.target.lower, case.target.upper);
    println!("    Distribution:");
    for b in bins {
        let bar_len = ((b.count as f64 / max_count as f64) * 40.0).round() as usize;
        let mark = if lsl >= b.lower && lsl < b.upper {
            " < LSL"
        } else if usl > b.lower && usl <= b.upper {
            " < USL"
        } else {
            ""
        };
        println!(
            "      {:>9.4} | {:<40} {}{}",
            b.lower,
            "#".repeat(bar_len),
            b.count,
            mark
        );
    }
}

fn parse_models(cli: &Cli, files: &[PathBuf]) -> Option<sysml_core::model::Model> {
    crate::load_model(cli, files)
}

fn run_execute(cli: &Cli, files: &[PathBuf], name: Option<&str>, bindings: &[String]) -> ExitCode {
    let Some(model) = parse_models(cli, files) else {
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

    // Solve the case's assert-constraint equations by substitution so a
    // declared `return` can actually be computed (or explained).
    let mut solve_env = env.clone();
    for (n, v) in &eval_result.bindings {
        solve_env.bind(n.clone(), sysml_core::sim::expr::Value::Number(*v));
    }
    let mut equations = Vec::new();
    for file_path in files {
        if let Ok(source) = std::fs::read_to_string(file_path) {
            for c in sysml_core::sim::constraint_eval::extract_constraints(
                &file_path.to_string_lossy(),
                &source,
            ) {
                // Only constraints declared inside this analysis case
                if c.span.start_byte >= case.span.start_byte
                    && c.span.end_byte <= case.span.end_byte
                {
                    if let Some(expr) = c.expression {
                        equations.push(expr);
                    }
                }
            }
        }
    }
    let solve = sysml_core::sim::analysis::solve_equations(&equations, &mut solve_env);
    let return_value = eval_result.return_value.or_else(|| {
        case.return_decl
            .as_ref()
            .and_then(|r| solve_env.get(&r.name))
            .and_then(|v| v.as_number())
    });

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
                "solved": solve.solved.iter().map(|(n, v)| {
                    serde_json::json!({"name": n, "value": v})
                }).collect::<Vec<_>>(),
                "unbound": solve.unbound,
                "return_value": return_value,
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
            if !solve.solved.is_empty() {
                println!("  Solved:");
                for (n, v) in &solve.solved {
                    println!("    {} = {:.4}", n, v);
                }
            }
            if let Some(ref ret) = case.return_decl {
                let computed = return_value
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
                if return_value.is_none() {
                    // Only fail when computation was clearly intended:
                    // equations or a value expression exist but couldn't
                    // produce a value. A purely structural case just echoes.
                    if !equations.is_empty() && !solve.unbound.is_empty() {
                        eprintln!(
                            "error: could not compute `{}` — unbound: {}",
                            ret.name,
                            solve.unbound.join(", ")
                        );
                        eprintln!(
                            "hint: bind unknowns with -b (e.g. -b {}=<value>)",
                            solve.unbound[0]
                        );
                        return ExitCode::FAILURE;
                    }
                    if ret.value_expr.is_some() || !equations.is_empty() {
                        eprintln!(
                            "error: could not compute `{}` — no equation defines it",
                            ret.name
                        );
                        return ExitCode::FAILURE;
                    }
                    println!(
                        "  (no value expression or constraint equations — nothing to compute)"
                    );
                }
            }
        }
    }

    ExitCode::SUCCESS
}

fn run_trade(cli: &Cli, files: &[PathBuf], name: Option<&str>) -> ExitCode {
    let Some(model) = parse_models(cli, files) else {
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
