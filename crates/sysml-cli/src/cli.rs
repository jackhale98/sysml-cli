//! CLI argument definitions: Cli struct, Command enum, and all subcommand enums.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sysml",
    about = "SysML v2 command-line tool for validation, simulation, diagram generation, and model management",
    long_about = "\
sysml works with SysML v2 models in textual notation.

SysML v2 is the next-generation systems modeling language from OMG. It uses \
a textual notation where 'definitions' declare reusable types (part def, port def, \
action def, etc.) and 'usages' create instances of those types within a context.

GETTING STARTED:
  Validate a model:       sysml check model.sysml
  List model elements:    sysml list --kind parts model.sysml
  Show element details:   sysml show model.sysml Vehicle
  Generate a diagram:     sysml diagram -t bdd model.sysml
  Simulate a state machine: sysml simulate state-machine model.sysml
  Run an analysis case:   sysml analyze run model.sysml -n GapAnalysis
  Roll up an attribute:   sysml rollup compute --root Vehicle --attr mass
  Add to a model:         sysml add model.sysml part-def Vehicle --doc 'A vehicle'
  Format a file:          sysml fmt model.sysml

LEARN MORE:
  SysML v2 spec:          https://www.omgsysml.org/
  This tool:              https://github.com/jackhale98/sysml-cli",
    version
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    /// Output format.
    #[arg(short, long, default_value = "text", global = true,
          value_parser = ["text", "json"])]
    pub(crate) format: String,

    /// Suppress summary line on stderr.
    #[arg(short, long, global = true)]
    pub(crate) quiet: bool,

    /// Additional SysML files or directories to include for import resolution.
    /// Definitions from these files are available to imported names.
    #[arg(short = 'I', long = "include", global = true)]
    pub(crate) include: Vec<PathBuf>,

    /// Path to the SysML v2 standard library directory.
    /// Definitions from the standard library are available for import resolution.
    /// Can also be set via SYSML_STDLIB_PATH environment variable or
    /// stdlib_path in .sysml/config.toml.
    #[arg(long = "stdlib-path", global = true, env = "SYSML_STDLIB_PATH")]
    pub(crate) stdlib_path: Option<PathBuf>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Command {
    /// List model elements with optional filters.
    ///
    /// Lists definitions and usages from SysML v2 files. Filter by kind,
    /// name pattern, parent definition, visibility, or structural properties.
    ///
    /// SysML v2 elements are either 'definitions' (reusable types like
    /// part def, port def) or 'usages' (instances like part, port).
    #[command(visible_alias = "ls")]
    List {
        /// SysML v2 files to inspect (omit to scan project).
        files: Vec<PathBuf>,

        /// Filter by element kind.
        /// parts, ports, actions, states, requirements, constraints, etc. show both defs and usages.
        /// Append -def or -usage to restrict (e.g., part-def, action-usage).
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

        /// Show only variation points (`variation` defs/usages, Ch 35).
        #[arg(long)]
        variations: bool,

        /// Show only variant choices (`variant` usages, Ch 35).
        #[arg(long)]
        variants: bool,

        /// Show only elements annotated with this metadata type
        /// (e.g. --metadata Status).
        #[arg(long)]
        metadata: Option<String>,

        /// Constrain metadata values: KEY=VALUE (repeatable, use with
        /// --metadata; e.g. --where status=draft).
        #[arg(long = "where", value_name = "KEY=VALUE")]
        where_clauses: Vec<String>,

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
    /// Use --raw to print the original SysML source text for the element.
    Show {
        /// SysML v2 file to inspect.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the element to show.
        #[arg(required = true)]
        element: String,

        /// Print the raw SysML source text of the element.
        #[arg(long)]
        raw: bool,
    },
    /// Generate a requirements traceability matrix.
    ///
    /// Lists all requirement definitions and shows their satisfaction
    /// and verification status. In SysML v2, requirements are traced via
    /// 'satisfy' and 'verify' relationships.
    Trace {
        /// SysML v2 files to analyze (omit to scan project).
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
    /// DIAGRAM TYPES (SysML v2 StandardViewDefinitions):
    ///   gv     — General View (definitions and relationships)
    ///   iv     — Interconnection View (internal structure with ports)
    ///   afv    — Action Flow View (actions with control nodes)
    ///   stv    — State Transition View (states and transitions)
    ///   sv     — Sequence View (lifelines and messages)
    ///   grv    — Grid View (tabular/matrix)
    ///   bv     — Browser View (package hierarchy)
    ///
    /// SPECIALIZATIONS:
    ///   par    — Parametric (constraint parameters)
    ///   req    — Requirements grid
    ///   trace  — Traceability matrix
    ///   alloc  — Allocation matrix
    ///   ucd    — Use Case view
    ///
    /// LEGACY ALIASES (still supported):
    ///   bdd=gv  ibd=iv  stm=stv  act=afv  pkg=bv
    ///
    /// OUTPUT FORMATS:
    ///   mermaid  — Mermaid.js (render in GitHub, Obsidian, etc.)
    ///   plantuml — PlantUML (puml alias)
    ///   dot      — Graphviz DOT
    ///   d2       — Terrastruct D2
    ///
    /// EXAMPLES:
    ///   sysml diagram -t bdd model.sysml
    ///   sysml diagram -t ibd -s Vehicle model.sysml
    ///   sysml diagram -t trace model.sysml
    ///   sysml diagram -t alloc -o plantuml model.sysml
    ///   sysml diagram -t bdd --view StructureView model.sysml
    Diagram {
        /// SysML v2 file to generate diagram from.
        #[arg(required = true)]
        file: PathBuf,

        /// Diagram type. Optional when --view names a view def with a
        /// `render as` clause — the view's declared rendering is used.
        #[arg(short = 't', long = "type",
              value_parser = ["gv", "iv", "afv", "stv", "sv", "grv", "bv",
                              "bdd", "ibd", "stm", "act", "req", "pkg", "par",
                              "trace", "alloc", "ucd",
                              "state", "activity", "requirements", "package",
                              "parametric", "traceability", "allocation",
                              "usecase", "use-case", "sequence"],
              help_heading = "Diagram")]
        diagram_type: Option<String>,

        /// Diagram renderer: mermaid, plantuml, dot, d2 (and aliases).
        #[arg(short = 'r', long = "renderer", default_value = "mermaid",
              value_parser = ["mermaid", "mmd", "plantuml", "puml", "dot", "graphviz", "d2", "terrastruct"])]
        output_format: String,

        /// Focus diagram on a specific definition.
        /// bdd: show only this def and its children/relationships.
        /// ibd: show internal structure (ports, parts, connections).
        /// stm/act: show this specific state machine or action.
        #[arg(long)]
        scope: Option<String>,

        /// Apply a named SysML v2 view definition.
        /// The view's expose and filter clauses determine which elements
        /// appear, and its `render as` clause supplies the diagram type.
        #[arg(long)]
        view: Option<String>,

        /// Layout direction: TB (top-bottom), LR (left-right), BT, RL.
        #[arg(long)]
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
    #[command(visible_alias = "sim")]
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
    /// Add an element to a SysML model — interactively or with flags.
    ///
    /// With no arguments, launches a guided wizard using domain vocabulary.
    /// With a file, kind, and name, inserts directly (power-user mode).
    /// With --stdout, prints to terminal without modifying files.
    ///
    /// KINDS:
    ///   part-def, port-def, action-def, state-def, constraint-def, calc-def,
    ///   requirement (req), enum-def, attribute-def (attr), item-def, view-def,
    ///   viewpoint-def, package (pkg), use-case, connection-def, interface-def,
    ///   flow-def, allocation-def, part, port, attribute, action, state, item
    ///
    /// EXAMPLES:
    ///   sysml add                                        (interactive wizard)
    ///   sysml add model.sysml part-def Vehicle           (insert into file)
    ///   sysml add --stdout part-def Vehicle              (print to stdout)
    ///   sysml add model.sysml part engine -t Engine      (usage inside def)
    ///   sysml add model.sysml part-def Vehicle --doc 'A vehicle' -m 'part engine:Engine'
    ///   sysml add model.sysml enum-def Color -m red -m green -m blue
    ///   sysml add model.sysml part wheels -t Wheel -m 'part hub:Hub[4]'
    ///   sysml add model.sysml connection c1 --connect 'a.x to b.y' --inside Assy
    ///   sysml add model.sysml satisfy TempReq --by Vehicle
    ///   sysml add model.sysml import 'Vehicles::*'
    ///   sysml add --teach --stdout part-def Vehicle      (teaching comments)
    Add {
        /// Target SysML file (omit for interactive or stdout mode).
        file: Option<PathBuf>,

        /// Element kind (see KINDS above).
        kind: Option<String>,

        /// Element name.
        name: Option<String>,

        /// Type reference (`: Type` for usages, `:> Type` for defs with --extends).
        #[arg(short = 't', long)]
        type_ref: Option<String>,

        /// Insert inside this definition (auto-detected if omitted for usages).
        #[arg(long)]
        inside: Option<String>,

        /// Preview changes as a unified diff without writing.
        #[arg(long)]
        dry_run: bool,

        /// Print generated SysML to stdout without modifying files.
        #[arg(long)]
        stdout: bool,

        /// Include teaching comments (like scaffold element).
        #[arg(long)]
        teach: bool,

        /// Documentation comment text.
        #[arg(long)]
        doc: Option<String>,

        /// Specialization supertype.
        #[arg(long)]
        extends: Option<String>,

        /// Mark as abstract.
        #[arg(long)]
        r#abstract: bool,

        /// Short name alias.
        #[arg(long)]
        short_name: Option<String>,

        /// Add members (repeatable or comma-separated).
        /// Format: "[direction] kind name[:type[mult]]".
        /// For enum-def, just the member name: -m red,green,blue
        #[arg(long = "member", short = 'm', value_delimiter = ',')]
        members: Vec<String>,

        /// Connection binding endpoints (e.g., "a.portOut to b.portIn").
        #[arg(long)]
        connect: Option<String>,

        /// Create a satisfy relationship: --satisfy REQ_NAME --by ELEMENT.
        #[arg(long)]
        satisfy: Option<String>,

        /// Create a verify relationship: --verify REQ_NAME --by ELEMENT.
        #[arg(long)]
        verify: Option<String>,

        /// Target element for --satisfy or --verify.
        #[arg(long)]
        by: Option<String>,

        /// (view-def only) Expose clause.
        #[arg(long = "expose")]
        exposes: Vec<String>,

        /// (view-def only) Filter by element kind.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Remove a named element from a SysML file.
    ///
    /// Removes the element and its body from the file.
    ///
    /// EXAMPLES:
    ///   sysml remove model.sysml Engine
    ///   sysml remove model.sysml Engine --dry-run
    #[command(visible_alias = "rm")]
    Remove {
        /// Target SysML file.
        #[arg(required = true)]
        file: PathBuf,

        /// Name of the element to remove.
        #[arg(required = true)]
        name: String,

        /// Preview changes without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Rename an element and update all references.
    ///
    /// Finds all whole-word occurrences of the old name and replaces them.
    ///
    /// EXAMPLES:
    ///   sysml rename model.sysml Engine Motor
    ///   sysml rename model.sysml Engine Motor --dry-run
    Rename {
        /// Target SysML file (or first file for --project).
        #[arg(required = true)]
        file: PathBuf,

        /// Current name of the element.
        #[arg(required = true)]
        old_name: String,

        /// New name for the element.
        #[arg(required = true)]
        new_name: String,

        /// Preview changes without writing.
        #[arg(long)]
        dry_run: bool,

        /// Rename across all project files (not just the specified file).
        #[arg(long)]
        project: bool,
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
    ///   sysml completions bash > ~/.local/share/bash-completion/completions/sysml-cli
    ///   sysml completions zsh > ~/.zfunc/_sysml-cli
    ///   sysml completions fish > ~/.config/fish/completions/sysml-cli.fish
    Completions {
        /// Shell: bash, zsh, fish, elvish, powershell.
        #[arg(required = true)]
        shell: String,
    },
    /// Show model statistics and metrics.
    ///
    /// Displays aggregate metrics: element counts by kind, documentation
    /// coverage, nesting depth, relationship counts, and more.
    Stats {
        /// SysML v2 files to analyze (omit to scan project).
        files: Vec<PathBuf>,
    },
    /// Analyze dependencies and impact of a model element.
    ///
    /// Shows what references a given element (reverse/impact analysis) and
    /// what the element depends on (forward analysis).
    ///
    /// EXAMPLES:
    ///   sysml deps model.sysml Engine
    ///   sysml deps model.sysml Vehicle --reverse
    ///   sysml deps model.sysml Engine --forward
    Deps {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Name of the element to analyze.
        #[arg(required = true)]
        target: String,
        /// Show only reverse dependencies (what references this element).
        #[arg(long)]
        reverse: bool,
        /// Show only forward dependencies (what this element depends on).
        #[arg(long)]
        forward: bool,
        /// Include transitive dependencies (follow chains).
        #[arg(long)]
        transitive: bool,
    },
    /// Semantic diff between two SysML v2 files.
    ///
    /// Compares model structure (not text) — reports added, removed, and
    /// changed definitions, usages, and relationships.
    ///
    /// EXAMPLES:
    ///   sysml diff old.sysml new.sysml
    ///   sysml diff -f json v1.sysml v2.sysml
    Diff {
        /// Original (old) SysML file.
        #[arg(required = true)]
        file_a: PathBuf,
        /// Modified (new) SysML file.
        #[arg(required = true)]
        file_b: PathBuf,
    },
    /// Show allocation traceability matrix.
    ///
    /// Lists logical-to-physical allocation mappings and identifies
    /// unallocated elements. In SysML v2, allocations map actions/use-cases
    /// to parts (logical to physical architecture).
    #[command(visible_alias = "alloc")]
    Allocation {
        /// SysML v2 files to analyze (omit to scan project).
        files: Vec<PathBuf>,
        /// Exit with error if unallocated elements exist (CI gate).
        #[arg(long)]
        check: bool,
        /// Show only unallocated elements.
        #[arg(long)]
        unallocated: bool,
    },
    /// Initialize a SysML project in the current directory.
    ///
    /// Creates a `.sysml/` directory with a `config.toml` file containing
    /// default project settings. Auto-detects the model root if `.sysml`
    /// files are present.
    ///
    /// EXAMPLES:
    ///   sysml init
    ///   sysml init --force
    Init {
        /// Overwrite existing `.sysml/config.toml` if present.
        #[arg(long)]
        force: bool,
    },
    /// Build or rebuild the project index (cache).
    ///
    /// Parses all SysML files under the model root and populates an
    /// in-memory cache of elements and relationships. Requires `sysml init`.
    ///
    /// EXAMPLES:
    ///   sysml index
    ///   sysml index --stats
    Index {
        /// Show index statistics.
        #[arg(long)]
        stats: bool,
    },
    /// Validate SysML v2 models.
    ///
    /// Runs all lint checks: syntax, name resolution (cross-file, with
    /// include paths), requirement coverage, naming conventions, and more.
    ///
    /// EXAMPLES:
    ///   sysml check model.sysml
    ///   sysml check --severity error model.sysml
    ///   sysml check --lint-only model.sysml
    Check {
        /// SysML v2 files to validate (omit to scan project).
        files: Vec<PathBuf>,

        /// Disable specific checks (comma-separated).
        #[arg(short, long, value_delimiter = ',')]
        disable: Vec<String>,

        /// Minimum severity to report: note, warning, error.
        #[arg(short, long, default_value = "note")]
        severity: String,
    },
    /// Model completeness and quality report.
    ///
    /// Checks documentation coverage, type completeness, requirement
    /// satisfaction/verification, and computes an overall quality score.
    /// Use --check in CI to enforce a minimum score.
    ///
    /// EXAMPLES:
    ///   sysml coverage model.sysml
    ///   sysml coverage --check --min-score 80 model.sysml
    Coverage {
        /// SysML v2 files to analyze (omit to scan project).
        files: Vec<PathBuf>,
        /// Exit with error if score is below minimum (CI gate).
        #[arg(long)]
        check: bool,
        /// Minimum acceptable score (0-100, used with --check).
        #[arg(long, default_value = "0")]
        min_score: f64,
    },
    /// Interactive REPL for exploring SysML models.
    ///
    /// Loads model files into memory and provides an interactive prompt
    /// for querying, inspecting, and computing over the model.
    ///
    /// REPL COMMANDS: list, show, find, deps, trace, rollup, sim, help, quit
    ///
    /// EXAMPLES:
    ///   sysml repl model.sysml
    ///   sysml repl
    Repl {
        /// SysML v2 files to load (omit to scan project).
        files: Vec<PathBuf>,
    },
    /// Generate documentation from model structure and comments.
    ///
    /// Produces Markdown documentation with element hierarchy,
    /// type information, and embedded doc comments.
    ///
    /// EXAMPLES:
    ///   sysml doc model.sysml
    ///   sysml doc model.sysml --root Vehicle
    Doc {
        /// SysML v2 files to document (omit to scan project).
        files: Vec<PathBuf>,
        /// Root element to start documentation from.
        #[arg(long)]
        root: Option<String>,
    },
    /// Run analysis cases defined in SysML v2 models.
    ///
    /// Lists, executes, and compares analysis cases. Supports trade studies
    /// with maximize/minimize objectives and parametric sweeps.
    ///
    /// SUBCOMMANDS: list, run, trade
    ///
    /// EXAMPLES:
    ///   sysml analyze list model.sysml
    ///   sysml analyze run model.sysml -n FuelEconomyAnalysis
    ///   sysml analyze trade model.sysml -n EngineTradeOff
    Analyze {
        #[command(subcommand)]
        kind: AnalyzeCommand,
    },
    /// Compute attribute rollups over the part hierarchy.
    ///
    /// Walks the composition tree starting from a root definition,
    /// resolves attribute values, and aggregates them. Works for any
    /// numeric attribute: mass, cost, power, tolerance, etc.
    ///
    /// SUBCOMMANDS: compute, budget, sensitivity, query
    ///
    /// EXAMPLES:
    ///   sysml rollup compute model.sysml --root Vehicle --attr mass
    ///   sysml rollup budget model.sysml --root Vehicle --attr mass --limit 2000
    ///   sysml rollup sensitivity model.sysml --root Vehicle --attr mass
    ///   sysml rollup query model.sysml --attr mass
    Rollup {
        #[command(subcommand)]
        kind: RollupCommand,
    },
}

// =========================================================================
// Subcommand enums
// =========================================================================

#[derive(Subcommand)]
pub(crate) enum SimulateCommand {
    /// Evaluate constraints and calculations with variable bindings.
    ///
    /// Evaluates SysML v2 constraint expressions (returns satisfied/violated)
    /// and calculation expressions (returns computed values).
    ///
    /// EXAMPLES:
    ///   sysml simulate eval model.sysml -b speed=100,mass=1500
    ///   sysml simulate eval model.sysml -n SpeedLimit -b speed=120
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
    ///   sysml simulate state-machine lights.sysml -e next,next,next
    ///   sysml simulate state-machine model.sysml -n TrafficLight
    ///   sysml simulate state-machine model.sysml  (interactive)
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
    ///   sysml simulate action-flow model.sysml -n ProvidePower
    ///   sysml simulate action-flow model.sysml -b fuelLevel=80
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
    /// Generate Modelica partial model stub.
    ///
    /// EXAMPLES:
    ///   sysml export modelica model.sysml -p Vehicle
    ///   sysml export modelica model.sysml -p Vehicle -o Vehicle.mo
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

#[derive(Subcommand)]
pub(crate) enum RollupCommand {
    /// Compute an attribute rollup from a root definition.
    ///
    /// Aggregates a named attribute across the part hierarchy using
    /// sum (default), RSS, product, min, or max.
    ///
    /// EXAMPLES:
    ///   sysml rollup compute model.sysml --root Vehicle --attr mass
    ///   sysml rollup compute model.sysml --root Vehicle --attr mass --method rss
    Compute {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition to start from.
        #[arg(long, required = true)]
        root: String,
        /// Attribute name to aggregate (e.g., mass, cost, power).
        #[arg(long, required = true)]
        attr: String,
        /// Aggregation method: sum, rss, product, min, max.
        #[arg(long, default_value = "sum")]
        method: String,
        /// Select a variant for a variation point: POINT=CHOICE
        /// (e.g. --variant battery=powerBattery). Repeatable. POINT is
        /// the part usage name or the variation def name; unselected
        /// variation points include all variants.
        #[arg(long = "variant", value_name = "POINT=CHOICE")]
        variant: Vec<String>,
    },
    /// Check an attribute rollup against a budget limit.
    ///
    /// Computes the rollup and exits with error if total exceeds limit.
    /// Use in CI to enforce budgets.
    ///
    /// EXAMPLES:
    ///   sysml rollup budget model.sysml --root Vehicle --attr mass --limit 2000
    Budget {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition.
        #[arg(long, required = true)]
        root: String,
        /// Attribute name.
        #[arg(long, required = true)]
        attr: String,
        /// Budget limit value.
        #[arg(long, required = true)]
        limit: f64,
        /// Aggregation method: sum, rss, product, min, max.
        #[arg(long, default_value = "sum")]
        method: String,
    },
    /// Show which children contribute most to a rollup total.
    ///
    /// EXAMPLES:
    ///   sysml rollup sensitivity model.sysml --root Vehicle --attr mass
    Sensitivity {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition.
        #[arg(long, required = true)]
        root: String,
        /// Attribute name.
        #[arg(long, required = true)]
        attr: String,
        /// Aggregation method: sum, rss, product, min, max.
        #[arg(long, default_value = "sum")]
        method: String,
    },
    /// Parametric sweep: evaluate rollup across a range of values.
    ///
    /// EXAMPLES:
    ///   sysml rollup sweep model.sysml --root Vehicle --attr mass --param engine --from 100 --to 300 --steps 5
    Sweep {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition.
        #[arg(long, required = true)]
        root: String,
        /// Attribute name.
        #[arg(long, required = true)]
        attr: String,
        /// Parameter to sweep (dotted path, e.g., "engine" or "engine.mass").
        #[arg(long, required = true)]
        param: String,
        /// Start value for sweep.
        #[arg(long, required = true)]
        from: f64,
        /// End value for sweep.
        #[arg(long, required = true)]
        to: f64,
        /// Number of steps.
        #[arg(long, default_value = "10")]
        steps: usize,
        /// Aggregation method: sum, rss, product, min, max.
        #[arg(long, default_value = "sum")]
        method: String,
    },
    /// What-if analysis: compare rollup under different scenarios.
    ///
    /// EXAMPLES:
    ///   sysml rollup what-if model.sysml --root Vehicle --attr mass --scenario "light:engine=100" --scenario "heavy:engine=300"
    WhatIf {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition.
        #[arg(long, required = true)]
        root: String,
        /// Attribute name.
        #[arg(long, required = true)]
        attr: String,
        /// Scenarios as "name:path=value,path=value" (repeatable).
        #[arg(long = "scenario")]
        scenarios: Vec<String>,
        /// Aggregation method: sum, rss, product, min, max.
        #[arg(long, default_value = "sum")]
        method: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum AnalyzeCommand {
    /// List analysis cases found in model files.
    ///
    /// EXAMPLES:
    ///   sysml analyze list model.sysml
    List {
        /// SysML v2 files to inspect.
        files: Vec<PathBuf>,
    },
    /// Execute an analysis case with the model's current values.
    ///
    /// Classic analysis cases bind the subject and evaluate the return
    /// expression. Cases whose type specializes
    /// Uncertainty::UncertaintyAnalysis (e.g. Tolerancing::ToleranceStackup
    /// from the domain libraries) run uncertainty propagation instead:
    /// worst-case interval arithmetic, RSS variance propagation
    /// (Cp/Cpk, sensitivity, Bender mean shift), and seeded Monte Carlo.
    ///
    /// EXAMPLES:
    ///   sysml analyze run model.sysml -n FuelEconomyAnalysis
    ///   sysml analyze run model.sysml -n MassAnalysis -b mass=500
    ///   sysml analyze run -I libraries model.sysml -n gapAnalysis
    ///   sysml analyze run -I libraries model.sysml -n gapAnalysis \
    ///       --method monte-carlo --iterations 50000 --seed 12345
    Run {
        /// SysML v2 files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Analysis case name.
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// Variable bindings: name=value.
        #[arg(short = 'b', long = "bind", value_delimiter = ',')]
        bindings: Vec<String>,
        /// Uncertainty method: worst-case, rss, monte-carlo, or all
        /// (only for cases typed by Uncertainty::UncertaintyAnalysis).
        #[arg(long)]
        method: Option<String>,
        /// Monte Carlo iteration count (overrides the model's setting).
        #[arg(long)]
        iterations: Option<u64>,
        /// Monte Carlo seed for bit-for-bit reproducible runs
        /// (overrides the model's setting; recorded in the output).
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Compare alternatives in a trade study analysis case.
    ///
    /// EXAMPLES:
    ///   sysml analyze trade model.sysml -n EngineTradeOff
    Trade {
        /// SysML v2 files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Analysis case name.
        #[arg(short = 'n', long)]
        name: Option<String>,
    },
}
