/// Interactive REPL for exploring SysML v2 models.
///
/// Provides stateful navigation, relationship queries, and filtering
/// that go beyond what batch CLI commands offer.

use std::path::PathBuf;
use std::process::ExitCode;

use rustyline::DefaultEditor;
use sysml_core::model::{simple_name, Model};
use sysml_core::parser as sysml_parser;

pub fn run(files: &[PathBuf]) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }

    let mut model = load_model(&files);
    println!("sysml repl v{} — {} definitions, {} usages loaded from {} file(s)",
        env!("CARGO_PKG_VERSION"), model.definitions.len(), model.usages.len(), files.len());
    println!("Type 'help' for commands, 'quit' to exit.\n");

    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("error: cannot initialize line editor: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut context = ReplContext { focus: None };

    loop {
        let prompt = match &context.focus {
            Some(name) => format!("sysml [{}]> ", name),
            None => "sysml> ".to_string(),
        };
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                if line == "reload" {
                    model = load_model(&files);
                    println!("Reloaded: {} definitions, {} usages from {} file(s)",
                        model.definitions.len(), model.usages.len(), files.len());
                    continue;
                }
                if !dispatch(&model, line, &mut context) {
                    break;
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("error: {}", e);
                break;
            }
        }
    }

    ExitCode::SUCCESS
}

struct ReplContext {
    /// Currently focused element (for contextual queries).
    focus: Option<String>,
}

fn load_model(files: &[PathBuf]) -> Model {
    let mut merged = Model::new("repl".to_string());
    for file_path in files {
        let path_str = file_path.to_string_lossy().to_string();
        if let Ok(source) = std::fs::read_to_string(file_path) {
            let m = sysml_parser::parse_file(&path_str, &source);
            merged.definitions.extend(m.definitions);
            merged.usages.extend(m.usages);
            merged.connections.extend(m.connections);
            merged.flows.extend(m.flows);
            merged.satisfactions.extend(m.satisfactions);
            merged.verifications.extend(m.verifications);
            merged.allocations.extend(m.allocations);
            merged.type_references.extend(m.type_references);
            merged.imports.extend(m.imports);
            merged.comments.extend(m.comments);
            merged.views.extend(m.views);
        }
    }
    merged
}

