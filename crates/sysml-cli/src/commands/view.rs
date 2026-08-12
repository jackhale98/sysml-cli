//! View command: render model-defined views (`view def` +
//! `@TableRendering`) as tables. The rendering semantics live entirely in
//! `sysml_core::view_render`; this file is terminal formatting only.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::view_render::{available_views, render_view, RenderedTable};

use crate::Cli;

pub fn run(cli: &Cli, name: Option<&str>, files: &[PathBuf]) -> ExitCode {
    // `sysml view model.sysml` — a first positional that is an existing
    // file is a file, not a view name (list mode).
    let mut files = files.to_vec();
    let name = match name {
        Some(n) if std::path::Path::new(n).exists() => {
            files.insert(0, PathBuf::from(n));
            None
        }
        other => other,
    };
    let files = &files[..];

    let Some(models) = crate::load_per_file_models(cli, files) else {
        return ExitCode::FAILURE;
    };

    let Some(name) = name else {
        // List available views.
        let views = available_views(&models);
        if views.is_empty() {
            eprintln!("no view defs found (are the libraries on the include path?)");
            return ExitCode::FAILURE;
        }
        if cli.format == "json" {
            let items: Vec<_> = views
                .iter()
                .map(|(n, f, spec)| {
                    serde_json::json!({ "name": n, "file": f, "renderable": spec })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
        } else {
            for (n, f, spec) in &views {
                println!(
                    "{:<28} {} {}",
                    n,
                    if *spec { " " } else { "(no @TableRendering)" },
                    f
                );
            }
        }
        return ExitCode::SUCCESS;
    };

    let table = match render_view(&models, name) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    for w in &table.warnings {
        eprintln!("warning: {w}");
    }

    match cli.format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&table).unwrap()),
        "csv" => print_csv(&table),
        "md" => print_markdown(&table),
        _ => print_text(&table),
    }
    ExitCode::SUCCESS
}

fn print_text(t: &RenderedTable) {
    // Column widths from content.
    let mut widths: Vec<usize> = t.columns.iter().map(|c| c.chars().count()).collect();
    for row in &t.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }
    let line = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{:<w$}", c, w = widths.get(i).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    println!("{}", line(&t.columns));
    println!("{}", "-".repeat(widths.iter().sum::<usize>() + 2 * (widths.len().saturating_sub(1))));
    for row in &t.rows {
        println!("{}", line(row));
    }
    eprintln!("{} row(s).", t.rows.len());
}

fn print_csv(t: &RenderedTable) {
    let esc = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    println!(
        "{}",
        t.columns.iter().map(|c| esc(c)).collect::<Vec<_>>().join(",")
    );
    for row in &t.rows {
        println!(
            "{}",
            row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(",")
        );
    }
}

fn print_markdown(t: &RenderedTable) {
    let esc = |s: &str| s.replace('|', "\\|");
    println!(
        "| {} |",
        t.columns.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")
    );
    println!("|{}|", vec!["---"; t.columns.len()].join("|"));
    for row in &t.rows {
        println!(
            "| {} |",
            row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")
        );
    }
}
