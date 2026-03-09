/// sysml-cli: SysML v2 command-line tool for validation, simulation,
/// diagram generation, and model management.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use sysml_core::checks::{self, Check};
use sysml_core::diagnostic::{Diagnostic, Severity};
use sysml_core::parser as sysml_parser;

mod output;

#[derive(Parser)]
#[command(
    name = "sysml-cli",
    about = "SysML v2 command-line tool for validation, simulation, diagram generation, and model management",
    long_about = "\
sysml-cli works with SysML v2 models in textual notation.

SysML v2 is the next-generation systems modeling language from OMG. It uses \
a textual notation where 'definitions' declare reusable types (part def, port def, \
action def, etc.) and 'usages' create instances of those types within a context.

GETTING STARTED:
  Validate a model:       sysml-cli lint model.sysml
  List model elements:    sysml-cli list --kind parts model.sysml
  Show element details:   sysml-cli show model.sysml Vehicle
  Generate a diagram:     sysml-cli diagram -t bdd -o mermaid model.sysml
  Simulate a state machine: sysml-cli simulate state-machine model.sysml
  Create a new definition: sysml-cli new part-def Vehicle --doc 'A vehicle'
  Edit an existing file:  sysml-cli edit add model.sysml part engine -t Engine
  Format a file:          sysml-cli fmt model.sysml
  Export to FMI:          sysml-cli export interfaces model.sysml --part MyPart