fn dispatch(model: &Model, input: &str, ctx: &mut ReplContext) -> bool {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = parts.get(1).unwrap_or(&"").trim();

    match cmd {
        "quit" | "exit" | "q" => return false,

        "help" | "h" | "?" => print_help(),

        // --- Navigation ---
        "cd" | "focus" => {
            if args.is_empty() || args == ".." || args == "/" {
                ctx.focus = None;
                println!("Focus cleared.");
            } else if model.find_def(args).is_some()
                || model.usages.iter().any(|u| u.name == args)
            {
                ctx.focus = Some(args.to_string());
                cmd_show(model, args);
            } else {
                println!("Element `{}` not found.", args);
            }
        }

        // --- Listing with filters ---
        "list" | "ls" => cmd_list(model, args, &ctx.focus),

        "defs" => {
            let pat = if args.is_empty() { None } else { Some(args.to_lowercase()) };
            for def in &model.definitions {
                if let Some(ref p) = pat {
                    if !def.kind.label().contains(p.as_str()) && !def.name.to_lowercase().contains(p.as_str()) {
                        continue;
                    }
                }
                if let Some(ref focus) = ctx.focus {
                    if def.parent_def.as_deref() != Some(focus.as_str()) && def.name != *focus {
                        continue;
                    }
                }
                println!("  {:14} {}{}", def.kind.label(), def.name,
                    def.super_type.as_ref().map(|s| format!(" :> {}", s)).unwrap_or_default());
            }
        }

        "usages" => cmd_usages(model, args, &ctx.focus),

        // --- Relationship queries ---
        "typeof" | "instances" => {
            // Show all usages of a given type
            let type_name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if type_name.is_empty() {
                println!("Usage: typeof <TypeName> — show all usages of this type");
                return true;
            }
            let mut count = 0;
            for usage in &model.usages {
                if let Some(ref tr) = usage.type_ref {
                    if simple_name(tr) == type_name {
                        println!("  {:14} {} : {} (in {})",
                            usage.kind, usage.name, tr,
                            usage.parent_def.as_deref().unwrap_or("?"));
                        count += 1;
                    }
                }
            }
            if count == 0 {
                println!("No usages of type `{}`.", type_name);
            } else {
                println!("({} usages of {})", count, type_name);
            }
        }

        "subtypes" | "specializations" => {
            let name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if name.is_empty() {
                println!("Usage: subtypes <Name>");
                return true;
            }
            let mut count = 0;
            for def in &model.definitions {
                if let Some(ref st) = def.super_type {
                    if simple_name(st) == name {
                        println!("  {} :> {}", def.name, st);
                        count += 1;
                    }
                }
            }
            if count == 0 {
                println!("No subtypes of `{}`.", name);
            }
        }

        "supertypes" | "hierarchy" => {
            let name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if name.is_empty() {
                println!("Usage: supertypes <Name>");
                return true;
            }
            let mut current = name.to_string();
            let mut depth = 0;
            loop {
                if let Some(def) = model.find_def(&current) {
                    let indent = "  ".repeat(depth);
                    println!("{}{} ({})", indent, def.name, def.kind.label());
                    if let Some(ref st) = def.super_type {
                        current = simple_name(st).to_string();
                        depth += 1;
                    } else {
                        break;
                    }
                } else {
                    if depth > 0 {
                        println!("{}  {} (external)", "  ".repeat(depth), current);
                    }
                    break;
                }
            }
        }

        "connections" | "connected" => {
            let name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if name.is_empty() {
                // Show all connections
                for conn in &model.connections {
                    println!("  {} → {}{}", conn.source, conn.target,
                        conn.name.as_ref().map(|n| format!(" ({})", n)).unwrap_or_default());
                }
                println!("({} connections)", model.connections.len());
            } else {
                // Show connections involving this element
                let mut count = 0;
                for conn in &model.connections {
                    if simple_name(&conn.source) == name || simple_name(&conn.target) == name
                        || conn.source.contains(name) || conn.target.contains(name)
                    {
                        println!("  {} → {}", conn.source, conn.target);
                        count += 1;
                    }
                }
                if count == 0 {
                    println!("No connections involving `{}`.", name);
                }
            }
        }

        "flows" => {
            let name = if args.is_empty() { ctx.focus.as_deref().unwrap_or("") } else { args };
            let mut count = 0;
            for flow in &model.flows {
                if name.is_empty()
                    || simple_name(&flow.source) == name
                    || simple_name(&flow.target) == name
                    || flow.source.contains(name)
                    || flow.target.contains(name)
                {
                    println!("  {} → {}{}",
                        flow.source, flow.target,
                        flow.item_type.as_ref().map(|t| format!(" ({})", t)).unwrap_or_default());
                    count += 1;
                }
            }
            if count == 0 {
                println!("No flows{}.", if name.is_empty() { "".to_string() } else { format!(" involving `{}`", name) });
            }
        }

        "trace" => cmd_trace(model, args),

        "satisfy" | "satisfactions" => {
            let name = if args.is_empty() { ctx.focus.as_deref().unwrap_or("") } else { args };
            for sat in &model.satisfactions {
                if name.is_empty()
                    || sat.requirement == name
                    || sat.by.as_deref() == Some(name)
                {
                    println!("  satisfy {} by {}",
                        sat.requirement,
                        sat.by.as_deref().unwrap_or("?"));
                }
            }
        }

        "verify" | "verifications" => {
            let name = if args.is_empty() { ctx.focus.as_deref().unwrap_or("") } else { args };
            for ver in &model.verifications {
                if name.is_empty()
                    || ver.requirement == name
                    || ver.by == name
                {
                    println!("  verify {} by {}", ver.requirement, ver.by);
                }
            }
        }

        // --- Existing commands ---
        "show" => {
            let name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if name.is_empty() {
                println!("Usage: show <name> (or focus on an element first with 'cd')");
            } else {
                cmd_show(model, name);
            }
        }

        "find" => {
            if args.is_empty() {
                println!("Usage: find <pattern>");
                return true;
            }
            let pat = args.to_lowercase();
            let mut count = 0;
            for def in &model.definitions {
                if def.name.to_lowercase().contains(&pat)
                    || def.doc.as_ref().map_or(false, |d| d.to_lowercase().contains(&pat))
                {
                    println!("  {:14} {}", def.kind.label(), def.name);
                    count += 1;
                }
            }
            for usage in &model.usages {
                if usage.name.to_lowercase().contains(&pat)
                    || usage.type_ref.as_ref().map_or(false, |t| t.to_lowercase().contains(&pat))
                {
                    println!("  {:14} {}{} (in {})", usage.kind, usage.name,
                        usage.type_ref.as_ref().map(|t| format!(" : {}", t)).unwrap_or_default(),
                        usage.parent_def.as_deref().unwrap_or("?"));
                    count += 1;
                }
            }
            println!("({} matches)", count);
        }

        "deps" => {
            let name = if args.is_empty() {
                ctx.focus.as_deref().unwrap_or("")
            } else {
                args
            };
            if name.is_empty() {
                println!("Usage: deps <name>");
                return true;
            }
            let deps = sysml_core::query::dependency_analysis(model, name);
            if !deps.referenced_by.is_empty() {
                println!("  Referenced by ({}):", deps.referenced_by.len());
                for r in &deps.referenced_by {
                    println!("    {} ({}) via {}", r.name, r.kind, r.relationship);
                }
            }
            if !deps.depends_on.is_empty() {
                println!("  Depends on ({}):", deps.depends_on.len());
                for d in &deps.depends_on {
                    println!("    {} ({}) via {}", d.name, d.kind, d.relationship);
                }
            }
            if deps.referenced_by.is_empty() && deps.depends_on.is_empty() {
                println!("  No dependencies found for `{}`.", name);
            }
        }

        "rollup" => {
            let rparts: Vec<&str> = args.splitn(2, ' ').collect();
            let (root, attr) = if rparts.len() >= 2 {
                (rparts[0], rparts[1])
            } else if let Some(ref focus) = ctx.focus {
                if args.is_empty() {
                    println!("Usage: rollup [root] <attribute>");
                    return true;
                }
                (focus.as_str(), args)
            } else {
                println!("Usage: rollup <root_def> <attribute>");
                return true;
            };
            use sysml_core::sim::rollup::{evaluate_rollup, AggregationMethod, format_rollup_text};
            if model.find_def(root).is_none() {
                println!("Definition `{}` not found.", root);
                return true;
            }
            let result = evaluate_rollup(model, root, attr, AggregationMethod::Sum);
            print!("{}", format_rollup_text(&result));
        }

        "stats" => {
            println!("  Definitions:    {}", model.definitions.len());
            println!("  Usages:         {}", model.usages.len());
            println!("  Connections:    {}", model.connections.len());
            println!("  Flows:          {}", model.flows.len());
            println!("  Satisfactions:  {}", model.satisfactions.len());
            println!("  Verifications:  {}", model.verifications.len());
            println!("  Allocations:    {}", model.allocations.len());
            println!("  Imports:        {}", model.imports.len());
        }

        _ => {
            println!("Unknown command: `{}`. Type 'help' for commands.", cmd);
        }
    }

    true
}

