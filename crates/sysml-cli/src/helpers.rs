/// Shared helper functions for the CLI.

use std::path::PathBuf;
use std::process::ExitCode;

/// Read a SysML source file, returning (path_string, contents) or an error exit code.
pub(crate) fn read_source(file: &PathBuf) -> Result<(String, String), ExitCode> {
    let path_str = file.to_string_lossy().to_string();
    match std::fs::read_to_string(file) {
        Ok(s) => Ok((path_str, s)),
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path_str, e);
            Err(ExitCode::from(1))
        }
    }
}

/// Parse variable bindings from "name=value" strings into a simulation environment.
pub(crate) fn parse_bindings(bindings: &[String]) -> sysml_core::sim::expr::Env {
    use sysml_core::sim::expr::{Env, Value};
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

/// Recursively collect .sysml and .kerml files from a directory.
pub(crate) fn collect_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) {
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

/// Prompt the user to select from a list of items interactively.
/// Returns None if not a TTY or selection fails.
pub(crate) fn select_item(kind: &str, items: &[&str]) -> Option<usize> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        eprintln!(
            "error: multiple {}s found. Use --name to specify one, or run interactively.",
            kind
        );
        eprintln!("  available: {}", items.join(", "));
        return None;
    }

    eprintln!("Multiple {}s found. Select one:", kind);
    match FuzzySelect::new()
        .items(items)
        .default(0)
        .interact_opt()
    {
        Ok(Some(idx)) => Some(idx),
        Ok(None) => {
            eprintln!("No selection made.");
            None
        }
        Err(e) => {
            eprintln!("error: selection failed: {}", e);
            None
        }
    }
}

/// Interactively prompt for events to feed into a state machine simulation.
///
/// Shows available signal triggers and lets the user pick events one at a time.
/// Returns the collected event sequence.
pub(crate) fn prompt_events(available_signals: &[String]) -> Vec<String> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        eprintln!(
            "error: this state machine requires events. Use --events to specify them."
        );
        eprintln!("  available signals: {}", available_signals.join(", "));
        return Vec::new();
    }

    let mut events = Vec::new();
    let mut items: Vec<String> = available_signals.to_vec();
    items.push("[done — run simulation]".to_string());

    eprintln!("This state machine has signal triggers. Select events to inject:");
    eprintln!("  (select [done] when finished)");

    loop {
        let selection = FuzzySelect::new()
            .items(&items)
            .default(0)
            .interact_opt();

        match selection {
            Ok(Some(idx)) if idx < available_signals.len() => {
                events.push(available_signals[idx].clone());
                eprintln!("  events so far: [{}]", events.join(", "));
            }
            Ok(Some(_)) => {
                // Selected "[done]"
                break;
            }
            Ok(None) | Err(_) => {
                break;
            }
        }
    }

    events
}

/// Resolve the effective include paths by combining:
/// 1. CLI `--include` paths
/// 2. `--stdlib-path` (CLI flag or SYSML_STDLIB_PATH env)
/// 3. Config file `stdlib_path` (from `.sysml/config.toml`)
///
/// Returns a combined list of include paths for import resolution.
pub(crate) fn resolve_include_paths(cli: &crate::Cli) -> Vec<PathBuf> {
    let mut paths = cli.include.clone();

    // Add stdlib path from CLI/env
    if let Some(ref stdlib) = cli.stdlib_path {
        if stdlib.is_dir() && !paths.contains(stdlib) {
            paths.push(stdlib.clone());
        }
    }

    // Try loading config for stdlib_path if not already provided
    if cli.stdlib_path.is_none() {
        let config_path = std::path::Path::new(".sysml/config.toml");
        if config_path.exists() {
            if let Ok(config) = sysml_core::config::ProjectConfig::load(config_path) {
                if let Some(stdlib) = config.project.stdlib_path {
                    if stdlib.is_dir() && !paths.contains(&stdlib) {
                        paths.push(stdlib);
                    }
                }
            }
        }
    }

    paths
}

/// Generate shell completions for the given shell.
pub(crate) fn generate_completions(shell: &str) {
    use clap::CommandFactory;
    use clap_complete::{generate, Shell};

    let shell = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "elvish" => Shell::Elvish,
        "powershell" | "ps" => Shell::PowerShell,
        other => {
            eprintln!("error: unknown shell `{}`. Use: bash, zsh, fish, elvish, powershell", other);
            return;
        }
    };

    let mut cmd = crate::Cli::command();
    generate(shell, &mut cmd, "sysml", &mut std::io::stdout());
}
