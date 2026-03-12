use std::path::Path;
use std::process::ExitCode;

use sysml_core::config::ProjectConfig;

use crate::Cli;

/// Gitignore entries managed by `sysml init`.
const GITIGNORE_ENTRIES: &[&str] = &[".sysml/cache.db", ".sysml/cache.db-journal"];

pub(crate) fn run(_cli: &Cli, force: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {e}");
            return ExitCode::from(1);
        }
    };

    let dot_sysml = cwd.join(".sysml");
    let config_path = dot_sysml.join("config.toml");

    // Guard against overwriting an existing config unless --force is given.
    if config_path.exists() && !force {
        eprintln!(
            "error: {} already exists (use --force to overwrite)",
            config_path.display()
        );
        return ExitCode::from(1);
    }

    // Build a default config, auto-detecting model_root and library paths.
    let mut config = ProjectConfig::default();
    config.project.model_root = detect_model_root(&cwd);

    // Auto-detect libraries/ directory
    let lib_dir = cwd.join("libraries");
    if lib_dir.is_dir() && has_sysml_files(&lib_dir) {
        config.project.library_paths.push(std::path::PathBuf::from("libraries/"));
    }

    // Auto-detect SysML v2 standard library submodule
    let stdlib_candidates = [
        "sysml-v2-release/sysml.library",
        "SysML-v2-Release/sysml.library",
    ];
    for candidate in &stdlib_candidates {
        let stdlib_dir = cwd.join(candidate);
        if stdlib_dir.is_dir() {
            config.project.stdlib_path = Some(std::path::PathBuf::from(*candidate));
            break;
        }
    }

    // Try to derive a project name from the directory name.
    if let Some(name) = cwd.file_name().and_then(|n| n.to_str()) {
        config.project.name = name.to_string();
    }

    // Create .sysml/ directory.
    if let Err(e) = std::fs::create_dir_all(&dot_sysml) {
        eprintln!("error: cannot create {}: {e}", dot_sysml.display());
        return ExitCode::from(1);
    }

    // Write config.toml.
    let toml_content = config.to_toml_string();
    if let Err(e) = std::fs::write(&config_path, &toml_content) {
        eprintln!("error: cannot write {}: {e}", config_path.display());
        return ExitCode::from(1);
    }

    // Update .gitignore with cache entries.
    update_gitignore(&cwd);

    println!("Initialized SysML project in {}", config_path.display());
    if config.project.model_root != Path::new(".") {
        println!(
            "  model_root = \"{}\"",
            config.project.model_root.display()
        );
    }
    if let Some(ref stdlib) = config.project.stdlib_path {
        println!("  stdlib_path = \"{}\"", stdlib.display());
    }
    if !config.project.library_paths.is_empty() {
        for lib in &config.project.library_paths {
            println!("  library_path = \"{}\"", lib.display());
        }
    }

    ExitCode::SUCCESS
}

/// Detect where model files live.
///
/// If a `model/` subdirectory exists and contains `.sysml` files, use that.
/// If the current directory itself contains `.sysml` files, use `.` (default).
/// Otherwise fall back to `.`.
fn detect_model_root(cwd: &Path) -> std::path::PathBuf {
    let model_dir = cwd.join("model");
    if model_dir.is_dir() && has_sysml_files(&model_dir) {
        return std::path::PathBuf::from("model/");
    }
    if has_sysml_files(cwd) {
        return std::path::PathBuf::from(".");
    }
    std::path::PathBuf::from(".")
}

/// Returns `true` if `dir` directly contains at least one `.sysml` file.
fn has_sysml_files(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().extension().is_some_and(|ext| ext == "sysml"))
}

/// Ensure `.gitignore` contains our cache entries.
///
/// If `.gitignore` exists, appends missing entries. If it does not exist but
/// the directory is a git repo (`.git/` present), creates a new `.gitignore`.
fn update_gitignore(cwd: &Path) {
    let gitignore_path = cwd.join(".gitignore");
    let is_git_repo = cwd.join(".git").exists();

    // Only touch .gitignore if the file already exists or this is a git repo.
    if !gitignore_path.exists() && !is_git_repo {
        return;
    }

    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let existing_lines: Vec<&str> = existing.lines().collect();

    let mut to_add = Vec::new();
    for entry in GITIGNORE_ENTRIES {
        if !existing_lines.iter().any(|line| line.trim() == *entry) {
            to_add.push(*entry);
        }
    }

    if to_add.is_empty() {
        return;
    }

    let mut content = existing;
    // Ensure we start on a fresh line.
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }

    // Add a blank separator if the file has content and doesn't end with two newlines.
    if !content.is_empty() && !content.ends_with("\n\n") {
        content.push('\n');
    }

    content.push_str("# sysml-cli cache\n");
    for entry in &to_add {
        content.push_str(entry);
        content.push('\n');
    }

    if let Err(e) = std::fs::write(&gitignore_path, content) {
        eprintln!(
            "warning: could not update {}: {e}",
            gitignore_path.display()
        );
    }
}