fn print_help() {
    println!("Navigation:");
    println!("  cd <name>            Focus on an element (prompt shows context)");
    println!("  cd ..                Clear focus");
    println!("  show [name]          Show element details (uses focus if no name)");
    println!();
    println!("Listing:");
    println!("  list [kind]          List elements, optionally filtered by kind");
    println!("  defs [pattern]       List definitions (filtered by kind or name)");
    println!("  usages [type|in:parent|kind:X]  Filter usages");
    println!("  find <pattern>       Search names and doc comments");
    println!();
    println!("Relationships:");
    println!("  typeof <Type>        Show all usages of a type");
    println!("  subtypes <Name>      Show all specializations of a definition");
    println!("  supertypes <Name>    Walk the inheritance chain upward");
    println!("  connections [name]   Show connections (optionally filtered)");
    println!("  flows [name]         Show flows (optionally filtered)");
    println!("  satisfy [name]       Show satisfy relationships");
    println!("  verify [name]        Show verify relationships");
    println!("  deps [name]          Forward and reverse dependencies");
    println!("  trace [req]          Requirements traceability");
    println!();
    println!("Analysis:");
    println!("  rollup <root> <attr> Compute attribute rollup (sum)");
    println!("  stats                Model statistics");
    println!();
    println!("  reload               Reload model files from disk");
    println!("  help                 Show this help");
    println!("  quit                 Exit");
}