LEARN MORE:
  SysML v2 spec:          https://www.omgsysml.org/
  This tool:              https://github.com/jackhale98/sysml-cli",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format: text, json.
    #[arg(short, long, default_value = "text", global = true)]
    format: String,

    /// Suppress summary line on stderr.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Additional SysML files or directories to include for import resolution.
    /// Definitions from these files are available to imported names.
    #[arg(short = 'I', long = "include", global = true)]
    include: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Lint SysML v2 files for structural issues.
    ///
    /// Validates SysML v2 models against structural rules: syntax errors,
    /// duplicate definitions, unused elements, unsatisfied requirements, and more.
    Lint {
        /// SysML v2 files to validate.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Disable specific checks (comma-separated).
        /// Available: syntax, duplicates, unused, unresolved, unsatisfied, unverified, port-types, constraints, calculations
        #[arg(short, long, value_delimiter = ',')]
        disable: Vec<String>,

        /// Minimum severity to report: note, warning, error.
        #[arg(short, long, default_value = "note")]
        severity: String,
    },
    /// List model elements with optional filters.
    ///
    /// Lists definitions and usages from SysML v2 files. Filter by kind,
    /// name pattern, parent definition, visibility, or structural properties.
    ///
    /// SysML v2 elements are either 'definitions' (reusable types like
    /// part def, port def) or 'usages' (instances like part, port).
    #[command(visible_alias = "ls")]
    List {
        /// SysML v2 files to inspect.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Filter by element kind.
        /// Definitions: parts, ports, actions, states, requirements, constraints, etc.
        /// Usages: use the singular form (part, port, action, etc.)
        /// Special: all, definitions, usages
        #[arg(short, long)]
        kind: Option<String>,

        /// Filter by name (substring match).
        #[arg(short, long)]
        name: Option<String>,

        /// Filter by parent definition.
        #[arg(short, long)]
        parent: Option<String>,

        /// Show only unused definitions.
        #[arg(long)]
        unused: bool,

        /// Show only abstract definitions.
        #[arg(long, name = "abstract")]
        abstract_only: bool,

        /// Filter by visibility (public, private, protected).
        #[arg(long)]
        visibility: Option<String>,

        /// Apply a named SysML v2 view definition as a filter preset.
        /// The view's expose and filter clauses determine which elements are shown.
        #[arg(long)]
        view: Option<String>,
    },
    /// Show detailed information about a specific element.
    ///
    /// Displays all known information about a named definition or usage:
    /// kind, visibility, parent, documentation, type, children, and relationships.
    Show {
        /// SysML v2 file to inspect.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the element to show.
        #[arg(required = true)]
        element: String,
    },
    /// Generate a requirements traceability matrix.
    ///
    /// Lists all requirement definitions and shows their satisfaction
    /// and verification status. In SysML v2, requirements are traced via
    /// 'satisfy' and 'verify' relationships.
    Trace {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Exit with error if any requirement lacks satisfaction or verification.
        /// Useful for CI pipelines.
        #[arg(long)]
        check: bool,

        /// Minimum coverage percentage required (used with --check).
        #[arg(long, default_value = "0")]
        min_coverage: f64,
    },
    /// Analyze port interfaces and connections.
    ///
    /// Lists ports across definitions and identifies unconnected ports.
    /// In SysML v2, ports define the interaction points of parts.
    Interfaces {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Show only unconnected ports (gaps in the interface).
        #[arg(long)]
        unconnected: bool,
    },
    /// Generate a diagram from a SysML v2 model.
    ///
    /// Produces diagrams in Mermaid, PlantUML, DOT, or D2 format.
    ///
    /// DIAGRAM TYPES:
    ///   bdd  — Block Definition Diagram (definitions and relationships)
    ///   ibd  — Internal Block Diagram (internal structure of a part)
    ///   stm  — State Machine Diagram (states and transitions)
    ///   act  — Activity Diagram (action flow with decisions/forks)
    ///   req  — Requirements Diagram (requirements and trace status)
    ///   pkg  — Package Diagram (packages and containment hierarchy)
    ///   par  — Parametric Diagram (constraints and parameters)
    ///
    /// OUTPUT FORMATS:
    ///   mermaid  — Mermaid.js (render in GitHub, Obsidian, etc.)
    ///   plantuml — PlantUML (puml alias)
    ///   dot      — Graphviz DOT
    ///   d2       — Terrastruct D2
    ///
    /// EXAMPLES:
    ///   sysml-cli diagram -t bdd model.sysml
    ///   sysml-cli diagram -t ibd -s Vehicle model.sysml
    ///   sysml-cli diagram -t stm -o plantuml model.sysml
    ///   sysml-cli diagram -t bdd -d LR --depth 2 model.sysml
    Diagram {
        /// SysML v2 file to generate diagram from.
        #[arg(required = true)]
        file: PathBuf,

        /// Diagram type (see DIAGRAM TYPES above).
        #[arg(short = 't', long = "type", required = true)]
        diagram_type: String,

        /// Output format (see OUTPUT FORMATS above).
        #[arg(short = 'o', long = "output-format", default_value = "mermaid")]
        output_format: String,

        /// Focus diagram on a specific definition.
        /// bdd: show only this def and its children/relationships.
        /// ibd: show internal structure (ports, parts, connections).
        /// stm/act: show this specific state machine or action.
        #[arg(short, long)]
        scope: Option<String>,

        /// Layout direction: TB (top-bottom), LR (left-right), BT, RL.
        #[arg(short, long)]
        direction: Option<String>,

        /// Maximum nesting depth to display.
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Run simulations on SysML v2 models.
    ///
    /// Evaluate constraints, simulate state machines with event sequences,
    /// or execute action flows step-by-step. Use `simulate list` to discover
    /// what can be simulated in a file.
    ///
    /// SUBCOMMANDS: eval, state-machine (sm), action-flow (af), list
    Simulate {
        #[command(subcommand)]
        kind: SimulateCommand,
    },
    /// Export FMI/SSP artifacts from SysML models.
    ///
    /// Generate co-simulation interfaces (FMI 3.0), Modelica stubs, or
    /// SSP system structure descriptions from SysML v2 part definitions.
    ///
    /// SUBCOMMANDS: interfaces, modelica, ssp, list
    Export {
        #[command(subcommand)]
        kind: ExportCommand,
    },
    /// Generate a new SysML v2 definition.
    ///
    /// Creates boilerplate text for a definition (part def, port def, etc.)
    /// and writes it to stdout or a file.
    ///
    /// KINDS:
    ///   part-def, port-def, action-def, state-def, constraint-def, calc-def,
    ///   requirement (req), enum-def, attribute-def (attr), item-def, view-def,
    ///   viewpoint-def, package (pkg), use-case, connection-def, interface-def,
    ///   flow-def, allocation-def
    ///
    /// EXAMPLES:
    ///   sysml-cli new part-def Vehicle
    ///   sysml-cli new part-def Vehicle --extends Base --doc "A vehicle"
    ///   sysml-cli new part-def Vehicle -m "part engine:Engine" -m "part wheels:Wheel"
    ///   sysml-cli new port-def FuelPort -m "in item fuel:FuelType"
    ///   sysml-cli new view-def PartsView --expose "Vehicle::*" --filter part
    ///   sysml-cli new part-def Engine --output model.sysml --append
    ///   sysml-cli new part-def Engine --output model.sysml --inside Vehicle
    New {
        /// Element kind (see KINDS above).
        #[arg(required = true)]
        kind: String,

        /// Element name.
        #[arg(required = true)]
        name: String,

        /// Output file (default: stdout). With --append or --inside, modifies in place.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Specialization: the definition extends (specializes) this supertype.
        /// Generates `:>` syntax, e.g., `part def Car :> Vehicle`.
        #[arg(long)]
        extends: Option<String>,

        /// Mark as abstract (cannot be instantiated directly).
        #[arg(long)]
        r#abstract: bool,

        /// Short name alias. In SysML v2, appears as `<alias>` before the name.
        /// Example: --short-name V produces `part def <V> Vehicle`.
        #[arg(long)]
        short_name: Option<String>,

        /// Documentation comment text. Generates `doc /* text */` inside the body.
        #[arg(long)]
        doc: Option<String>,

        /// Add a member (repeatable). Format: "[direction] kind name[:type]".
        /// Examples: "part engine:Engine", "in port fuel:FuelPort", "attribute mass".
        #[arg(long = "member", short = 'm')]
        members: Vec<String>,

        /// (view-def only) Expose clause: qualified name pattern to include.
        /// Examples: "Vehicle::*" (direct children), "Package::**" (recursive).
        #[arg(long = "expose")]
        exposes: Vec<String>,

        /// (view-def only) Filter by element kind. Only elements of this type are shown.
        /// Examples: "part", "port", "requirement".
        #[arg(long)]
        filter: Option<String>,

        /// Append to an existing file instead of overwriting.
        #[arg(long)]
        append: bool,

        /// Insert inside an existing definition body (requires --output).
        /// Example: --inside Vehicle adds the new element before Vehicle's closing `}`.
        #[arg(long)]
        inside: Option<String>,

        /// Preview: show what would be generated without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Edit SysML v2 files: add, remove, or rename elements.
    ///
    /// Surgically modifies SysML v2 files using byte-accurate CST positions.
    /// All subcommands support --dry-run to preview changes as a unified diff.
    ///
    /// EXAMPLES:
    ///   sysml-cli edit add model.sysml part engine -t Engine
    ///   sysml-cli edit add model.sysml part-def Wheel
    ///   sysml-cli edit remove model.sysml Engine
    ///   sysml-cli edit rename model.sysml Engine Motor --dry-run
    Edit {
        #[command(subcommand)]
        kind: EditCommand,
    },
    /// Format SysML v2 files.
    ///
    /// Normalizes indentation and whitespace. Use --check in CI to verify
    /// files are formatted.
    Fmt {
        /// SysML v2 files to format.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Check formatting without modifying (exit 1 if unformatted).
        #[arg(long)]
        check: bool,

        /// Print diff instead of writing files.
        #[arg(long)]
        diff: bool,

        /// Indentation width (default: 4).
        #[arg(long, default_value = "4")]
        indent_width: usize,
    },
}

#[derive(Subcommand)]
enum EditCommand {
    /// Add an element (definition or usage) to a SysML file.
    ///
    /// For usage-level kinds (part, port, attribute, etc.), the element is
    /// inserted inside an existing definition. If --inside is not given and
    /// the file has definitions with bodies, you'll be prompted to choose one.
    ///
    /// For definition-level kinds (part-def, port-def, etc.), the element is
    /// added at the top level of the file.
    ///
    /// EXAMPLES:
    ///   sysml-cli edit add model.sysml part engine -t Engine
    ///   sysml-cli edit add model.sysml port fuelIn -t FuelPort --inside Vehicle
    ///   sysml-cli edit add model.sysml part-def Wheel --dry-run
    Add {
        /// Target SysML file.
        #[arg(required = true)]
        file: PathBuf,

        /// Element kind. For usages: part, port, attribute, item, ref, action, state.
        /// For definitions: part-def, port-def, action-def, etc.
        #[arg(required = true)]
        kind: String,

        /// Element name.
        #[arg(required = true)]
        name: String,

        /// Type reference (generates `: Type` for usages, `:> Type` for defs).
        #[arg(short = 't', long)]
        type_ref: Option<String>,

        /// Insert inside this definition (auto-detected if omitted for usages).
        #[arg(long)]
        inside: Option<String>,

        /// Preview changes as a unified diff without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove an element (definition or usage) from a file by name.
    ///
    /// Removes the entire line(s) of the element. Searches definitions first,
    /// then usages.
    ///
    /// EXAMPLE: sysml-cli edit remove model.sysml Engine --dry-run
    Remove {
        /// Target SysML file.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the element to remove.
        #[arg(required = true)]
        name: String,

        /// Show changes without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Rename an element and update all references to it.
    ///
    /// Finds all whole-word occurrences of the old name and replaces them.
    /// This includes the definition itself and all type references to it.
    ///
    /// EXAMPLE: sysml-cli edit rename model.sysml Engine Motor --dry-run
    Rename {
        /// Target SysML file.
        #[arg(required = true)]
        file: PathBuf,

        /// Current name of the element.
        #[arg(required = true)]
        old_name: String,

        /// New name for the element.
        #[arg(required = true)]
        new_name: String,

        /// Preview changes as a unified diff without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum SimulateCommand {
    /// Evaluate constraints and calculations with variable bindings.
    ///
    /// Evaluates SysML v2 constraint expressions (returns satisfied/violated)
    /// and calculation expressions (returns computed values).
    ///
    /// EXAMPLES:
    ///   sysml-cli simulate eval model.sysml -b speed=100,mass=1500
    ///   sysml-cli simulate eval model.sysml -n SpeedLimit -b speed=120
    Eval {
        /// SysML v2 file containing constraints/calculations.
        #[arg(required = true)]
        file: PathBuf,

        /// Variable bindings: name=value (comma-separated or repeatable).
        /// Example: -b speed=100,mass=1500
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,

        /// Evaluate only this named constraint or calculation.
        /// Without this flag, all constraints and calculations are evaluated.
        #[arg(short = 'n', long)]
        name: Option<String>,
    },
    /// Simulate a state machine step-by-step.
    ///
    /// Traces state transitions given a sequence of events. If --events is
    /// omitted and the state machine has signal triggers, you will be prompted
    /// to select events interactively.
    ///
    /// EXAMPLES:
    ///   sysml-cli simulate state-machine lights.sysml -e next,next,next
    ///   sysml-cli simulate state-machine model.sysml -n TrafficLight
    ///   sysml-cli simulate state-machine model.sysml  (interactive)
    #[command(visible_alias = "sm")]
    StateMachine {
        /// SysML v2 file containing state machine definitions.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the state machine to simulate (prompted if omitted).
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// Events to inject in order (comma-separated).
        /// These match signal triggers on transitions (e.g., `accept switchOn`).
        #[arg(short = 'e', long, value_delimiter = ',')]
        events: Vec<String>,

        /// Maximum simulation steps before stopping.
        #[arg(short = 'm', long, default_value = "100")]
        max_steps: usize,

        /// Variable bindings for guard expressions: name=value.
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
    },
    /// Execute an action flow step-by-step.
    ///
    /// Walks through the action's perform steps, decisions, forks,
    /// and loops, producing an execution trace.
    ///
    /// EXAMPLES:
    ///   sysml-cli simulate action-flow model.sysml -n ProvidePower
    ///   sysml-cli simulate action-flow model.sysml -b fuelLevel=80
    #[command(visible_alias = "af")]
    ActionFlow {
        /// SysML v2 file containing action definitions.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the action to execute (prompted if omitted).
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// Maximum execution steps before stopping.
        #[arg(short = 'm', long, default_value = "1000")]
        max_steps: usize,

        /// Variable bindings: name=value.
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
    },
    /// List all simulatable constructs in a file.
    ///
    /// Shows state machines, action definitions, constraints, and calculations
    /// found in the file. Use --format json for machine-readable output.
    List {
        /// SysML v2 file to inspect.
        #[arg(required = true)]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ExportCommand {
    /// Extract FMI interface items from a part definition.
    Interfaces {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Part definition name.
        #[arg(short, long)]
        part: String,
    },
    /// Generate Modelica partial model stub.
    Modelica {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Part definition name.
        #[arg(short, long)]
        part: String,
        /// Output file path (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate SSP SystemStructureDescription XML.
    Ssp {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
        /// Output file path (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List exportable parts and their interfaces.
    List {
        /// SysML v2 file.
        #[arg(required = true)]
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Command::Lint {
            files,
            disable,
            severity,
        } => run_lint(&cli, files, disable, severity),
        Command::List {
            files,
            kind,
            name,
            parent,
            unused,
            abstract_only,
            visibility,
            view,
        } => run_list(
            &cli,
            files,
            kind.as_deref(),
            name.as_deref(),
            parent.as_deref(),
            *unused,
            *abstract_only,
            visibility.as_deref(),
            view.as_deref(),
        ),
        Command::Show { file, element } => run_show(&cli, file, element),
        Command::Trace {
            files,
            check,
            min_coverage,
        } => run_trace(&cli, files, *check, *min_coverage),
        Command::Interfaces {
            files,
            unconnected,
        } => run_interfaces(&cli, files, *unconnected),
        Command::Diagram {
            file,
            diagram_type,
            output_format,
            scope,
            direction,
            depth,
        } => run_diagram(&cli, file, diagram_type, output_format, scope.as_deref(), direction.as_deref(), *depth),
        Command::Simulate { kind } => run_simulate(&cli, kind),
        Command::Export { kind } => run_export(&cli, kind),
        Command::New {
            kind,
            name,
            output,
            extends,
            r#abstract,
            short_name,
            doc,
            members,
            exposes,
            filter,
            append,
            inside,
            dry_run,
        } => run_new(
            kind, name, output.as_ref(), extends.as_deref(), *r#abstract,
            short_name.as_deref(), doc.as_deref(), members, exposes,
            filter.as_deref(), *append, inside.as_deref(), *dry_run,
        ),
        Command::Edit { kind } => run_edit(kind),
        Command::Fmt {
            files,
            check,
            diff,
            indent_width,
        } => run_fmt(files, *check, *diff, *indent_width),
    }
}

fn run_lint(cli: &Cli, files: &[PathBuf], disable: &[String], severity: &str) -> ExitCode {
    let disabled: HashSet<&str> = disable.iter().map(|s| s.as_str()).collect();
    let min_severity = match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => Severity::Note,
    };

    let active_checks: Vec<Box<dyn Check>> = checks::all_checks()
        .into_iter()
        .filter(|c| !disabled.contains(c.name()))
        .collect();

    // Build project resolver if includes are specified
    let project = if !cli.include.is_empty() {
        let mut all_files: Vec<PathBuf> = files.to_vec();
        for inc in &cli.include {
            if inc.is_dir() {
                collect_files_recursive(inc, &mut all_files);
            } else {
                all_files.push(inc.clone());
            }
        }
        Some(sysml_core::resolver::Project::from_files(&all_files))
    } else if files.len() > 1 {
        // Multi-file lint: auto-resolve imports between the given files
        Some(sysml_core::resolver::Project::from_files(files))
    } else {
        None
    };

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut had_parse_error = false;

    for file_path in files {
        let path_str = file_path.to_string_lossy().to_string();

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                had_parse_error = true;
                continue;
            }
        };

        let mut model = sysml_parser::parse_file(&path_str, &source);

        // Resolve imports if project is available
        if let Some(ref proj) = project {
            model.resolved_imports = proj.resolve_imports(&model);
        }

        for check in &active_checks {
            let diagnostics = check.run(&model);
            for d in diagnostics {
                if d.severity >= min_severity {
                    all_diagnostics.push(d);
                }
            }
        }
    }

    all_diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.span.start_row.cmp(&b.span.start_row))
            .then(a.span.start_col.cmp(&b.span.start_col))
    });

    if !all_diagnostics.is_empty() {
        let output = match cli.format.as_str() {
            "json" => output::format_json(&all_diagnostics),
            _ => output::format_text(&all_diagnostics),
        };
        println!("{}", output);
    }

    if !cli.quiet {
        output::print_summary(&all_diagnostics);
    }

    let has_errors = all_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    if has_errors || had_parse_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

// ========================================================================
// List command
// ========================================================================

#[allow(clippy::too_many_arguments)]
fn run_list(
    cli: &Cli,
    files: &[PathBuf],
    kind: Option<&str>,
    name: Option<&str>,
    parent: Option<&str>,
    unused: bool,
    abstract_only: bool,
    visibility: Option<&str>,
    view: Option<&str>,
) -> ExitCode {
    use sysml_core::model::{DefKind, Visibility};
    use sysml_core::query::{self, KindFilter, ListFilter, parse_kind_filter};

    let kind_filter = kind.and_then(|k| parse_kind_filter(k).or_else(|| {
        // Fallback for extra aliases not in parse_kind_filter
        Some(match k {
            "use-cases" => KindFilter::DefKind(DefKind::UseCase),
            "verifications" => KindFilter::DefKind(DefKind::Verification),
            "allocations" => KindFilter::DefKind(DefKind::Allocation),
            "ref" => KindFilter::UsageKind("ref".to_string()),
            other => KindFilter::UsageKind(other.to_string()),
        })
    }));

    let vis_filter = visibility.map(|v| match v {
        "public" | "pub" => Visibility::Public,
        "private" | "priv" => Visibility::Private,
        "protected" | "prot" => Visibility::Protected,
        _ => {
            eprintln!("warning: unknown visibility `{}`, expected: public, private, protected", v);
            Visibility::Public
        }
    });

    let mut filter = ListFilter {
        kind: kind_filter,
        name_pattern: name.map(|s| s.to_string()),
        parent: parent.map(|s| s.to_string()),
        unused_only: unused,
        abstract_only,
        visibility: vis_filter,
    };

    // --view flag: will be applied per-file after parsing (view defs are in the model)
    let view_name = view.map(|s| s.to_string());

    // Collect into owned data to avoid lifetime issues across files
    struct ListRow {
        file: String,
        name: String,
        kind: String,
        line: usize,
        parent: Option<String>,
        type_ref: Option<String>,
        short_name: Option<String>,
        doc: Option<String>,
    }

    let mut rows = Vec::new();
    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);

        // Apply view filter if --view is specified
        if let Some(ref vn) = view_name {
            if let Some(view_filter) = query::filter_from_view(&model, vn) {
                // Merge view filter with CLI filter (CLI flags override view)
                if filter.kind.is_none() {
                    filter.kind = view_filter.kind;
                }
                if filter.parent.is_none() {
                    filter.parent = view_filter.parent;
                }
            } else {
                eprintln!("warning: view definition `{}` not found in `{}`", vn, path_str);
            }
        }

        let elements = query::list_elements(&model, &filter);
        for el in elements {
            rows.push(ListRow {
                file: path_str.clone(),
                name: el.name().to_string(),
                kind: el.kind_label().to_string(),
                line: el.span().start_row,
                parent: el.parent_def().map(|s| s.to_string()),
                type_ref: el.type_ref().map(|s| s.to_string()),
                short_name: el.short_name().map(|s| s.to_string()),
                doc: el.doc().map(|s| s.to_string()),
            });
        }
    }

    if cli.format == "json" {
        let json: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let mut obj = serde_json::json!({
                    "file": r.file,
                    "name": r.name,
                    "kind": r.kind,
                    "line": r.line,
                });
                if let Some(ref p) = r.parent {
                    obj["parent"] = serde_json::json!(p);
                }
                if let Some(ref t) = r.type_ref {
                    obj["type"] = serde_json::json!(t);
                }
                if let Some(ref sn) = r.short_name {
                    obj["short_name"] = serde_json::json!(sn);
                }
                if let Some(ref doc) = r.doc {
                    obj["doc"] = serde_json::json!(doc);
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if rows.is_empty() {
            println!("No matching elements found.");
            return ExitCode::SUCCESS;
        }
        for r in &rows {
            let loc = format!("{}:{}", r.file, r.line);
            let parent_str = r
                .parent
                .as_ref()
                .map(|p| format!(" (in {})", p))
                .unwrap_or_default();
            let type_str = r
                .type_ref
                .as_ref()
                .map(|t| format!(" : {}", t))
                .unwrap_or_default();
            println!(
                "  {:<14} {}{}{} [{}]",
                r.kind, r.name, type_str, parent_str, loc,
            );
        }
        if !cli.quiet {
            eprintln!("{} element(s) found.", rows.len());
        }
    }

    ExitCode::SUCCESS
}

// ========================================================================
// Show command
// ========================================================================

fn run_show(cli: &Cli, file: &PathBuf, element: &str) -> ExitCode {
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

// ========================================================================
// Trace command
// ========================================================================

fn run_trace(cli: &Cli, files: &[PathBuf], check: bool, min_coverage: f64) -> ExitCode {
    use sysml_core::query;

    // Parse all files into a merged model
    let mut merged = sysml_core::model::Model::new("(merged)".to_string());
    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        merged.definitions.extend(model.definitions);
        merged.usages.extend(model.usages);
        merged.satisfactions.extend(model.satisfactions);
        merged.verifications.extend(model.verifications);
    }

    let rows = query::trace_requirements(&merged);
    let coverage = query::trace_coverage(&rows);

    if cli.format == "json" {
        let json = serde_json::json!({
            "requirements": rows.iter().map(|r| {
                serde_json::json!({
                    "name": r.requirement,
                    "satisfied_by": r.satisfied_by,
                    "verified_by": r.verified_by,
                })
            }).collect::<Vec<_>>(),
            "coverage": {
                "total": coverage.total_requirements,
                "satisfied": coverage.satisfied_count,
                "verified": coverage.verified_count,
                "fully_traced": coverage.fully_traced_count,
            },
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if rows.is_empty() {
            println!("No requirements found.");
            return ExitCode::SUCCESS;
        }

        // Print RTM table
        println!(
            "{:<20} {:<20} {:<20}",
            "Requirement", "Satisfied By", "Verified By"
        );
        println!("{}", "-".repeat(60));
        for row in &rows {
            let sat = if row.satisfied_by.is_empty() {
                "-".to_string()
            } else {
                row.satisfied_by.join(", ")
            };
            let ver = if row.verified_by.is_empty() {
                "-".to_string()
            } else {
                row.verified_by.join(", ")
            };
            println!("{:<20} {:<20} {:<20}", row.requirement, sat, ver);
        }

        // Print coverage summary
        if coverage.total_requirements > 0 {
            let sat_pct =
                100.0 * coverage.satisfied_count as f64 / coverage.total_requirements as f64;
            let ver_pct =
                100.0 * coverage.verified_count as f64 / coverage.total_requirements as f64;
            println!();
            println!(
                "Coverage: {}/{} satisfied ({:.0}%), {}/{} verified ({:.0}%)",
                coverage.satisfied_count,
                coverage.total_requirements,
                sat_pct,
                coverage.verified_count,
                coverage.total_requirements,
                ver_pct,
            );
        }
    }

    if check {
        let total = coverage.total_requirements;
        if total == 0 {
            return ExitCode::SUCCESS;
        }
        let traced_pct = 100.0 * coverage.fully_traced_count as f64 / total as f64;
        if traced_pct < min_coverage {
            eprintln!(
                "error: trace coverage {:.0}% is below minimum {:.0}%",
                traced_pct, min_coverage
            );
            return ExitCode::from(1);
        }
        if coverage.satisfied_count < total || coverage.verified_count < total {
            eprintln!(
                "error: {} requirement(s) missing satisfaction or verification",
                total - coverage.fully_traced_count
            );
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

// ========================================================================
// Interfaces command
// ========================================================================

fn run_interfaces(cli: &Cli, files: &[PathBuf], unconnected_only: bool) -> ExitCode {
    use sysml_core::query;

    let mut merged = sysml_core::model::Model::new("(merged)".to_string());
    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        merged.definitions.extend(model.definitions);
        merged.usages.extend(model.usages);
        merged.connections.extend(model.connections);
    }

    let ports = if unconnected_only {
        query::unconnected_ports(&merged)
    } else {
        query::list_ports(&merged)
    };

    if cli.format == "json" {
        let json: Vec<serde_json::Value> = ports
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "owner": p.owner,
                    "type": p.type_ref,
                    "direction": p.direction.map(|d| d.label()),
                    "conjugated": p.is_conjugated,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        if ports.is_empty() {
            if unconnected_only {
                println!("All ports are connected.");
            } else {
                println!("No ports found.");
            }
            return ExitCode::SUCCESS;
        }

        let header = if unconnected_only {
            "Unconnected Ports:"
        } else {
            "Ports:"
        };
        println!("{}", header);
        println!(
            "  {:<15} {:<15} {:<15} {:<10}",
            "Name", "Owner", "Type", "Direction"
        );
        println!("  {}", "-".repeat(55));
        for p in &ports {
            let dir = p
                .direction
                .map(|d| d.label().to_string())
                .unwrap_or_else(|| "-".to_string());
            let t = p.type_ref.as_deref().unwrap_or("-");
            println!(
                "  {:<15} {:<15} {:<15} {:<10}",
                p.name, p.owner, t, dir
            );
        }
        if !cli.quiet {
            eprintln!("{} port(s) found.", ports.len());
        }
    }

    ExitCode::SUCCESS
}

// ========================================================================
// Diagram command
// ========================================================================

fn run_diagram(
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

fn run_simulate(cli: &Cli, kind: &SimulateCommand) -> ExitCode {
    match kind {
        SimulateCommand::Eval {
            file,
            bindings,
            name,
        } => run_sim_eval(cli, file, bindings, name.as_deref()),
        SimulateCommand::StateMachine {
            file,
            name,
            events,
            max_steps,
            bindings,
        } => run_sim_state_machine(cli, file, name.as_deref(), events, *max_steps, bindings),
        SimulateCommand::ActionFlow {
            file,
            name,
            max_steps,
            bindings,
        } => run_sim_action_flow(cli, file, name.as_deref(), *max_steps, bindings),
        SimulateCommand::List { file } => run_sim_list(cli, file),
    }
}

fn parse_bindings(bindings: &[String]) -> sysml_core::sim::expr::Env {
    use sysml_core::sim::expr::{Env, Value};
    let mut env = Env::new();
    for b in bindings {
        if let Some((name, val_str)) = b.split_once('=') {
            let value = if let Ok(n) = val_str.parse::<f64>() {
                Value::Number(n)
            } else if val_str == "true" {
                Value::Bool(true)
            } else if val_str == "false" {
                Value::Bool(false)
            } else {
                Value::String(val_str.to_string())
            };
            env.bind(name.trim(), value);
        }
    }
    env
}

fn collect_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files_recursive(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "sysml" || ext == "kerml" {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }
    }
}

fn read_source(file: &PathBuf) -> Result<(String, String), ExitCode> {
    let path_str = file.to_string_lossy().to_string();
    match std::fs::read_to_string(file) {
        Ok(s) => Ok((path_str, s)),
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path_str, e);
            Err(ExitCode::from(1))
        }
    }
}

/// Prompt the user to select from a list of items interactively.
/// Returns None if not a TTY or selection fails.
fn select_item(kind: &str, items: &[&str]) -> Option<usize> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        eprintln!(
            "error: multiple {}s found. Use --name to specify one, or run interactively.",
            kind
        );
        eprintln!("  available: {}", items.join(", "));
        return None;
    }

    eprintln!("Multiple {}s found. Select one:", kind);
    match FuzzySelect::new()
        .items(items)
        .default(0)
        .interact_opt()
    {
        Ok(Some(idx)) => Some(idx),
        Ok(None) => {
            eprintln!("No selection made.");
            None
        }
        Err(e) => {
            eprintln!("error: selection failed: {}", e);
            None
        }
    }
}

/// Interactively prompt for events to feed into a state machine simulation.
///
/// Shows available signal triggers and lets the user pick events one at a time.
/// Returns the collected event sequence.
fn prompt_events(available_signals: &[String]) -> Vec<String> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        eprintln!(
            "error: this state machine requires events. Use --events to specify them."
        );
        eprintln!("  available signals: {}", available_signals.join(", "));
        return Vec::new();
    }

    let mut events = Vec::new();
    let mut items: Vec<String> = available_signals.to_vec();
    items.push("[done — run simulation]".to_string());

    eprintln!("This state machine has signal triggers. Select events to inject:");
    eprintln!("  (select [done] when finished)");

    loop {
        let selection = FuzzySelect::new()
            .items(&items)
            .default(0)
            .interact_opt();

        match selection {
            Ok(Some(idx)) if idx < available_signals.len() => {
                events.push(available_signals[idx].clone());
                eprintln!("  events so far: [{}]", events.join(", "));
            }
            Ok(Some(_)) => {
                // Selected "[done]"
                break;
            }
            Ok(None) | Err(_) => {
                break;
            }
        }
    }

    events
}

