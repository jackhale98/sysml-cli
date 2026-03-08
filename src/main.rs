/// sysml2-cli: SysML v2 model validator, simulator, and FMI export tool.
///
/// Uses tree-sitter to parse SysML v2 files and runs structural
/// validation checks and behavioral simulations.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use sysml2_cli::checks::{self, Check};
use sysml2_cli::diagnostic::{Diagnostic, Severity};
use sysml2_cli::output;
use sysml2_cli::parser as sysml_parser;

#[derive(Parser)]
#[command(
    name = "sysml2-cli",
    about = "SysML v2 model validator, simulator, and FMI export tool",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format: text, json.
    #[arg(short, long, default_value = "text", global = true)]
    format: String,

    /// Suppress summary line on stderr.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Additional SysML files or directories to include for import resolution.
    /// Definitions from these files are available to imported names.
    #[arg(short = 'I', long = "include", global = true)]
    include: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Lint SysML v2 files for structural issues.
    Lint {
        /// SysML v2 files to validate.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Disable specific checks (comma-separated).
        /// Available: syntax, duplicates, unused, unresolved, unsatisfied, unverified, port-types, constraints, calculations
        #[arg(short, long, value_delimiter = ',')]
        disable: Vec<String>,

        /// Minimum severity to report: note, warning, error.
        #[arg(short, long, default_value = "note")]
        severity: String,
    },
    /// Run simulations on SysML v2 models.
    Simulate {
        #[command(subcommand)]
        kind: SimulateCommand,
    },
    /// Export FMI/SSP artifacts from SysML models.
    Export {
        #[command(subcommand)]
        kind: ExportCommand,
    },
}

#[derive(Subcommand)]
enum SimulateCommand {
    /// Evaluate constraints and calculations with variable bindings.
    Eval {
        /// SysML v2 file containing constraints/calculations.
        #[arg(required = true)]
        file: PathBuf,
        /// Variable bindings in the form name=value (e.g., speed=100).
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
        /// Name of a specific constraint or calculation to evaluate.
        #[arg(short = 'n', long)]
        name: Option<String>,
    },
    /// Simulate a state machine.
    StateMachine {
        /// SysML v2 file containing state machine definitions.
        #[arg(required = true)]
        file: PathBuf,
        /// Name of the state machine to simulate.
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// Events to inject (comma-separated).
        #[arg(short = 'e', long, value_delimiter = ',')]
        events: Vec<String>,
        /// Maximum simulation steps.
        #[arg(short = 'm', long, default_value = "100")]
        max_steps: usize,
        /// Variable bindings for guards (name=value).
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
    },
    /// Execute an action flow.
    ActionFlow {
        /// SysML v2 file containing action definitions.
        #[arg(required = true)]
        file: PathBuf,
        /// Name of the action to execute.
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// Maximum execution steps.
        #[arg(short = 'm', long, default_value = "1000")]
        max_steps: usize,
        /// Variable bindings (name=value).
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
    },
    /// List all simulatable constructs in a file.
    List {
        /// SysML v2 file to inspect.
        #[arg(required = true)]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ExportCommand {
    /// Extract FMI interface items from a part definition.
    Interfaces {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Part definition name.
        #[arg(short, long)]
        part: String,
    },
    /// Generate Modelica partial model stub.
    Modelica {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Part definition name.
        #[arg(short, long)]
        part: String,
        /// Output file path (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate SSP SystemStructureDescription XML.
    Ssp {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Output file path (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List exportable parts and their interfaces.
    List {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Command::Lint {
            files,
            disable,
            severity,
        } => run_lint(&cli, files, disable, severity),
        Command::Simulate { kind } => run_simulate(&cli, kind),
        Command::Export { kind } => run_export(&cli, kind),
    }
}

fn run_lint(cli: &Cli, files: &[PathBuf], disable: &[String], severity: &str) -> ExitCode {
    let disabled: HashSet<&str> = disable.iter().map(|s| s.as_str()).collect();
    let min_severity = match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => Severity::Note,
    };

    let active_checks: Vec<Box<dyn Check>> = checks::all_checks()
        .into_iter()
        .filter(|c| !disabled.contains(c.name()))
        .collect();

    // Build project resolver if includes are specified
    let project = if !cli.include.is_empty() {
        let mut all_files: Vec<PathBuf> = files.to_vec();
        for inc in &cli.include {
            if inc.is_dir() {
                collect_files_recursive(inc, &mut all_files);
            } else {
                all_files.push(inc.clone());
            }
        }
        Some(sysml2_cli::resolver::Project::from_files(&all_files))
    } else if files.len() > 1 {
        // Multi-file lint: auto-resolve imports between the given files
        Some(sysml2_cli::resolver::Project::from_files(files))
    } else {
        None
    };

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut had_parse_error = false;

    for file_path in files {
        let path_str = file_path.to_string_lossy().to_string();

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                had_parse_error = true;
                continue;
            }
        };

        let mut model = sysml_parser::parse_file(&path_str, &source);

        // Resolve imports if project is available
        if let Some(ref proj) = project {
            model.resolved_imports = proj.resolve_imports(&model);
        }

        for check in &active_checks {
            let diagnostics = check.run(&model);
            for d in diagnostics {
                if d.severity >= min_severity {
                    all_diagnostics.push(d);
                }
            }
        }
    }

    all_diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.span.start_row.cmp(&b.span.start_row))
            .then(a.span.start_col.cmp(&b.span.start_col))
    });

    if !all_diagnostics.is_empty() {
        let output = match cli.format.as_str() {
            "json" => output::format_json(&all_diagnostics),
            _ => output::format_text(&all_diagnostics),
        };
        println!("{}", output);
    }

    if !cli.quiet {
        output::print_summary(&all_diagnostics);
    }

    let has_errors = all_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    if has_errors || had_parse_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_simulate(cli: &Cli, kind: &SimulateCommand) -> ExitCode {
    match kind {
        SimulateCommand::Eval {
            file,
            bindings,
            name,
        } => run_sim_eval(cli, file, bindings, name.as_deref()),
        SimulateCommand::StateMachine {
            file,
            name,
            events,
            max_steps,
            bindings,
        } => run_sim_state_machine(cli, file, name.as_deref(), events, *max_steps, bindings),
        SimulateCommand::ActionFlow {
            file,
            name,
            max_steps,
            bindings,
        } => run_sim_action_flow(cli, file, name.as_deref(), *max_steps, bindings),
        SimulateCommand::List { file } => run_sim_list(cli, file),
    }
}