fn cmd_show(model: &Model, name: &str) {
    if let Some(def) = model.find_def(name) {
        println!("  {} `{}`", def.kind.label(), def.name);
        if let Some(ref st) = def.super_type {
            println!("  Specializes: {}", st);
        }
        if let Some(ref doc) = def.doc {
            println!("  Doc: {}", doc);
        }
        if let Some(ref parent) = def.parent_def {
            println!("  In: {}", parent);
        }
        if def.is_abstract {
            println!("  Abstract: yes");
        }
        if let Some(ref vis) = def.visibility {
            println!("  Visibility: {}", vis.label());
        }
        let members = model.usages_in_def(&def.name);
        if !members.is_empty() {
            println!("  Members ({}):", members.len());
            for u in &members {
                println!("    {} {}{}{}",
                    u.kind, u.name,
                    u.type_ref.as_ref().map(|t| format!(" : {}", t)).unwrap_or_default(),
                    u.multiplicity.as_ref().map(|m| format!(" {}", m)).unwrap_or_default());
            }
        }
        // Show relationships
        let sats: Vec<_> = model.satisfactions.iter()
            .filter(|s| s.by.as_deref() == Some(name) || s.requirement == name)
            .collect();
        if !sats.is_empty() {
            println!("  Satisfactions:");
            for s in &sats {
                println!("    satisfy {} by {}", s.requirement, s.by.as_deref().unwrap_or("?"));
            }
        }
        let vers: Vec<_> = model.verifications.iter()
            .filter(|v| v.by == name || v.requirement == name)
            .collect();
        if !vers.is_empty() {
            println!("  Verifications:");
            for v in &vers {
                println!("    verify {} by {}", v.requirement, v.by);
            }
        }
    } else if let Some(usage) = model.usages.iter().find(|u| u.name == name) {
        println!("  {} `{}`", usage.kind, usage.name);
        if let Some(ref tr) = usage.type_ref {
            println!("  Type: {}", tr);
        }
        if let Some(ref parent) = usage.parent_def {
            println!("  In: {}", parent);
        }
        if let Some(ref mult) = usage.multiplicity {
            println!("  Multiplicity: {}", mult);
        }
        if let Some(ref val) = usage.value_expr {
            println!("  Value: {}", val);
        }
        if let Some(ref dir) = usage.direction {
            println!("  Direction: {}", dir.label());
        }
    } else {
        println!("Element `{}` not found.", name);
    }
}

