/// sysml-cli: SysML v2 command-line tool for validation, simulation,
/// diagram generation, and model management.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;
mod model_writer;
mod output;
mod records;
mod wizard;

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
  Validate a model:       sysml lint model.sysml
  List model elements:    sysml list --kind parts model.sysml
  Show element details:   sysml show model.sysml Vehicle
  Generate a diagram:     sysml diagram -t bdd -o mermaid model.sysml
  Simulate a state machine: sysml simulate state-machine model.sysml
  Add to a model:         sysml add model.sysml part-def Vehicle --doc 'A vehicle'
  Interactive wizard:     sysml add
  Remove from a model:    sysml remove model.sysml Engine
  Format a file:          sysml fmt model.sysml
  Export to FMI:          sysml export interfaces model.sysml --part MyPart

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

        /// Diagram type: bdd, ibd, stm, act, req, pkg, par.
        #[arg(short = 't', long = "type", required = true,
              value_parser = ["bdd", "ibd", "stm", "act", "req", "pkg", "par"],
              help_heading = "Diagram")]
        diagram_type: String,

        /// Output format: mermaid, plantuml, dot, d2 (and aliases).
        #[arg(short = 'o', long = "output-format", default_value = "mermaid",
              value_parser = ["mermaid", "mmd", "plantuml", "puml", "dot", "graphviz", "d2", "terrastruct"])]
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

        /// Add members (repeatable). Format: "[direction] kind name[:type]".
        #[arg(long = "member", short = 'm')]
        members: Vec<String>,

        /// (view-def only) Expose clause.
        #[arg(long = "expose")]
        exposes: Vec<String>,

        /// (view-def only) Filter by element kind.
        #[arg(long)]
        filter: Option<String>,

        /// Launch interactive wizard even when args are provided.
        #[arg(short = 'i', long)]
        interactive: bool,
    },
    /// Remove a named element from a SysML file.
    ///
    /// Removes the element and its body from the file.
    ///
    /// EXAMPLES:
    ///   sysml remove model.sysml Engine
    ///   sysml remove model.sysml Engine --dry-run
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
        /// Target SysML file.
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
    /// Show model statistics and metrics.
    ///
    /// Displays aggregate metrics: element counts by kind, documentation
    /// coverage, nesting depth, relationship counts, and more.
    Stats {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Analyze dependencies and impact of a model element.
    ///
    /// Shows what references a given element (reverse/impact analysis) and
    /// what the element depends on (forward analysis).
    ///
    /// EXAMPLES:
    ///   sysml-cli deps model.sysml Engine
    ///   sysml-cli deps model.sysml Vehicle --reverse
    ///   sysml-cli deps model.sysml Engine --forward
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
    },
    /// Semantic diff between two SysML v2 files.
    ///
    /// Compares model structure (not text) — reports added, removed, and
    /// changed definitions, usages, and relationships.
    ///
    /// EXAMPLES:
    ///   sysml-cli diff old.sysml new.sysml
    ///   sysml-cli diff -f json v1.sysml v2.sysml
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
    Allocation {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
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
        /// Rebuild everything including records (default).
        #[arg(long, default_value = "true")]
        full: bool,

        /// Show index statistics.
        #[arg(long)]
        stats: bool,
    },
    /// Validate SysML v2 models and check project integrity.
    ///
    /// Runs all lint checks plus optional project-level checks (broken
    /// record references, orphaned records). Use `sysml lint` as a
    /// shortcut for `sysml check --lint-only`.
    ///
    /// EXAMPLES:
    ///   sysml check model.sysml
    ///   sysml check --lint-only model.sysml
    Check {
        /// SysML v2 files to validate.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Disable specific checks (comma-separated).
        #[arg(short, long, value_delimiter = ',')]
        disable: Vec<String>,

        /// Minimum severity to report: note, warning, error.
        #[arg(short, long, default_value = "note")]
        severity: String,

        /// Run only lint checks (no record or project checks).
        #[arg(long)]
        lint_only: bool,
    },
    /// Verification domain commands.
    ///
    /// Manage verification case execution, coverage analysis, and
    /// test record tracking for SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml verify coverage model.sysml
    ///   sysml verify list model.sysml
    Verify {
        #[command(subcommand)]
        kind: VerifyCommand,
    },
    /// Generate a complete example project with teaching comments.
    ///
    /// Creates a set of SysML v2 files forming a working example project
    /// with parts, requirements, and verification cases.
    ///
    /// EXAMPLES:
    ///   sysml example brake-system
    ///   sysml example sensor-module -o ./myproject
    ///   sysml example --list
    Example {
        /// Example name (omit with --list to see available examples).
        name: Option<String>,

        /// Output directory (default: current directory).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// List available example projects.
        #[arg(long)]
        list: bool,
    },
    /// Risk management commands.
    ///
    /// Identify, assess, and track risks. Generate risk matrices and
    /// FMEA worksheets from SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml risk list model.sysml
    ///   sysml risk matrix model.sysml
    ///   sysml risk fmea model.sysml
    Risk {
        #[command(subcommand)]
        kind: RiskCommand,
    },
    /// Tolerance analysis commands.
    ///
    /// Perform worst-case, RSS, and Monte Carlo tolerance stack-up
    /// analysis on dimension chains defined in SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml tol analyze model.sysml
    ///   sysml tol sensitivity model.sysml
    Tol {
        #[command(subcommand)]
        kind: TolCommand,
    },
    /// Bill of materials commands.
    ///
    /// Build hierarchical BOM trees, perform mass/cost rollups,
    /// where-used queries, and CSV export from SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml bom rollup model.sysml --root Vehicle
    ///   sysml bom where-used model.sysml --part Engine
    ///   sysml bom export model.sysml --root Vehicle
    Bom {
        #[command(subcommand)]
        kind: BomCommand,
    },
    /// Supplier management commands.
    ///
    /// List suppliers, view approved source lists, and generate
    /// request-for-quotation documents from SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml source list model.sysml
    ///   sysml source asl model.sysml
    ///   sysml source rfq --part Resistor --quantity 5000
    Source {
        #[command(subcommand)]
        kind: SourceCommand,
    },
    /// Manufacturing execution commands.
    ///
    /// List manufacturing routings extracted from action definitions
    /// and compute SPC statistics on process parameter readings.
    ///
    /// EXAMPLES:
    ///   sysml mfg list model.sysml
    ///   sysml mfg spc --parameter Diameter --values 10.01,10.02,9.99,10.00
    Mfg {
        #[command(subcommand)]
        kind: MfgCommand,
    },
    /// Quality control commands.
    ///
    /// ANSI Z1.4 sample size lookup and process capability (Cp/Cpk)
    /// analysis for manufacturing quality control.
    ///
    /// EXAMPLES:
    ///   sysml qc sample-size --lot-size 500
    ///   sysml qc capability --usl 10.05 --lsl 9.95 --values 10.01,10.02,9.99
    Qc {
        #[command(subcommand)]
        kind: QcCommand,
    },
    /// Quality management commands (NCR, CAPA, Process Deviation).
    ///
    /// Create and manage nonconformance reports (NCRs), corrective/preventive
    /// action programs (CAPAs), and process deviations — each as distinct
    /// quality items with their own lifecycle.
    ///
    /// EXAMPLES:
    ///   sysml quality trend model.sysml
    ///   sysml quality list
    Quality {
        #[command(subcommand)]
        kind: QualityCommand,
    },
    /// Cross-domain reporting commands.
    ///
    /// Generate project dashboards, full lifecycle traceability
    /// threads, and gate readiness checks from SysML v2 models.
    ///
    /// EXAMPLES:
    ///   sysml report dashboard model.sysml
    ///   sysml report traceability model.sysml --requirement BrakeReq
    ///   sysml report gate model.sysml --gate-name CDR
    Report {
        #[command(subcommand)]
        kind: ReportCommand,
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
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Exit with error if score is below minimum (CI gate).
        #[arg(long)]
        check: bool,
        /// Minimum acceptable score (0-100, used with --check).
        #[arg(long, default_value = "0")]
        min_score: f64,
    },
    /// Read a help topic about SysML or this tool.
    ///
    /// Displays concise tutorials and reference material for engineers
    /// who are new to SysML v2 or model-based systems engineering.
    ///
    /// EXAMPLES:
    ///   sysml guide                    List available topics
    ///   sysml guide getting-started    Tutorial for first-time users
    ///   sysml guide sysml-basics       SysML v2 language overview
    Guide {
        /// Topic to display (omit to list all topics).
        topic: Option<String>,
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

#[derive(Subcommand)]
pub(crate) enum VerifyCommand {
    /// Show verification coverage for requirements.
    ///
    /// Combines model traceability (verify relationships) with execution
    /// records to show which requirements have been verified and passed.
    Coverage {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List verification cases found in model files.
    List {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Show verification status for all requirements.
    Status {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}


#[derive(Subcommand)]
pub(crate) enum RiskCommand {
    /// List risks found in model files.
    List {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Generate a risk matrix from model risks.
    Matrix {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Generate an FMEA worksheet from model risks.
    Fmea {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum TolCommand {
    /// Run tolerance stack-up analysis on dimension chains.
    Analyze {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Analysis method: worst-case, rss, monte-carlo.
        #[arg(long, default_value = "worst-case")]
        method: String,

        /// Number of Monte Carlo iterations.
        #[arg(long, default_value = "10000")]
        iterations: usize,
    },
    /// Rank tolerance contributors by sensitivity.
    Sensitivity {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum BomCommand {
    /// Build a hierarchical BOM tree with optional mass/cost rollup.
    Rollup {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition name.
        #[arg(long, required = true)]
        root: String,
        /// Include mass rollup in output.
        #[arg(long)]
        include_mass: bool,
        /// Include cost rollup in output.
        #[arg(long)]
        include_cost: bool,
    },
    /// Find all definitions that use a given part (reverse lookup).
    WhereUsed {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Part definition name to search for.
        #[arg(long, required = true)]
        part: String,
    },
    /// Export a flattened BOM as CSV.
    Export {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Root part definition name.
        #[arg(long, required = true)]
        root: String,
        /// Output format (csv).
        #[arg(long, default_value = "csv")]
        format: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum SourceCommand {
    /// List suppliers extracted from model files.
    List {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Show approved source list (approved/preferred suppliers only).
    Asl {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Generate a request-for-quotation (RFQ) document.
    Rfq {
        /// Part name.
        #[arg(long, required = true)]
        part: String,
        /// Part description.
        #[arg(long, default_value = "")]
        description: String,
        /// Required quantity.
        #[arg(long, default_value = "1")]
        quantity: u32,
    },
}

#[derive(Subcommand)]
pub(crate) enum MfgCommand {
    /// List manufacturing routings (action definitions) in model files.
    List {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Compute SPC statistics for a parameter from readings.
    Spc {
        /// Parameter name.
        #[arg(long, required = true)]
        parameter: String,
        /// Comma-separated measurement values.
        #[arg(long, required = true, value_delimiter = ',')]
        values: Vec<f64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum QcCommand {
    /// Look up ANSI Z1.4 sample size for a given lot.
    SampleSize {
        /// Lot size (total number of units).
        #[arg(long, required = true)]
        lot_size: usize,
        /// Acceptable quality level (percent defective).
        #[arg(long, default_value = "1.0")]
        aql: f64,
        /// Inspection level: reduced, normal, tightened.
        #[arg(long, default_value = "normal")]
        level: String,
    },
    /// Compute process capability indices (Cp/Cpk).
    Capability {
        /// Upper specification limit.
        #[arg(long, required = true)]
        usl: f64,
        /// Lower specification limit.
        #[arg(long, required = true)]
        lsl: f64,
        /// Comma-separated measurement values.
        #[arg(long, required = true, value_delimiter = ',')]
        values: Vec<f64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum QualityCommand {
    /// Analyze NCR trends grouped by category or severity.
    Trend {
        /// SysML v2 files to correlate with NCR data.
        files: Vec<PathBuf>,
        /// Grouping dimension: category, severity, part, supplier.
        #[arg(long, default_value = "category")]
        group_by: String,
    },
    /// Show quality item status overview and workflow guidance.
    List,
}

#[derive(Subcommand)]
pub(crate) enum ReportCommand {
    /// Generate a project dashboard from model files.
    Dashboard {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Trace a requirement through satisfaction, verification, and execution.
    Traceability {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Requirement name to trace.
        #[arg(long, required = true)]
        requirement: String,
    },
    /// Check gate readiness (coverage, risks, NCRs).
    Gate {
        /// SysML v2 files to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Gate name (e.g. PDR, CDR, FRR).
        #[arg(long, required = true)]
        gate_name: String,
        /// Minimum verification coverage percentage required.
        #[arg(long, default_value = "80.0")]
        min_coverage: f64,
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
        Command::Stats { files } => commands::stats::run(&cli, files),
        Command::Deps { files, target, reverse, forward } => commands::deps::run(&cli, files, target, *reverse, *forward),
        Command::Diff { file_a, file_b } => commands::diff::run(&cli, file_a, file_b),
        Command::Allocation { files, check, unallocated } => commands::allocation::run(&cli, files, *check, *unallocated),
        Command::Coverage { files, check, min_score } => commands::coverage::run(&cli, files, *check, *min_score),
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
    generate(shell, &mut cmd, "sysml", &mut std::io::stdout());
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
