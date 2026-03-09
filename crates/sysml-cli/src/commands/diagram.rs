use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, read_source};

pub(crate) fn run(
    _cli: &Cli,
    file: &PathBuf,
    diagram_type: &str,
    output_format: &str,
    scope: Option<&str>,
    direction: Option<&str>,
    depth: Option<usize>,
) -> ExitCode {
    use sysml_core::diagram::*;

    let kind = match DiagramKind::from_str(diagram_type) {
        Some(k) => k,
        None => {
            eprintln!(
                "error: unknown diagram type `{}`. Available: bdd, ibd, stm, act, req, pkg, par",
                diagram_type
            );
            return ExitCode::from(1);
        }
    };

    let format = match DiagramFormat::from_str(output_format) {
        Some(f) => f,
        None => {
            eprintln!(
                "error: unknown output format `{}`. Available: mermaid, plantuml (puml), dot, d2",
                output_format
            );
            return ExitCode::from(1);
        }
    };

    let layout_dir = if let Some(d) = direction {
        match LayoutDirection::from_str(d) {
            Some(dir) => dir,
            None => {
                eprintln!(
                    "error: unknown direction `{}`. Available: TB, LR, BT, RL",
                    d
                );
                return ExitCode::from(1);
            }
        }
    } else {
        LayoutDirection::default()
    };

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let model = sysml_parser::parse_file(&path_str, &source);

    let mut graph = match kind {
        DiagramKind::Bdd => build_bdd(&model, scope),
        DiagramKind::Ibd => {
            let def_name = match scope {
                Some(s) => s,
                None => {
                    eprintln!("error: ibd requires --scope <DefinitionName>");
                    return ExitCode::from(1);
                }
            };
            build_ibd(&model, def_name)
        }
        DiagramKind::Stm => {
            // Try rich STM from state_parser first
            use sysml_core::sim::state_parser::extract_state_machines;
            let machines = extract_state_machines(&path_str, &source);
            let machine = if let Some(s) = scope {
                machines.iter().find(|m| m.name == s)
            } else {
                machines.first()
            };
            if let Some(sm) = machine {
                build_stm_from_state_machine(sm)
            } else {
                build_stm(&model, scope)
            }
        }
        DiagramKind::Act => {
            use sysml_core::sim::action_parser::extract_actions;
            let actions = extract_actions(&path_str, &source);
            let action = if let Some(s) = scope {
                actions.iter().find(|a| a.name == s)
            } else {
                actions.first()
            };
            match action {
                Some(a) => build_act_from_action_model(a),
                None => {
                    eprintln!("error: no action definitions found in `{}`", path_str);
                    return ExitCode::from(1);
                }
            }
        }
        DiagramKind::Req => build_req(&model),
        DiagramKind::Pkg => build_pkg(&model),
        DiagramKind::Par => build_par(&model, scope),
    };

    graph.direction = layout_dir;
    graph.max_depth = depth;

    let output = render(&graph, format);
    println!("{}", output);

    ExitCode::SUCCESS
}
