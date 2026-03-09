use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, ExportCommand, read_source};

pub(crate) fn run(cli: &Cli, kind: &ExportCommand) -> ExitCode {
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
    use sysml_core::export::fmi;

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
    use sysml_core::export::{fmi, modelica};

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
    use sysml_core::export::ssp;

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
    use sysml_core::export::fmi;

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
