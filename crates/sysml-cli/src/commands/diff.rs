use std::path::PathBuf;
use std::process::ExitCode;
use sysml_core::parser as sysml_parser;
use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, file_a: &PathBuf, file_b: &PathBuf) -> ExitCode {
    use sysml_core::query;

    let (path_a, source_a) = match read_source(file_a) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let (path_b, source_b) = match read_source(file_b) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model_a = sysml_parser::parse_file(&path_a, &source_a);
    let model_b = sysml_parser::parse_file(&path_b, &source_b);
    let diff = query::model_diff(&model_a, &model_b);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&diff).unwrap());
    } else {
        let has_changes = !diff.added_defs.is_empty()
            || !diff.removed_defs.is_empty()
            || !diff.changed_defs.is_empty()
            || !diff.added_usages.is_empty()
            || !diff.removed_usages.is_empty()
            || !diff.changed_usages.is_empty()
            || !diff.added_connections.is_empty()
            || !diff.removed_connections.is_empty();

        if !has_changes {
            println!("No semantic differences found.");
            return ExitCode::SUCCESS;
        }

        println!("Semantic diff: {} -> {}", path_a, path_b);
        println!("{}", "=".repeat(50));

        if !diff.added_defs.is_empty() {
            println!();
            println!("Added definitions:");
            for name in &diff.added_defs {
                println!("  + {}", name);
            }
        }
        if !diff.removed_defs.is_empty() {
            println!();
            println!("Removed definitions:");
            for name in &diff.removed_defs {
                println!("  - {}", name);
            }
        }
        if !diff.changed_defs.is_empty() {
            println!();
            println!("Changed definitions:");
            for change in &diff.changed_defs {
                println!("  ~ {}", change.name);
                for c in &change.changes {
                    println!("      {}", c);
                }
            }
        }
        if !diff.added_usages.is_empty() {
            println!();
            println!("Added usages:");
            for u in &diff.added_usages {
                let parent = u.parent.as_deref().unwrap_or("(top-level)");
                println!("  + {} in {}", u.name, parent);
            }
        }
        if !diff.removed_usages.is_empty() {
            println!();
            println!("Removed usages:");
            for u in &diff.removed_usages {
                let parent = u.parent.as_deref().unwrap_or("(top-level)");
                println!("  - {} in {}", u.name, parent);
            }
        }
        if !diff.changed_usages.is_empty() {
            println!();
            println!("Changed usages:");
            for u in &diff.changed_usages {
                let parent = u.key.parent.as_deref().unwrap_or("(top-level)");
                println!("  ~ {} in {}", u.key.name, parent);
                for c in &u.changes {
                    println!("      {}", c);
                }
            }
        }
        if !diff.added_connections.is_empty() || !diff.removed_connections.is_empty() {
            println!();
            println!("Connection changes:");
            for c in &diff.added_connections {
                println!("  + {}", c);
            }
            for c in &diff.removed_connections {
                println!("  - {}", c);
            }
        }
    }
    ExitCode::SUCCESS
}
