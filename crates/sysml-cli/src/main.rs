/// sysml-cli: SysML v2 command-line tool for validation, simulation,
/// diagram generation, and model management.

use std::process::ExitCode;

use clap::Parser;

mod cli;
mod commands;
mod helpers;
mod model_writer;
mod output;
mod records;
mod wizard;

// Re-export for use by command modules.
pub(crate) use cli::*;
pub(crate) use helpers::*;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Command::Lint { files, disable, severity } => {
            commands::lint::run(&cli, files, disable, severity)
        }
        Command::List {
            files, kind, name, parent, unused, abstract_only, visibility, view,
        } => commands::list::run(
            &cli, files, kind.as_deref(), name.as_deref(), parent.as_deref(),
            *unused, *abstract_only, visibility.as_deref(), view.as_deref(),
        ),
        Command::Show { file, element, raw } => commands::show::run(&cli, file, element, *raw),
        Command::Trace { files, check, min_coverage } => {
            commands::trace::run(&cli, files, *check, *min_coverage)
        }
        Command::Interfaces { files, unconnected } => {
            commands::interfaces::run(&cli, files, *unconnected)
        }
        Command::Diagram { file, diagram_type, output_format, scope, view, direction, depth } => {
            commands::diagram::run(&cli, file, diagram_type, output_format,
                scope.as_deref(), view.as_deref(), direction.as_deref(), *depth)
        }
        Command::Simulate { kind } => commands::simulate::run(&cli, kind),
        Command::Export { kind } => commands::export::run(&cli, kind),
        Command::Add {
            file, kind, name, type_ref, inside, dry_run, stdout,
            teach, doc, extends, r#abstract, short_name, members,
            exposes, filter, interactive,
        } => commands::add::run(
            file.as_ref(), kind.as_deref(), name.as_deref(),
            type_ref.as_deref(), inside.as_deref(), *dry_run, *stdout,
            *teach, doc.as_deref(), extends.as_deref(), *r#abstract,
            short_name.as_deref(), members, exposes,
            filter.as_deref(), *interactive,
        ),
        Command::Remove { file, name, dry_run } => {
            commands::remove::run(file, name, *dry_run)
        }
        Command::Rename { file, old_name, new_name, dry_run } => {
            commands::rename::run(file, old_name, new_name, *dry_run)
        }
        Command::Fmt { files, check, diff, indent_width } => {
            commands::fmt::run(files, *check, *diff, *indent_width)
        }
        Command::Completions { shell } => {
            generate_completions(shell);
            ExitCode::SUCCESS
        }
        Command::Stats { files } => commands::stats::run(&cli, files),
        Command::Deps { files, target, reverse, forward } => {
            commands::deps::run(&cli, files, target, *reverse, *forward)
        }
        Command::Diff { file_a, file_b } => commands::diff::run(&cli, file_a, file_b),
        Command::Allocation { files, check, unallocated } => {
            commands::allocation::run(&cli, files, *check, *unallocated)
        }
        Command::Coverage { files, check, min_score } => {
            commands::coverage::run(&cli, files, *check, *min_score)
        }
        Command::Init { force } => commands::init::run(&cli, *force),
        Command::Index { full, stats } => commands::index::run(&cli, *full, *stats),
        Command::Check { files, disable, severity, lint_only } => {
            commands::check::run(&cli, files, disable, severity, *lint_only)
        }
        Command::Verify { kind } => commands::verify::run(&cli, kind),
        Command::Example { name, output, list } => {
            commands::scaffold::run_example_command(name.as_deref(), output.as_ref(), *list)
        }
        Command::Risk { kind } => commands::risk::run(&cli, kind),
        Command::Tol { kind } => commands::tol::run(&cli, kind),
        Command::Bom { kind } => commands::bom::run(&cli, kind),
        Command::Source { kind } => commands::source::run(&cli, kind),
        Command::Mfg { kind } => commands::mfg::run(&cli, kind),
        Command::Qc { kind } => commands::qc::run(&cli, kind),
        Command::Quality { kind } => commands::quality::run(&cli, kind),
        Command::Report { kind } => commands::report::run(&cli, kind),
        Command::Guide { topic } => commands::help_topics::run(topic.as_deref()),
        Command::Pipeline { ref kind } => commands::pipeline::run(kind, &cli.format, cli.quiet),
    }
}
