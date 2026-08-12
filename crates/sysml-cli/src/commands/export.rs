use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{read_source, Cli, ExportCommand};

pub(crate) fn run(cli: &Cli, kind: &ExportCommand) -> ExitCode {
    match kind {
        ExportCommand::Modelica { file, part, output } => {
            run_export_modelica(cli, file, part, output.as_ref())
        }
        ExportCommand::Ssp { file, output } => run_export_ssp(cli, file, output.as_ref()),
        ExportCommand::List { file } => run_export_list(cli, file),
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
