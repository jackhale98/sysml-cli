use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{Cli, read_source};

pub(crate) fn run(cli: &Cli, file: &PathBuf, element: &str) -> ExitCode {
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    // Try to find as a definition first, then as a usage
    if let Some(def) = model.find_def(element) {
        if cli.format == "json" {
            println!("{}", serde_json::to_string_pretty(def).unwrap());
        } else {
            println!("{} {}", def.kind.label(), def.name);
            if let Some(ref sn) = def.short_name {
                println!("  short name: <{}>", sn);
            }
            if let Some(ref vis) = def.visibility {
                println!("  visibility: {}", vis.label());
            }
            if def.is_abstract {
                println!("  abstract: yes");
            }
            if let Some(ref st) = def.super_type {
                println!("  specializes: {}", st);
            }
            if let Some(ref doc) = def.doc {
                println!("  doc: {}", doc);
            }
            if let Some(ref parent) = def.parent_def {
                println!("  parent: {}", parent);
            }
            println!(
                "  location: {}:{}:{}",
                path_str, def.span.start_row, def.span.start_col
            );

            // Show children (usages inside this def)
            let children = model.usages_in_def(&def.name);
            if !children.is_empty() {
                println!("  members:");
                for u in children {
                    let t = u
                        .type_ref
                        .as_deref()
                        .map(|t| format!(" : {}", t))
                        .unwrap_or_default();
                    let mult = u
                        .multiplicity
                        .as_ref()
                        .map(|m| format!(" {}", m))
                        .unwrap_or_default();
                    println!("    {} {}{}{}", u.kind, u.name, t, mult);
                }
            }

            // Show relationships involving this def
            let sats: Vec<_> = model
                .satisfactions
                .iter()
                .filter(|s| {
                    sysml_core::model::simple_name(&s.requirement) == element
                        || s.by.as_deref() == Some(element)
                })
                .collect();
            if !sats.is_empty() {
                println!("  satisfactions:");
                for s in sats {
                    let by = s.by.as_deref().unwrap_or("(implicit)");
                    println!("    {} satisfied by {}", s.requirement, by);
                }
            }

            let vers: Vec<_> = model
                .verifications
                .iter()
                .filter(|v| {
                    sysml_core::model::simple_name(&v.requirement) == element || v.by == element
                })
                .collect();
            if !vers.is_empty() {
                println!("  verifications:");
                for v in vers {
                    println!("    {} verified by {}", v.requirement, v.by);
                }
            }
        }
        return ExitCode::SUCCESS;
    }

    // Try as usage
    if let Some(usage) = model.usages.iter().find(|u| u.name == element) {
        if cli.format == "json" {
            println!("{}", serde_json::to_string_pretty(usage).unwrap());
        } else {
            println!("{} {}", usage.kind, usage.name);
            if let Some(ref t) = usage.type_ref {
                println!("  type: {}", t);
            }
            if let Some(ref dir) = usage.direction {
                println!("  direction: {}", dir.label());
            }
            if let Some(ref mult) = usage.multiplicity {
                println!("  multiplicity: {}", mult);
            }
            if let Some(ref val) = usage.value_expr {
                println!("  default: {}", val);
            }
            if let Some(ref redef) = usage.redefinition {
                println!("  redefines: {}", redef);
            }
            if let Some(ref sub) = usage.subsets {
                println!("  subsets: {}", sub);
            }
            if let Some(ref parent) = usage.parent_def {
                println!("  parent: {}", parent);
            }
            println!(
                "  location: {}:{}:{}",
                path_str, usage.span.start_row, usage.span.start_col
            );
        }
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "error: element `{}` not found in `{}`",
        element, path_str
    );
    ExitCode::from(1)
}