fn parse_bindings(bindings: &[String]) -> sysml2_cli::sim::expr::Env {
    use sysml2_cli::sim::expr::{Env, Value};
    let mut env = Env::new();
    for b in bindings {
        if let Some((name, val_str)) = b.split_once('=') {
            let value = if let Ok(n) = val_str.parse::<f64>() {
                Value::Number(n)
            } else if val_str == "true" {
                Value::Bool(true)
            } else if val_str == "false" {
                Value::Bool(false)
            } else {
                Value::String(val_str.to_string())
            };
            env.bind(name.trim(), value);
        }
    }
    env
}

fn collect_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files_recursive(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "sysml" || ext == "kerml" {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }
    }
}

fn read_source(file: &PathBuf) -> Result<(String, String), ExitCode> {
    let path_str = file.to_string_lossy().to_string();
    match std::fs::read_to_string(file) {
        Ok(s) => Ok((path_str, s)),
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path_str, e);
            Err(ExitCode::from(1))
        }
    }
}

fn run_sim_eval(
    cli: &Cli,
    file: &PathBuf,
    bindings: &[String],
    name: Option<&str>,
) -> ExitCode {
    use sysml2_cli::sim::constraint_eval::*;
    use sysml2_cli::sim::eval;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let env = parse_bindings(bindings);

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);

    let target_constraints: Vec<&ConstraintModel> = if let Some(n) = name {
        constraints.iter().filter(|c| c.name == n).collect()
    } else {
        constraints.iter().collect()
    };

    let target_calcs: Vec<&CalcModel> = if let Some(n) = name {
        calcs.iter().filter(|c| c.name == n).collect()
    } else {
        calcs.iter().collect()
    };

    if target_constraints.is_empty() && target_calcs.is_empty() {
        if let Some(n) = name {
            eprintln!("error: no constraint or calculation named `{}` found", n);
        } else {
            eprintln!("error: no constraints or calculations found in `{}`", path_str);
        }
        return ExitCode::from(1);
    }

    let is_json = cli.format == "json";
    let mut results = Vec::new();

    for c in &target_constraints {
        if let Some(ref expr) = c.expression {
            let result = eval::evaluate_constraint(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "constraint",
                    "name": c.name,
                    "result": match &result {
                        Ok(b) => serde_json::json!(b),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(b) => println!(
                        "constraint {}: {}",
                        c.name,
                        if b { "satisfied" } else { "violated" }
                    ),
                    Err(e) => println!("constraint {}: error: {}", c.name, e),
                }
            }
        }
    }

    for c in &target_calcs {
        if let Some(ref expr) = c.return_expr {
            let result = eval::evaluate(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "calculation",
                    "name": c.name,
                    "result": match &result {
                        Ok(v) => serde_json::json!(v),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(v) => println!("calc {}: {}", c.name, v),
                    Err(e) => println!("calc {}: error: {}", c.name, e),
                }
            }
        }
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    }

    ExitCode::SUCCESS
}

