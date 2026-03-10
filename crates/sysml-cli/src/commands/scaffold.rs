/// Scaffolding CLI commands — example project generation.

use std::path::PathBuf;
use std::process::ExitCode;

/// Entry point for the `example` top-level command.
pub fn run_example_command(
    name: Option<&str>,
    output: Option<&PathBuf>,
    list: bool,
) -> ExitCode {
    if list || name.is_none() {
        return run_list_examples();
    }
    run_example(name.unwrap(), output)
}

fn run_example(name: &str, output: Option<&PathBuf>) -> ExitCode {
    let files = match sysml_scaffold::scaffold_example(name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {}", e);
            let examples = sysml_scaffold::list_examples();
            eprintln!("Available examples:");
            for (n, desc) in examples {
                eprintln!("  {:<20} {}", n, desc);
            }
            return ExitCode::FAILURE;
        }
    };

    let out_dir = output.cloned().unwrap_or_else(|| PathBuf::from("."));

    for (filename, content) in &files {
        let path = out_dir.join(filename);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("error: cannot create directory `{}`: {}", parent.display(), e);
                    return ExitCode::FAILURE;
                }
            }
        }

        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("error: cannot write `{}`: {}", path.display(), e);
            return ExitCode::FAILURE;
        }
        eprintln!("  created {}", path.display());
    }

    eprintln!("Example `{}` scaffolded ({} files).", name, files.len());
    ExitCode::SUCCESS
}

fn run_list_examples() -> ExitCode {
    let examples = sysml_scaffold::list_examples();
    println!("Available example projects:");
    for (name, desc) in examples {
        println!("  {:<20} {}", name, desc);
    }
    ExitCode::SUCCESS
}
