/// Pipeline command: run named validation pipelines from config.

use std::path::Path;
use std::process::{self, ExitCode};

use sysml_core::config::ProjectConfig;

use crate::cli::PipelineCommand;

pub fn run(kind: &PipelineCommand, format: &str, quiet: bool) -> ExitCode {
    match kind {
        PipelineCommand::List => run_list(format, quiet),
        PipelineCommand::Run { name, dry_run } => run_pipeline(name, *dry_run, format, quiet),
        PipelineCommand::Create { name } => run_create(name, quiet),
    }
}

fn load_config() -> Result<ProjectConfig, ExitCode> {
    let config_path = Path::new(".sysml/config.toml");
    if !config_path.exists() {
        eprintln!("error: no .sysml/config.toml found — run `sysml init` first");
        return Err(ExitCode::FAILURE);
    }
    ProjectConfig::load(config_path).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn run_list(format: &str, quiet: bool) -> ExitCode {
    let config = match load_config() {
        Ok(c) => c,
        Err(code) => return code,
    };

    if config.pipelines.is_empty() {
        if format == "json" {
            println!("[]");
        } else {
            println!("No pipelines defined in .sysml/config.toml");
            println!();
            println!("Add a pipeline with:");
            println!("  sysml pipeline create <name>");
            println!();
            println!("Or add to config.toml manually:");
            println!("  [[pipeline]]");
            println!("  name = \"ci\"");
            println!("  steps = [\"lint *.sysml\", \"fmt --check *.sysml\"]");
        }
        return ExitCode::SUCCESS;
    }

    if format == "json" {
        let items: Vec<serde_json::Value> = config
            .pipelines
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "steps": p.steps,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for p in &config.pipelines {
            println!("{}  ({} steps)", p.name, p.steps.len());
            for (i, step) in p.steps.iter().enumerate() {
                println!("  {}. sysml {}", i + 1, step);
            }
        }
    }

    if !quiet {
        eprintln!(
            "{} pipeline(s) defined",
            config.pipelines.len()
        );
    }
    ExitCode::SUCCESS
}

fn run_pipeline(name: &str, dry_run: bool, _format: &str, quiet: bool) -> ExitCode {
    let config = match load_config() {
        Ok(c) => c,
        Err(code) => return code,
    };

    let Some(pipeline) = config.pipelines.iter().find(|p| p.name == name) else {
        eprintln!("error: no pipeline named `{name}`");
        eprintln!();
        eprintln!("Available pipelines:");
        for p in &config.pipelines {
            eprintln!("  {}", p.name);
        }
        return ExitCode::FAILURE;
    };

    if pipeline.steps.is_empty() {
        eprintln!("Pipeline `{name}` has no steps");
        return ExitCode::SUCCESS;
    }

    if dry_run {
        println!("Pipeline: {name}");
        println!();
        for (i, step) in pipeline.steps.iter().enumerate() {
            println!("  {}. sysml {step}", i + 1);
        }
        println!();
        println!("(dry run — no commands executed)");
        return ExitCode::SUCCESS;
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| "sysml".into());

    for (i, step) in pipeline.steps.iter().enumerate() {
        if !quiet {
            eprintln!("[{}/{}] sysml {step}", i + 1, pipeline.steps.len());
        }

        let args = shell_words(step);
        if args.is_empty() {
            continue;
        }

        let status = process::Command::new(&exe)
            .args(&args)
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                let code = s.code().unwrap_or(1);
                eprintln!(
                    "error: step {} failed (exit code {}): sysml {step}",
                    i + 1,
                    code
                );
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("error: failed to execute step {}: {e}", i + 1);
                return ExitCode::FAILURE;
            }
        }
    }

    if !quiet {
        eprintln!("Pipeline `{name}` completed successfully ({} steps)", pipeline.steps.len());
    }
    ExitCode::SUCCESS
}

fn run_create(name: &str, _quiet: bool) -> ExitCode {
    let config_path = Path::new(".sysml/config.toml");
    if !config_path.exists() {
        eprintln!("error: no .sysml/config.toml found — run `sysml init` first");
        return ExitCode::FAILURE;
    }

    let mut config = match ProjectConfig::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if config.pipelines.iter().any(|p| p.name == name) {
        eprintln!("error: pipeline `{name}` already exists");
        return ExitCode::FAILURE;
    }

    let pipeline = sysml_core::config::PipelineConfig {
        name: name.to_string(),
        steps: vec![
            "lint *.sysml".to_string(),
            "fmt --check *.sysml".to_string(),
        ],
    };

    config.pipelines.push(pipeline);

    if let Err(e) = std::fs::write(config_path, config.to_toml_string()) {
        eprintln!("error: failed to write config: {e}");
        return ExitCode::FAILURE;
    }

    println!("Created pipeline `{name}` with example steps");
    println!("Edit .sysml/config.toml to customize the steps");
    ExitCode::SUCCESS
}

/// Simple shell-word splitting (handles double quotes, no single quotes or escapes).
fn shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_simple() {
        assert_eq!(
            shell_words("lint model.sysml"),
            vec!["lint", "model.sysml"]
        );
    }

    #[test]
    fn shell_words_with_flags() {
        assert_eq!(
            shell_words("trace --check --min-coverage 80 *.sysml"),
            vec!["trace", "--check", "--min-coverage", "80", "*.sysml"]
        );
    }

    #[test]
    fn shell_words_quoted() {
        assert_eq!(
            shell_words(r#"lint "path with spaces.sysml""#),
            vec!["lint", "path with spaces.sysml"]
        );
    }

    #[test]
    fn shell_words_empty() {
        assert!(shell_words("").is_empty());
        assert!(shell_words("   ").is_empty());
    }
}
