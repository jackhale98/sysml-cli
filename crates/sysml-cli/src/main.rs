/// sysml-cli: SysML v2 command-line tool for validation, simulation,
/// diagram generation, and model management.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;
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
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format: text, json.
    #[arg(short, long, default_value = "text", global = true)]
    pub(crate) format: String,

    /// Suppress summary line on stderr.
    #[arg(short, long, global = true)]
    pub(crate) quiet: bool,

    /// Additional SysML files or directories to include for import resolution.
    /// Definitions from these files are available to imported names.
    #[arg(short = 'I', long = "include", global = true)]
    pub(crate) include: Vec<PathBuf>,
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
    /// Generate shell completions.
    ///
    /// EXAMPLES:
    ///   sysml-cli completions bash > ~/.local/share/bash-completion/completions/sysml-cli
    ///   sysml-cli completions zsh > ~/.zfunc/_sysml-cli
    ///   sysml-cli completions fish > ~/.config/fish/completions/sysml-cli.fish
    Completions {
        /// Shell: bash, zsh, fish, elvish, powershell.
        #[arg(required = true)]
        shell: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum EditCommand {
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
pub(crate) enum SimulateCommand {
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
pub(crate) enum ExportCommand {
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
        } => commands::lint::run(&cli, files, disable, severity),
        Command::List {
            files,
            kind,
            name,
            parent,
            unused,
            abstract_only,
            visibility,
            view,
        } => commands::list::run(
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
        Command::Show { file, element } => commands::show::run(&cli, file, element),
        Command::Trace {
            files,
            check,
            min_coverage,
        } => commands::trace::run(&cli, files, *check, *min_coverage),
        Command::Interfaces {
            files,
            unconnected,
        } => commands::interfaces::run(&cli, files, *unconnected),
        Command::Diagram {
            file,
            diagram_type,
            output_format,
            scope,
            direction,
            depth,
        } => commands::diagram::run(&cli, file, diagram_type, output_format, scope.as_deref(), direction.as_deref(), *depth),
        Command::Simulate { kind } => commands::simulate::run(&cli, kind),
        Command::Export { kind } => commands::export::run(&cli, kind),
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
        } => commands::new::run(
            kind, name, output.as_ref(), extends.as_deref(), *r#abstract,
            short_name.as_deref(), doc.as_deref(), members, exposes,
            filter.as_deref(), *append, inside.as_deref(), *dry_run,
        ),
        Command::Edit { kind } => commands::edit::run(kind),
        Command::Fmt {
            files,
            check,
            diff,
            indent_width,
        } => commands::fmt::run(files, *check, *diff, *indent_width),
        Command::Completions { shell } => {
            generate_completions(shell);
            ExitCode::SUCCESS
        }
    }
}

fn generate_completions(shell: &str) {
    use clap::CommandFactory;
    use clap_complete::{generate, Shell};

    let shell = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "elvish" => Shell::Elvish,
        "powershell" | "ps" => Shell::PowerShell,
        other => {
            eprintln!("error: unknown shell `{}`. Use: bash, zsh, fish, elvish, powershell", other);
            return;
        }
    };

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "sysml-cli", &mut std::io::stdout());
}

pub(crate) fn read_source(file: &PathBuf) -> Result<(String, String), ExitCode> {
    let path_str = file.to_string_lossy().to_string();
    match std::fs::read_to_string(file) {
        Ok(s) => Ok((path_str, s)),
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path_str, e);
            Err(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_bindings(bindings: &[String]) -> sysml_core::sim::expr::Env {
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

pub(crate) fn collect_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) {
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

/// Prompt the user to select from a list of items interactively.
/// Returns None if not a TTY or selection fails.
pub(crate) fn select_item(kind: &str, items: &[&str]) -> Option<usize> {
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
pub(crate) fn prompt_events(available_signals: &[String]) -> Vec<String> {
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
