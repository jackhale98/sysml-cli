use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{read_source, Cli};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    _cli: &Cli,
    file: &PathBuf,
    diagram_type: &Option<String>,
    output_format: &str,
    scope: Option<&str>,
    view: Option<&str>,
    direction: Option<&str>,
    depth: Option<usize>,
) -> ExitCode {
    use sysml_core::diagram::*;

    // Validate an explicit --type up front (before any file IO).
    let explicit_kind = match diagram_type {
        Some(t) => match DiagramKind::from_str(t) {
            Some(k) => Some(k),
            None => {
                eprintln!(
                    "error: unknown diagram type `{}`. Available: bdd, ibd, stm, act, req, pkg, par, trace, alloc, ucd",
                    t
                );
                return ExitCode::from(1);
            }
        },
        None => None,
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

    // Without --type, take the diagram kind from the view's `render as` clause.
    let kind = match explicit_kind {
        Some(k) => k,
        None => {
            let from_view = view.and_then(|view_name| {
                model
                    .views
                    .iter()
                    .find(|v| v.name == view_name)
                    .and_then(|v| v.render_as.as_deref())
                    .and_then(DiagramKind::from_render_clause)
            });
            match from_view {
                Some(k) => k,
                None => {
                    eprintln!("error: --type is required (or use --view with a view def that has a 'render as' clause)");
                    return ExitCode::FAILURE;
                }
            }
        }
    };

    let mut graph = match kind {
        DiagramKind::GeneralView(GeneralViewFlavor::Default) => build_bdd(&model, scope),
        DiagramKind::InterconnectionView => {
            let def_name = match scope {
                Some(s) => s,
                None => {
                    eprintln!("error: ibd requires --scope <DefinitionName>");
                    return ExitCode::from(1);
                }
            };
            build_ibd(&model, def_name)
        }
        DiagramKind::StateTransitionView => {
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
        DiagramKind::ActionFlowView => {
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
        DiagramKind::GridView(GridViewFlavor::Requirements) => build_req(&model),
        DiagramKind::BrowserView => build_pkg(&model),
        DiagramKind::GeneralView(GeneralViewFlavor::Parametric) => build_par(&model, scope),
        DiagramKind::GridView(GridViewFlavor::Trace) => build_trace(&model),
        DiagramKind::GridView(GridViewFlavor::Alloc) => build_alloc(&model),
        DiagramKind::GeneralView(GeneralViewFlavor::UseCase) => build_ucd(&model),
        DiagramKind::SequenceView => {
            // Sequence view: build from flows/messages
            build_sv(&model, scope)
        }
        DiagramKind::GridView(GridViewFlavor::Default) => build_trace(&model),
    };

    graph.direction = layout_dir;
    graph.max_depth = depth;

    // Apply view filter if specified
    if let Some(view_name) = view {
        match apply_view_filter(&mut graph, &model, view_name) {
            Ok(0) => {
                eprintln!("warning: view `{}` matches no elements", view_name);
            }
            Ok(_) => {}
            Err(msg) => {
                eprintln!("error: {}", msg);
                return ExitCode::FAILURE;
            }
        }
    }

    let output = render(&graph, format);
    println!("{}", output);

    ExitCode::SUCCESS
}
