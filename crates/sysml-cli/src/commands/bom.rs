/// Bill of materials CLI commands.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::BomCommand;

pub fn run(cli: &crate::Cli, kind: &BomCommand) -> ExitCode {
    match kind {
        BomCommand::Rollup {
            files,
            root,
            include_mass,
            include_cost,
        } => run_rollup(cli, files, root, *include_mass, *include_cost),
        BomCommand::WhereUsed { files, part } => run_where_used(cli, files, part),
        BomCommand::Export { files, root, format } => run_export(cli, files, root, format),
        BomCommand::Cost {
            files,
            root,
            quantity,
            apply,
        } => run_cost(cli, files, root, *quantity, *apply),
    }
}

fn run_rollup(
    cli: &crate::Cli,
    files: &[PathBuf],
    root: &str,
    include_mass: bool,
    include_cost: bool,
) -> ExitCode {
    let models = match parse_files(files) {
        Some(m) => m,
        None => return ExitCode::FAILURE,
    };

    // Merge all models to search across files.
    let merged = merge_models(&models);

    let tree = match sysml_bom::build_bom_tree(&merged, root) {
        Some(t) => t,
        None => {
            eprintln!("error: no part definition `{root}` found in model");
            return ExitCode::FAILURE;
        }
    };

    if cli.format == "json" {
        let summary = sysml_bom::bom_summary(&tree);
        let output = serde_json::json!({
            "tree": serde_json::to_value(&tree).unwrap_or_default(),
            "summary": serde_json::to_value(&summary).unwrap_or_default(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
    } else {
        let text = sysml_bom::format_bom_tree(&tree, include_mass, include_cost);
        print!("{text}");

        let summary = sysml_bom::bom_summary(&tree);
        if !cli.quiet {
            eprintln!(
                "BOM: {} total parts, {} unique, depth {}",
                summary.total_parts, summary.unique_parts, summary.max_depth,
            );
            if let Some(mass) = summary.total_mass_kg {
                eprintln!("  Total mass: {mass:.3} kg");
            }
            if let Some(cost) = summary.total_cost {
                eprintln!("  Total cost: {cost:.2}");
            }
        }
    }

    ExitCode::SUCCESS
}

fn run_where_used(cli: &crate::Cli, files: &[PathBuf], part: &str) -> ExitCode {
    let models = match parse_files(files) {
        Some(m) => m,
        None => return ExitCode::FAILURE,
    };

    let merged = merge_models(&models);
    let parents = sysml_bom::where_used(&merged, part);

    if cli.format == "json" {
        println!("{}", serde_json::to_string_pretty(&parents).unwrap_or_default());
    } else if parents.is_empty() {
        println!("Part `{part}` is not used in any definition.");
    } else {
        println!("Part `{part}` is used in:");
        for p in &parents {
            println!("  {p}");
        }
    }

    ExitCode::SUCCESS
}

fn run_export(
    cli: &crate::Cli,
    files: &[PathBuf],
    root: &str,
    _format: &str,
) -> ExitCode {
    let models = match parse_files(files) {
        Some(m) => m,
        None => return ExitCode::FAILURE,
    };

    let merged = merge_models(&models);

    let tree = match sysml_bom::build_bom_tree(&merged, root) {
        Some(t) => t,
        None => {
            eprintln!("error: no part definition `{root}` found in model");
            return ExitCode::FAILURE;
        }
    };

    if cli.format == "json" {
        let rows = sysml_bom::flatten_bom(&tree);
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
    } else {
        print!("{}", sysml_bom::format_bom_csv(&tree));
    }

    ExitCode::SUCCESS
}

fn run_cost(
    cli: &crate::Cli,
    files: &[PathBuf],
    root: &str,
    quantity: u32,
    apply: bool,
) -> ExitCode {
    let models = match parse_files(files) {
        Some(m) => m,
        None => return ExitCode::FAILURE,
    };

    let merged = merge_models(&models);

    let tree = match sysml_bom::build_bom_tree(&merged, root) {
        Some(t) => t,
        None => {
            eprintln!("error: no part definition `{root}` found in model");
            return ExitCode::FAILURE;
        }
    };

    // Load quote records from .sysml/records/
    let records_dir = crate::records::resolve_records_dir();
    let records = crate::records::read_records(&records_dir);
    let quotes = sysml_source::load_quotes_from_records(&records);

    if quotes.is_empty() {
        eprintln!("warning: no quote records found in {}", records_dir.display());
        eprintln!("  Create quotes with: sysml source quote");
    }

    let (rows, total_cost) = sysml_bom::costed_bom(&tree, &quotes, quantity);

    if cli.format == "json" {
        let output = serde_json::json!({
            "order_quantity": quantity,
            "rows": serde_json::to_value(&rows).unwrap_or_default(),
            "total_cost": total_cost,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
    } else {
        println!("Costed BOM — {} × {} (order qty {})\n", root, quantity, quantity);
        print!("{}", sysml_bom::format_costed_bom(&rows, total_cost));
    }

    if apply {
        let attrs = sysml_bom::cost_attributes_from_costed_bom(&rows);
        if attrs.is_empty() {
            eprintln!("\nNo prices to apply (no matching quotes found).");
            return ExitCode::SUCCESS;
        }

        let mut applied = 0;
        for file in files {
            let (path, source) = match crate::read_source(file) {
                Ok(ps) => ps,
                Err(_) => continue,
            };
            let model = sysml_core::parser::parse_file(&path, &source);
            let mut current_source = source.clone();

            // Apply in reverse order to preserve byte offsets
            let mut edits_for_file: Vec<_> = attrs.iter()
                .filter(|(def_name, _)| model.find_def(def_name).is_some())
                .collect();
            edits_for_file.reverse();

            for (def_name, attr_line) in &edits_for_file {
                // Re-parse after each edit to get fresh byte offsets
                let fresh_model = sysml_core::parser::parse_file(&path, &current_source);
                match sysml_core::codegen::edit::insert_member(
                    &current_source,
                    &fresh_model,
                    def_name,
                    attr_line,
                ) {
                    Ok(edit) => {
                        match sysml_core::codegen::edit::apply_edits(
                            &current_source,
                            &sysml_core::codegen::edit::EditPlan { edits: vec![edit] },
                        ) {
                            Ok(new_source) => {
                                current_source = new_source;
                                applied += 1;
                            }
                            Err(e) => eprintln!("warning: failed to apply edit for {}: {}", def_name, e),
                        }
                    }
                    Err(e) => eprintln!("warning: skipping {}: {}", def_name, e),
                }
            }

            if applied > 0 {
                if let Err(e) = std::fs::write(file, &current_source) {
                    eprintln!("error: failed to write {}: {}", file.display(), e);
                    return ExitCode::FAILURE;
                }
            }
        }

        if applied > 0 {
            eprintln!("\nApplied unitCost to {} definitions.", applied);
        }
    }

    ExitCode::SUCCESS
}

/// Merge multiple models into a single model for cross-file BOM lookups.
fn merge_models(models: &[sysml_core::model::Model]) -> sysml_core::model::Model {
    let mut merged = sysml_core::model::Model::new("merged".to_string());
    for m in models {
        merged.definitions.extend(m.definitions.iter().cloned());
        merged.usages.extend(m.usages.iter().cloned());
        merged.connections.extend(m.connections.iter().cloned());
        merged.satisfactions.extend(m.satisfactions.iter().cloned());
        merged.verifications.extend(m.verifications.iter().cloned());
    }
    merged
}

fn parse_files(files: &[PathBuf]) -> Option<Vec<sysml_core::model::Model>> {
    let mut models = Vec::new();
    for f in files {
        let (path, source) = match crate::read_source(f) {
            Ok(ps) => ps,
            Err(_) => return None,
        };
        models.push(sysml_core::parser::parse_file(&path, &source));
    }
    Some(models)
}