fn run_sim_eval(
    cli: &Cli,
    file: &PathBuf,
    bindings: &[String],
    name: Option<&str>,
) -> ExitCode {
    use sysml_core::sim::constraint_eval::*;
    use sysml_core::sim::eval;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let env = parse_bindings(bindings);

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);

    let target_constraints: Vec<&ConstraintModel> = if let Some(n) = name {
        constraints.iter().filter(|c| c.name == n).collect()
    } else {
        constraints.iter().collect()
    };

    let target_calcs: Vec<&CalcModel> = if let Some(n) = name {
        calcs.iter().filter(|c| c.name == n).collect()
    } else {
        calcs.iter().collect()
    };

    if target_constraints.is_empty() && target_calcs.is_empty() {
        if let Some(n) = name {
            eprintln!("error: no constraint or calculation named `{}` found", n);
            // Suggest available items
            let available: Vec<&str> = constraints.iter().map(|c| c.name.as_str())
                .chain(calcs.iter().map(|c| c.name.as_str()))
                .collect();
            if !available.is_empty() {
                eprintln!("  available: {}", available.join(", "));
            }
        } else {
            eprintln!("error: no constraints or calculations found in `{}`", path_str);
        }
        return ExitCode::from(1);
    }

    let is_json = cli.format == "json";
    let mut results = Vec::new();

    for c in &target_constraints {
        if let Some(ref expr) = c.expression {
            let result = eval::evaluate_constraint(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "constraint",
                    "name": c.name,
                    "result": match &result {
                        Ok(b) => serde_json::json!(b),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(b) => println!(
                        "constraint {}: {}",
                        c.name,
                        if b { "satisfied" } else { "violated" }
                    ),
                    Err(e) => println!("constraint {}: error: {}", c.name, e),
                }
            }
        }
    }

    for c in &target_calcs {
        if let Some(ref expr) = c.return_expr {
            let result = eval::evaluate(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "calculation",
                    "name": c.name,
                    "result": match &result {
                        Ok(v) => serde_json::json!(v),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(v) => println!("calc {}: {}", c.name, v),
                    Err(e) => println!("calc {}: error: {}", c.name, e),
                }
            }
        }
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    }

    ExitCode::SUCCESS
}

