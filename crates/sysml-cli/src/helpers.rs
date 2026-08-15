//! Shared helper functions for the CLI.

use std::path::PathBuf;
use std::process::ExitCode;

/// Read a SysML source file, returning (path_string, contents) or an error
/// exit code. The path `-` reads from stdin.
pub(crate) fn read_source(file: &PathBuf) -> Result<(String, String), ExitCode> {
    let path_str = file.to_string_lossy().to_string();
    if path_str == "-" {
        use std::io::Read;
        let mut buf = String::new();
        return match std::io::stdin().read_to_string(&mut buf) {
            Ok(_) => Ok(("<stdin>".to_string(), buf)),
            Err(e) => {
                eprintln!("error: cannot read stdin: {}", e);
                Err(ExitCode::from(1))
            }
        };
    }
    match std::fs::read_to_string(file) {
        Ok(s) => Ok((path_str, s)),
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path_str, e);
            Err(ExitCode::from(1))
        }
    }
}

/// Resolve the effective file set for a command: explicit args or project
/// discovery, plus every include path (`-I`, `--stdlib-path`, config).
/// Include-path files come after the primary files, so "the files the user
/// named" are always the prefix of the returned list (their count is the
/// second tuple field).
pub(crate) fn resolve_files(cli: &crate::Cli, files: &[PathBuf]) -> (Vec<PathBuf>, usize) {
    let (mut all, _) = files_or_project(files, cli.quiet);
    let primary = all.len();
    for inc in resolve_include_paths(cli) {
        if inc.is_dir() {
            collect_files_recursive(&inc, &mut all);
        } else if !all.contains(&inc) {
            all.push(inc);
        }
    }
    (all, primary)
}

/// Load one merged model for a whole-project command: resolved file set,
/// every model field merged (`Model::merge`), unreadable files are hard
/// errors. This is THE loader — commands must not hand-roll merging.
pub(crate) fn load_model(cli: &crate::Cli, files: &[PathBuf]) -> Option<sysml_core::model::Model> {
    let models = load_per_file_models(cli, files)?;
    let mut merged = sysml_core::model::Model::new("<project>".to_string());
    for m in models {
        merged.merge(m);
    }
    Some(merged)
}