fn run_sim_state_machine(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    events: &[String],
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml2_cli::sim::state_parser::extract_state_machines;
    use sysml2_cli::sim::state_sim::*;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let machines = extract_state_machines(&path_str, &source);

    let machine = if let Some(n) = name {
        machines.iter().find(|m| m.name == n)
    } else {
        machines.first()
    };

    let machine = match machine {
        Some(m) => m,
        None => {
            if let Some(n) = name {
                eprintln!("error: no state machine named `{}` found", n);
            } else {
                eprintln!("error: no state machines found in `{}`", path_str);
            }
            return ExitCode::from(1);
        }
    };

    let config = SimConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
        events: events.to_vec(),
    };

    let result = simulate(machine, &config);

    let output = match cli.format.as_str() {
        "json" => format_trace_json(&result),
        _ => format_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        SimStatus::Completed | SimStatus::Running => ExitCode::SUCCESS,
        SimStatus::Deadlocked => ExitCode::from(1),
        SimStatus::MaxSteps => ExitCode::from(2),
    }
}

fn run_sim_action_flow(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml2_cli::sim::action_exec::*;
    use sysml2_cli::sim::action_parser::extract_actions;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let actions = extract_actions(&path_str, &source);

    let action = if let Some(n) = name {
        actions.iter().find(|a| a.name == n)
    } else {
        actions.first()
    };

    let action = match action {
        Some(a) => a,
        None => {
            if let Some(n) = name {
                eprintln!("error: no action named `{}` found", n);
            } else {
                eprintln!("error: no action definitions found in `{}`", path_str);
            }
            return ExitCode::from(1);
        }
    };

    let config = ActionExecConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
    };

    let result = execute_action(action, &config);

    let output = match cli.format.as_str() {
        "json" => format_action_trace_json(&result),
        _ => format_action_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        ActionExecStatus::Completed => ExitCode::SUCCESS,
        ActionExecStatus::Error => ExitCode::from(1),
        ActionExecStatus::MaxSteps => ExitCode::from(2),
        ActionExecStatus::Running => ExitCode::SUCCESS,
    }
}

fn run_sim_list(_cli: &Cli, file: &PathBuf) -> ExitCode {
    use sysml2_cli::sim::action_parser::extract_actions;
    use sysml2_cli::sim::constraint_eval::*;
    use sysml2_cli::sim::state_parser::extract_state_machines;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);
    let machines = extract_state_machines(&path_str, &source);
    let actions = extract_actions(&path_str, &source);

    if constraints.is_empty() && calcs.is_empty() && machines.is_empty() && actions.is_empty() {
        println!("No simulatable constructs found in `{}`.", path_str);
        return ExitCode::SUCCESS;
    }

    if !constraints.is_empty() {
        println!("Constraints:");
        for c in &constraints {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            println!("  {} ({})", c.name, params.join(", "));
        }
        println!();
    }

    if !calcs.is_empty() {
        println!("Calculations:");
        for c in &calcs {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            let ret = c.return_type.as_deref().unwrap_or("?");
            println!("  {} ({}) -> {}", c.name, params.join(", "), ret);
        }
        println!();
    }

    if !machines.is_empty() {
        println!("State Machines:");
        for m in &machines {
            let states: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
            let entry = m.entry_state.as_deref().unwrap_or("?");
            println!(
                "  {} [entry: {}, states: {}, transitions: {}]",
                m.name,
                entry,
                states.join(", "),
                m.transitions.len()
            );
        }
        println!();
    }

    if !actions.is_empty() {
        println!("Actions:");
        for a in &actions {
            println!("  {} ({} steps)", a.name, a.steps.len());
        }
        println!();
    }

    ExitCode::SUCCESS
}