fn cmd_list(model: &Model, args: &str, focus: &Option<String>) {
    let kind_filter = if args.is_empty() { None } else { Some(args.to_lowercase()) };
    let mut count = 0;

    for def in &model.definitions {
        if let Some(ref focus) = focus {
            if def.parent_def.as_deref() != Some(focus.as_str()) && def.name != *focus {
                continue;
            }
        }
        if let Some(ref k) = kind_filter {
            if !def.kind.label().contains(k.as_str()) {
                continue;
            }
        }
        println!("  {:14} {}{}", def.kind.label(), def.name,
            def.super_type.as_ref().map(|s| format!(" :> {}", s)).unwrap_or_default());
        count += 1;
    }
    for usage in &model.usages {
        if let Some(ref focus) = focus {
            if usage.parent_def.as_deref() != Some(focus.as_str()) {
                continue;
            }
        }
        if let Some(ref k) = kind_filter {
            if !usage.kind.contains(k.as_str()) {
                continue;
            }
        }
        println!("  {:14} {}{} (in {})", usage.kind, usage.name,
            usage.type_ref.as_ref().map(|t| format!(" : {}", t)).unwrap_or_default(),
            usage.parent_def.as_deref().unwrap_or("?"));
        count += 1;
    }
    println!("({} elements)", count);
}

fn cmd_usages(model: &Model, args: &str, focus: &Option<String>) {
    // Parse filters: "type:Engine", "in:Vehicle", "kind:part", or just a pattern
    let mut type_filter: Option<&str> = None;
    let mut parent_filter: Option<&str> = focus.as_deref();
    let mut kind_filter: Option<&str> = None;
    let mut name_filter: Option<&str> = None;

    for token in args.split_whitespace() {
        if let Some(t) = token.strip_prefix("type:") {
            type_filter = Some(t);
        } else if let Some(p) = token.strip_prefix("in:") {
            parent_filter = Some(p);
        } else if let Some(k) = token.strip_prefix("kind:") {
            kind_filter = Some(k);
        } else if !token.is_empty() {
            // Could be a type name or name pattern
            if model.find_def(token).is_some() {
                type_filter = Some(token);
            } else {
                name_filter = Some(token);
            }
        }
    }

    let mut count = 0;
    for usage in &model.usages {
        if let Some(t) = type_filter {
            match &usage.type_ref {
                Some(tr) if simple_name(tr) == t => {}
                _ => continue,
            }
        }
        if let Some(p) = parent_filter {
            if usage.parent_def.as_deref() != Some(p) {
                continue;
            }
        }
        if let Some(k) = kind_filter {
            if !usage.kind.contains(k) {
                continue;
            }
        }
        if let Some(n) = name_filter {
            if !usage.name.to_lowercase().contains(&n.to_lowercase()) {
                continue;
            }
        }
        println!("  {:14} {} : {} (in {})",
            usage.kind,
            usage.name,
            usage.type_ref.as_deref().unwrap_or("-"),
            usage.parent_def.as_deref().unwrap_or("?"));
        count += 1;
    }
    println!("({} usages)", count);
}

fn cmd_trace(model: &Model, args: &str) {
    let trace = sysml_core::query::trace_requirements(model);
    if trace.is_empty() {
        println!("No requirements found.");
        return;
    }

    let filter = if args.is_empty() { None } else { Some(args.to_lowercase()) };

    println!("{:25} {:20} {:20}", "Requirement", "Satisfied By", "Verified By");
    println!("{}", "-".repeat(67));
    let mut shown = 0;
    for t in &trace {
        if let Some(ref f) = filter {
            let matches = t.requirement.to_lowercase().contains(f)
                || t.satisfied_by.iter().any(|s| s.to_lowercase().contains(f))
                || t.verified_by.iter().any(|v| v.to_lowercase().contains(f));
            if !matches {
                continue;
            }
        }
        let sat = if t.satisfied_by.is_empty() { "-".to_string() } else { t.satisfied_by.join(", ") };
        let ver = if t.verified_by.is_empty() { "-".to_string() } else { t.verified_by.join(", ") };
        println!("{:25} {:20} {:20}", t.requirement, sat, ver);
        shown += 1;
    }
    if shown == 0 && filter.is_some() {
        println!("No matches for `{}`.", args);
    }
}