/// Load per-file models (real per-file byte spans preserved) for commands
/// that need span-accurate association, e.g. the uncertainty analyzer.
pub(crate) fn load_per_file_models(
    cli: &crate::Cli,
    files: &[PathBuf],
) -> Option<Vec<sysml_core::model::Model>> {
    let (all, _) = resolve_files(cli, files);
    if all.is_empty() {
        eprintln!("error: no SysML files found.");
        return None;
    }
    let mut models = Vec::new();
    for file_path in &all {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(_) => return None,
        };
        models.push(sysml_core::parser::parse_file(&path_str, &source));
    }
    Some(models)
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
                if (ext == "sysml" || ext == "kerml") && !files.contains(&path) {
                    files.push(path);
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
    match FuzzySelect::new().items(items).default(0).interact_opt() {
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
/// Shows available signal triggers and lets the user pick events one at a
/// time. Returns None when not interactive — callers must abort rather
/// than silently simulate with zero events.
pub(crate) fn prompt_events(available_signals: &[String]) -> Option<Vec<String>> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        eprintln!("error: this state machine requires events. Use --events to specify them.");
        eprintln!("  available signals: {}", available_signals.join(", "));
        return None;
    }

    let mut events = Vec::new();
    let mut items: Vec<String> = available_signals.to_vec();
    items.push("[done — run simulation]".to_string());

    eprintln!("This state machine has signal triggers. Select events to inject:");
    eprintln!("  (select [done] when finished)");

    loop {
        let selection = FuzzySelect::new().items(&items).default(0).interact_opt();

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

    Some(events)
}

/// The project config, discovered by walking up from the cwd (same rule
/// as file discovery — the two used to disagree: files walked up, library
/// resolution looked only at `./.sysml/config.toml`). Relative paths in
/// the config resolve against the project root.
fn discovered_config() -> Option<(PathBuf, sysml_core::config::ProjectConfig)> {
    let cwd = std::env::current_dir().unwrap_or_default();
    sysml_core::project::discover_project(&cwd).map(|(root, cfg)| (root.to_path_buf(), cfg))
}

fn config_relative(root: &std::path::Path, p: &std::path::Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// The `[gates]` section of the discovered project config (empty
/// defaults when no project).
pub(crate) fn discovered_gates() -> sysml_core::config::GatesSection {
    discovered_config()
        .map(|(_, cfg)| cfg.gates)
        .unwrap_or_default()
}

/// Resolve the effective include paths by combining:
/// 1. CLI `--include` paths
/// 2. `--stdlib-path` (CLI flag or SYSML_STDLIB_PATH env)
/// 3. `.sysml/config.toml` `stdlib_path` + `library_paths` (project-root
///    relative, discovered by walking up)
pub(crate) fn resolve_include_paths(cli: &crate::Cli) -> Vec<PathBuf> {
    let mut paths = cli.include.clone();

    // Add stdlib path from CLI/env
    if let Some(ref stdlib) = cli.stdlib_path {
        if stdlib.is_dir() && !paths.contains(stdlib) {
            paths.push(stdlib.clone());
        }
    }

    if let Some((root, config)) = discovered_config() {
        // Add stdlib from config if not already provided via CLI/env
        if cli.stdlib_path.is_none() {
            if let Some(stdlib) = config.project.stdlib_path {
                let stdlib = config_relative(&root, &stdlib);
                if stdlib.is_dir() && !paths.contains(&stdlib) {
                    paths.push(stdlib);
                }
            }
        }

        // Always add library_paths from config
        for lib_path in &config.project.library_paths {
            let lib_path = config_relative(&root, lib_path);
            if lib_path.is_dir() && !paths.contains(&lib_path) {
                paths.push(lib_path);
            }
        }
    }

    paths
}

/// Resolve project include paths from config only (no CLI flags needed).
///
/// Reads the discovered `.sysml/config.toml` for `library_paths` and
/// `stdlib_path`. Used by commands that need library resolution without
/// the Cli struct.
pub(crate) fn resolve_project_includes() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some((root, config)) = discovered_config() {
        if let Some(stdlib) = config.project.stdlib_path {
            let stdlib = config_relative(&root, &stdlib);
            if stdlib.is_dir() {
                paths.push(stdlib);
            }
        }
        for lib_path in &config.project.library_paths {
            let lib_path = config_relative(&root, lib_path);
            if lib_path.is_dir() && !paths.contains(&lib_path) {
                paths.push(lib_path);
            }
        }
    }
    paths
}

/// Resolve file arguments: if files are provided use them, otherwise
/// discover project files automatically.
///
/// Discovery order:
/// 1. Explicit file arguments (use as-is)
/// 2. `SYSML_PROJECT_ROOT` environment variable
/// 3. `.sysml/config.toml` discovery (walk up from cwd)
/// 4. Current directory scan (fallback)
///
/// Returns the file list and whether discovery was used (for messaging).
/// Discovery notices go to stderr unless `quiet`.
pub(crate) fn files_or_project(files: &[PathBuf], quiet: bool) -> (Vec<PathBuf>, bool) {
    if !files.is_empty() {
        return (files.to_vec(), false);
    }

    // Check environment variable
    if let Ok(root) = std::env::var("SYSML_PROJECT_ROOT") {
        let root_path = PathBuf::from(&root);
        if root_path.is_dir() {
            let mut found = Vec::new();
            collect_files_recursive(&root_path, &mut found);
            if !found.is_empty() {
                if !quiet {
                    eprintln!("info: using project root from SYSML_PROJECT_ROOT={}", root);
                }
                return (found, true);
            }
        }
    }

    // Discover project via .sysml/config.toml
    let cwd = std::env::current_dir().unwrap_or_default();
    if let Some((project_root, config)) = sysml_core::project::discover_project(&cwd) {
        // Use model_root from config if set, otherwise project root
        let model_dir = if config.project.model_root.as_os_str().is_empty() {
            project_root.clone()
        } else if config.project.model_root.is_absolute() {
            config.project.model_root.clone()
        } else {
            project_root.join(&config.project.model_root)
        };
        let mut found = Vec::new();
        collect_files_recursive(&model_dir.to_path_buf(), &mut found);
        if !found.is_empty() {
            if !quiet {
                eprintln!(
                    "info: discovered project at {} ({} files)",
                    project_root.display(),
                    found.len()
                );
            }
            return (found, true);
        }
    }

    // Fallback: scan current directory
    let mut found = Vec::new();
    collect_files_recursive(&cwd.to_path_buf(), &mut found);
    if !found.is_empty() {
        if !quiet {
            eprintln!("info: scanning current directory ({} files)", found.len());
        }
        return (found, true);
    }

    (Vec::new(), false)
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
            eprintln!(
                "error: unknown shell `{}`. Use: bash, zsh, fish, elvish, powershell",
                other
            );
            return;
        }
    };

    let mut cmd = crate::Cli::command();
    generate(shell, &mut cmd, "sysml", &mut std::io::stdout());
}