fn run_sim_state_machine(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    events: &[String],
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml_core::sim::state_machine::Trigger;
    use sysml_core::sim::state_parser::extract_state_machines;
    use sysml_core::sim::state_sim::*;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let machines = extract_state_machines(&path_str, &source);

    if machines.is_empty() {
        eprintln!("error: no state machines found in `{}`", path_str);
        return ExitCode::from(1);
    }

    let machine = if let Some(n) = name {
        match machines.iter().find(|m| m.name == n) {
            Some(m) => m,
            None => {
                eprintln!("error: no state machine named `{}` found", n);
                let available: Vec<&str> = machines.iter().map(|m| m.name.as_str()).collect();
                if !available.is_empty() {
                    eprintln!("  available: {}", available.join(", "));
                }
                return ExitCode::from(1);
            }
        }
    } else if machines.len() == 1 {
        &machines[0]
    } else {
        // Interactive selection
        match select_item("state machine", &machines.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()) {
            Some(idx) => &machines[idx],
            None => return ExitCode::from(1),
        }
    };

    // Collect available signal triggers from this machine
    let available_signals: Vec<String> = machine
        .transitions
        .iter()
        .filter_map(|t| match &t.trigger {
            Some(Trigger::Signal(s)) => Some(s.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // If no events provided and the machine has signal triggers, prompt interactively
    let effective_events = if events.is_empty() && !available_signals.is_empty() {
        prompt_events(&available_signals)
    } else {
        events.to_vec()
    };

    let config = SimConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
        events: effective_events,
    };

    let result = simulate(machine, &config);

    let output = match cli.format.as_str() {
        "json" => format_trace_json(&result),
        _ => format_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        SimStatus::Completed | SimStatus::Running => ExitCode::SUCCESS,
        SimStatus::Deadlocked => ExitCode::from(1),
        SimStatus::MaxSteps => ExitCode::from(2),
    }
}

fn run_sim_action_flow(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml_core::sim::action_exec::*;
    use sysml_core::sim::action_parser::extract_actions;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let actions = extract_actions(&path_str, &source);

    if actions.is_empty() {
        eprintln!("error: no action definitions found in `{}`", path_str);
        return ExitCode::from(1);
    }

    let action = if let Some(n) = name {
        match actions.iter().find(|a| a.name == n) {
            Some(a) => a,
            None => {
                eprintln!("error: no action named `{}` found", n);
                let available: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
                if !available.is_empty() {
                    eprintln!("  available: {}", available.join(", "));
                }
                return ExitCode::from(1);
            }
        }
    } else if actions.len() == 1 {
        &actions[0]
    } else {
        match select_item("action", &actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>()) {
            Some(idx) => &actions[idx],
            None => return ExitCode::from(1),
        }
    };

    let config = ActionExecConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
    };

    let result = execute_action(action, &config);

    let output = match cli.format.as_str() {
        "json" => format_action_trace_json(&result),
        _ => format_action_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        ActionExecStatus::Completed => ExitCode::SUCCESS,
        ActionExecStatus::Error => ExitCode::from(1),
        ActionExecStatus::MaxSteps => ExitCode::from(2),
        ActionExecStatus::Running => ExitCode::SUCCESS,
    }
}

fn run_sim_list(cli: &Cli, file: &PathBuf) -> ExitCode {
    use sysml_core::sim::action_parser::extract_actions;
    use sysml_core::sim::constraint_eval::*;
    use sysml_core::sim::state_machine::Trigger;
    use sysml_core::sim::state_parser::extract_state_machines;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);
    let machines = extract_state_machines(&path_str, &source);
    let actions = extract_actions(&path_str, &source);

    if cli.format == "json" {
        // Structured JSON output for tool integration
        let json = serde_json::json!({
            "constraints": constraints.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "params": c.params.iter().map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "type": p.type_ref.as_deref().unwrap_or("?"),
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "calculations": calcs.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "params": c.params.iter().map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "type": p.type_ref.as_deref().unwrap_or("?"),
                        })
                    }).collect::<Vec<_>>(),
                    "return_type": c.return_type.as_deref().unwrap_or("?"),
                })
            }).collect::<Vec<_>>(),
            "state_machines": machines.iter().map(|m| {
                let triggers: Vec<&str> = m.transitions.iter()
                    .filter_map(|t| match &t.trigger {
                        Some(Trigger::Signal(s)) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let guards: Vec<String> = m.transitions.iter()
                    .filter(|t| t.guard.is_some())
                    .filter_map(|t| t.name.clone())
                    .collect();
                serde_json::json!({
                    "name": m.name,
                    "entry_state": m.entry_state,
                    "states": m.states.iter().map(|s| &s.name).collect::<Vec<_>>(),
                    "transitions": m.transitions.len(),
                    "triggers": triggers,
                    "guarded_transitions": guards,
                })
            }).collect::<Vec<_>>(),
            "actions": actions.iter().map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "steps": a.steps.len(),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
        return ExitCode::SUCCESS;
    }

    if constraints.is_empty() && calcs.is_empty() && machines.is_empty() && actions.is_empty() {
        println!("No simulatable constructs found in `{}`.", path_str);
        return ExitCode::SUCCESS;
    }

    if !constraints.is_empty() {
        println!("Constraints:");
        for c in &constraints {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            println!("  {} ({})", c.name, params.join(", "));
        }
        println!();
    }

    if !calcs.is_empty() {
        println!("Calculations:");
        for c in &calcs {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            let ret = c.return_type.as_deref().unwrap_or("?");
            println!("  {} ({}) -> {}", c.name, params.join(", "), ret);
        }
        println!();
    }

    if !machines.is_empty() {
        println!("State Machines:");
        for m in &machines {
            let states: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
            let entry = m.entry_state.as_deref().unwrap_or("?");
            let triggers: Vec<&str> = m
                .transitions
                .iter()
                .filter_map(|t| match &t.trigger {
                    Some(Trigger::Signal(s)) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            println!(
                "  {} [entry: {}, states: {}, transitions: {}{}]",
                m.name,
                entry,
                states.join(", "),
                m.transitions.len(),
                if triggers.is_empty() {
                    String::new()
                } else {
                    format!(", triggers: {}", triggers.join(", "))
                }
            );
        }
        println!();
    }

    if !actions.is_empty() {
        println!("Actions:");
        for a in &actions {
            println!("  {} ({} steps)", a.name, a.steps.len());
        }
        println!();
    }

    ExitCode::SUCCESS
}

// === Export commands ===

fn run_export(cli: &Cli, kind: &ExportCommand) -> ExitCode {
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

// === New command ===

fn run_new(
    kind: &str,
    name: &str,
    output: Option<&PathBuf>,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    doc: Option<&str>,
    members: &[String],
    exposes: &[String],
    filter: Option<&str>,
    append: bool,
    inside: Option<&str>,
    dry_run: bool,
) -> ExitCode {
    use sysml_core::codegen::template::*;
    use sysml_core::codegen::edit;

    let def_kind = match parse_template_kind(kind) {
        Some(k) => k,
        None => {
            eprintln!("error: unknown element kind `{}`", kind);
            eprintln!("  available: part-def, port-def, action-def, state-def, constraint-def,");
            eprintln!("            calc-def, requirement, enum-def, attribute-def, item-def,");
            eprintln!("            view-def, viewpoint-def, package, use-case, connection-def,");
            eprintln!("            flow-def, interface-def, allocation-def");
            return ExitCode::from(1);
        }
    };

    let parsed_members: Vec<MemberSpec> = members
        .iter()
        .filter_map(|s| parse_member_spec(s))
        .collect();

    let indent = if inside.is_some() { 4 } else { 0 };

    let opts = TemplateOptions {
        kind: def_kind,
        name: name.to_string(),
        super_type: extends.map(|s| s.to_string()),
        is_abstract,
        short_name: short_name.map(|s| s.to_string()),
        doc: doc.map(|s| s.to_string()),
        members: parsed_members,
        exposes: exposes.to_vec(),
        filter: filter.map(|s| s.to_string()),
        indent,
    };

    let generated = generate_template(&opts);

    if let Some(parent_name) = inside {
        let file = match output {
            Some(f) => f,
            None => {
                eprintln!("error: --inside requires --output <file>");
                return ExitCode::from(1);
            }
        };
        let (path_str, source) = match read_source(file) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        let text_edit = match edit::insert_member(&source, &model, parent_name, generated.trim()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(1);
            }
        };
        let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(1);
            }
        };
        if dry_run {
            print!("{}", edit::diff(&source, &result, &path_str));
        } else {
            if let Err(e) = std::fs::write(file, &result) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            eprintln!("Added {} inside {} in {}", name, parent_name, path_str);
        }
        return ExitCode::SUCCESS;
    }

    if append {
        if let Some(file) = output {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let text_edit = edit::insert_top_level(&source, generated.trim());
            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };
            if dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Appended {} to {}", name, path_str);
            }
            return ExitCode::SUCCESS;
        }
    }

    if dry_run {
        print!("{}", generated);
    } else if let Some(file) = output {
        if let Err(e) = std::fs::write(file, &generated) {
            eprintln!("error: cannot write `{}`: {}", file.display(), e);
            return ExitCode::from(1);
        }
        eprintln!("Wrote {} to {}", name, file.display());
    } else {
        print!("{}", generated);
    }

    ExitCode::SUCCESS
}

