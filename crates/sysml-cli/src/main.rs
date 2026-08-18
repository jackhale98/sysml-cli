//! sysml-cli: SysML v2 command-line tool for validation, simulation,
//! diagram generation, and model analysis.

use std::process::ExitCode;

use clap::Parser;

mod cli;
mod commands;
mod helpers;
mod model_writer;
mod output;
mod wizard;

// Re-export for use by command modules.
pub(crate) use cli::*;
pub(crate) use helpers::*;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Command::List {
            files,
            kind,
            name,
            doc,
            parent,
            unused,
            abstract_only,
            variations,
            variants,
            metadata,
            where_clauses,
            visibility,
            view,
            type_name,
        } => commands::list::run(
            &cli,
            files,
            kind.as_deref(),
            name.as_deref(),
            doc.as_deref(),
            parent.as_deref(),
            *unused,
            *abstract_only,
            *variations,
            *variants,
            metadata.as_deref(),
            where_clauses,
            visibility.as_deref(),
            view.as_deref(),
            type_name.as_deref(),
        ),
        Command::Show { file, element, raw } => commands::show::run(&cli, file, element, *raw),
        Command::Trace {
            files,
            check,
            gate,
        } => commands::trace::run(&cli, files, *check, gate.as_deref()),
        Command::Diagram {
            file,
            diagram_type,
            output_format,
            scope,
            direction,
            depth,
        } => commands::diagram::run(
            &cli,
            file,
            diagram_type,
            output_format,
            scope.as_deref(),
            direction.as_deref(),
            *depth,
        ),
        Command::Simulate { kind } => commands::simulate::run(&cli, kind),
        Command::Export { kind } => commands::export::run(&cli, kind),
        Command::Add {
            file,
            kind,
            name,
            type_ref,
            inside,
            dry_run,
            stdout,
            teach,
            doc,
            extends,
            r#abstract,
            short_name,
            members,
            connect,
            satisfy,
            verify,
            by,
            exposes,
            filter,
        } => commands::add::run(
            &cli,
            file.as_ref(),
            kind.as_deref(),
            name.as_deref(),
            type_ref.as_deref(),
            inside.as_deref(),
            *dry_run,
            *stdout,
            *teach,
            doc.as_deref(),
            extends.as_deref(),
            *r#abstract,
            short_name.as_deref(),
            members,
            exposes,
            filter.as_deref(),
            connect.as_deref(),
            satisfy.as_deref(),
            verify.as_deref(),
            by.as_deref(),
        ),
        Command::Remove {
            file,
            name,
            dry_run,
        } => commands::remove::run(&cli, file, name, *dry_run),
        Command::Rename {
            file,
            old_name,
            new_name,
            dry_run,
            project,
        } => commands::rename::run(&cli, file, old_name, new_name, *dry_run, *project),
        Command::Fmt {
            files,
            check,
            diff,
            indent_width,
        } => commands::fmt::run(&cli, files, *check, *diff, *indent_width),
        Command::Completions { shell } => {
            generate_completions(shell);
            ExitCode::SUCCESS
        }
        Command::Deps {
            files,
            target,
            reverse,
            forward,
            transitive,
        } => commands::deps::run(&cli, files, target, *reverse, *forward, *transitive),
        Command::Diff { file_a, file_b } => commands::diff::run(&cli, file_a, file_b),
        Command::Allocation {
            files,
            check,
            unallocated,
        } => commands::allocation::run(&cli, files, *check, *unallocated),
        Command::Coverage {
            files,
            check,
            gate,
        } => commands::coverage::run(&cli, files, *check, gate.as_deref()),
        Command::Init { force } => commands::init::run(&cli, *force),
        Command::Check {
            files,
            disable,
            severity,
        } => commands::check::run(&cli, files, disable, severity),
        Command::Repl { files } => commands::repl::run(&cli, files),
        Command::Doc { files, root } => commands::doc::run(&cli, files, root.as_deref()),
        Command::View { name, files, renderer } => {
            commands::view::run(&cli, name.as_deref(), files, renderer)
        }
        Command::Analyze { kind } => commands::analyze::run(&cli, kind),
        Command::Rollup { kind } => commands::rollup::run(&cli, kind),
    }
}