// === Export commands ===

fn run_export(cli: &Cli, kind: &ExportCommand) -> ExitCode {
    match kind {
        ExportCommand::Interfaces { file, part } => run_export_interfaces(cli, file, part),
        ExportCommand::Modelica { file, part, output } => {
            run_export_modelica(cli, file, part, output.as_ref())
        }
        ExportCommand::Ssp { file, output } => run_export_ssp(cli, file, output.as_ref()),
        ExportCommand::List { file } => run_export_list(cli, file),
    }
}

fn run_export_interfaces(cli: &Cli, file: &PathBuf, part: &str) -> ExitCode {
    use sysml2_cli::export::fmi;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model = sysml_parser::parse_file(&path_str, &source);

    match fmi::extract_interface(&model, part) {
        Ok(interface) => {
            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&interface).unwrap());
            } else {
                println!("FMI Interface: {}", interface.part_name);
                println!("{}", "-".repeat(60));
                if interface.items.is_empty() {
                    println!("  No interface items found.");
                } else {
                    println!(
                        "  {:<15} {:<10} {:<12} {:<10} {:<12} {}",
                        "Name", "Direction", "SysML Type", "FMI Type", "Causality", "Port"
                    );
                    println!("  {}", "-".repeat(70));
                    for item in &interface.items {
                        println!(
                            "  {:<15} {:<10} {:<12} {:<10} {:<12} {}",
                            item.name,
                            item.direction,
                            item.sysml_type,
                            item.fmi_type,
                            item.causality,
                            item.source_port,
                        );
                    }
                }
                if !interface.attributes.is_empty() {
                    println!("\n  Attributes:");
                    for attr in &interface.attributes {
                        println!("    {} : {}", attr.name, attr.sysml_type);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(1)
        }
    }
}

fn run_export_modelica(
    _cli: &Cli,
    file: &PathBuf,
    part: &str,
    output: Option<&PathBuf>,
) -> ExitCode {
    use sysml2_cli::export::{fmi, modelica};

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model = sysml_parser::parse_file(&path_str, &source);

    match fmi::extract_interface(&model, part) {
        Ok(interface) => {
            let mo = modelica::generate_modelica(&interface);
            if let Some(out_path) = output {
                match std::fs::write(out_path, &mo) {
                    Ok(_) => {
                        eprintln!("Modelica stub written to {}", out_path.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error writing {}: {}", out_path.display(), e);
                        ExitCode::from(1)
                    }
                }
            } else {
                println!("{}", mo);
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(1)
        }
    }
}

fn run_export_ssp(_cli: &Cli, file: &PathBuf, output: Option<&PathBuf>) -> ExitCode {
    use sysml2_cli::export::ssp;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model = sysml_parser::parse_file(&path_str, &source);
    let structure = ssp::extract_ssp_structure(&model);
    let xml = ssp::generate_ssd_xml(&structure);

    if let Some(out_path) = output {
        match std::fs::write(out_path, &xml) {
            Ok(_) => {
                eprintln!("SSP XML written to {}", out_path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error writing {}: {}", out_path.display(), e);
                ExitCode::from(1)
            }
        }
    } else {
        println!("{}", xml);
        ExitCode::SUCCESS
    }
}

fn run_export_list(cli: &Cli, file: &PathBuf) -> ExitCode {
    use sysml2_cli::export::fmi;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model = sysml_parser::parse_file(&path_str, &source);
    let parts = fmi::list_exportable(&model);

    if parts.is_empty() {
        println!("No exportable parts found in `{}`.", path_str);
        return ExitCode::SUCCESS;
    }

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&parts).unwrap());
    } else {
        println!("Exportable Parts:");
        for p in &parts {
            println!(
                "  {} ({} ports, {} attributes, {} connections)",
                p.name, p.ports, p.attributes, p.connections
            );
        }
    }

    ExitCode::SUCCESS
}