// === Edit command ===

fn run_edit(kind: &EditCommand) -> ExitCode {
    use sysml_core::codegen::edit;
    use sysml_core::codegen::template;

    match kind {
        EditCommand::Add {
            file, kind, name, type_ref, inside, dry_run,
        } => {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let model = sysml_parser::parse_file(&path_str, &source);

            // Only generate a full definition template for explicit def kinds
            // (e.g., "part-def", "port def", "package"), not usage keywords like "part"
            let is_def_kind = kind.contains("def") || kind.contains("package")
                || kind.contains("pkg");
            let text = if is_def_kind {
                if let Some(def_kind) = template::parse_template_kind(kind) {
                    let opts = template::TemplateOptions {
                        kind: def_kind,
                        name: name.clone(),
                        super_type: type_ref.clone(),
                        is_abstract: false,
                        short_name: None,
                        doc: None,
                        members: Vec::new(),
                        exposes: Vec::new(),
                        filter: None,
                        indent: if inside.is_some() { 4 } else { 0 },
                    };
                    template::generate_template(&opts)
                } else {
                    eprintln!("error: unknown definition kind `{}`", kind);
                    return ExitCode::from(1);
                }
            } else {
                // Usage format: kind name [: type];
                let t = type_ref
                    .as_ref()
                    .map(|t| format!(" : {}", t))
                    .unwrap_or_default();
                format!("{} {}{};", kind, name, t)
            };

            // Determine where to insert
            let target_parent: Option<String> = if let Some(parent) = inside {
                Some(parent.clone())
            } else if !is_def_kind {
                // For usage-level elements, prompt for which definition to insert into
                let defs_with_body: Vec<&str> = model.definitions.iter()
                    .filter(|d| d.body_end_byte.is_some())
                    .map(|d| d.name.as_str())
                    .collect();
                if defs_with_body.len() == 1 {
                    Some(defs_with_body[0].to_string())
                } else if defs_with_body.len() > 1 {
                    match select_item("definition", &defs_with_body) {
                        Some(idx) => Some(defs_with_body[idx].to_string()),
                        None => return ExitCode::from(1),
                    }
                } else {
                    None // No definitions with bodies — insert top-level
                }
            } else {
                None
            };

            let text_edit = if let Some(ref parent) = target_parent {
                match edit::insert_member(&source, &model, parent, text.trim()) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return ExitCode::from(1);
                    }
                }
            } else {
                edit::insert_top_level(&source, text.trim())
            };

            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            if *dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Added `{}` to {}", name, path_str);
            }
            ExitCode::SUCCESS
        }
        EditCommand::Remove { file, name, dry_run } => {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let model = sysml_parser::parse_file(&path_str, &source);

            let text_edit = match edit::remove_element(&source, &model, name) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            if *dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Removed `{}` from {}", name, path_str);
            }
            ExitCode::SUCCESS
        }
        EditCommand::Rename { file, old_name, new_name, dry_run } => {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let model = sysml_parser::parse_file(&path_str, &source);

            let plan = match edit::rename_element(&source, &model, old_name, new_name) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            let result = match edit::apply_edits(&source, &plan) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            if *dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Renamed `{}` to `{}` in {}", old_name, new_name, path_str);
            }
            ExitCode::SUCCESS
        }
    }
}

// === Fmt command ===

fn run_fmt(
    files: &[PathBuf],
    check: bool,
    show_diff: bool,
    indent_width: usize,
) -> ExitCode {
    use sysml_core::codegen::edit;

    let mut any_unformatted = false;

    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };

        let formatted = format_sysml(&source, indent_width);

        if formatted == source {
            continue;
        }

        any_unformatted = true;

        if check {
            eprintln!("{}: not formatted", path_str);
        } else if show_diff {
            print!("{}", edit::diff(&source, &formatted, &path_str));
        } else {
            if let Err(e) = std::fs::write(file_path, &formatted) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            eprintln!("Formatted {}", path_str);
        }
    }

    if check && any_unformatted {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn format_sysml(source: &str, indent_width: usize) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    let indent_str = " ".repeat(indent_width);

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        if trimmed.starts_with('}') {
            depth = depth.saturating_sub(1);
        }

        for _ in 0..depth {
            out.push_str(&indent_str);
        }
        out.push_str(trimmed);
        out.push('\n');

        if trimmed.ends_with('{') {
            depth += 1;
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }

    out
}
