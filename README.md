# sysml

A fast, standalone SysML v2 command-line toolchain for model validation, simulation, diagram generation, and full product lifecycle management.

Built on [tree-sitter](https://tree-sitter.github.io/) for reliable parsing of SysML v2 textual notation. Zero runtime dependencies — just a single binary.

## Installation

### From source

```sh
git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git
cd sysml-cli
cargo install --path crates/sysml-cli
```

Or build manually:

```sh
cargo build --release
cp target/release/sysml ~/.local/bin/
```

The build compiles the [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar from source (included as a submodule). Requires Rust 1.70+ and a C compiler (gcc or clang).

To enable the optional SQLite-backed persistent cache:

```sh
cargo install --path crates/sysml-cli --features sqlite
```

### Shell completions

```sh
sysml completions bash > ~/.local/share/bash-completion/completions/sysml
sysml completions zsh > ~/.zfunc/_sysml
sysml completions fish > ~/.config/fish/completions/sysml.fish
```

## Quick Start

```sh
sysml lint model.sysml                          # Validate a model
sysml list model.sysml                          # List all elements
sysml show model.sysml Vehicle                  # Element details
sysml diagram -t bdd model.sysml                # Block definition diagram
sysml diagram -t trace model.sysml              # Traceability diagram
sysml simulate state-machine model.sysml        # Interactive state machine
sysml fmt model.sysml                           # Format source
sysml add --stdout part-def Vehicle             # Generate template to stdout
sysml add model.sysml part-def Engine           # Insert into file
sysml add                                       # Interactive wizard
sysml init                                      # Initialize a project
sysml risk matrix model.sysml                   # Risk matrix
sysml tol analyze model.sysml --method rss      # Tolerance stack-up
sysml bom rollup model.sysml --root Vehicle     # BOM rollup
sysml quality create --type ncr                 # Create NCR interactively
sysml report dashboard model.sysml              # Project health
```

### Global Options

| Flag | Description |
|------|-------------|
| `-f, --format <FORMAT>` | Output format: `text`, `json` (default: `text`) |
| `-q, --quiet` | Suppress summary line on stderr |
| `-I, --include <PATH>` | Additional files/directories for import resolution |
| `--stdlib-path <PATH>` | Path to the SysML v2 standard library directory (env: `SYSML_STDLIB_PATH`, config: `stdlib_path`) |

## Commands

| Command | Description | Docs |
|---------|-------------|------|
| **Analysis** | | [analysis](docs/commands/analysis.md) |
| `lint` | Validate SysML v2 files against structural rules | |
| `list` (`ls`) | List model elements with filters | |
| `show` | Show detailed element information | |
| `check` | Validate models and project integrity | |
| `trace` | Requirements traceability matrix | |
| `interfaces` | Analyze port interfaces and connections | |
| `deps` | Dependency analysis for an element | |
| `diff` | Semantic diff between two SysML files | |
| `allocation` | Logical-to-physical allocation matrix | |
| `coverage` | Model quality and completeness report | |
| `stats` | Aggregate model statistics | |
| **Diagrams** | | [diagrams](docs/commands/diagrams.md) |
| `diagram` | Generate diagrams (bdd, ibd, stm, act, req, pkg, par, trace, alloc, ucd) | |
| **Editing** | | [editing](docs/commands/editing.md) |
| `add` | Add elements interactively, to a file, or to stdout | |
| `remove` | Remove an element from a SysML file | |
| `rename` | Rename an element and update all references | |
| `example` | Generate example projects with teaching comments | |
| `fmt` | Format SysML v2 source files | |
| **Simulation & Export** | | [simulation](docs/commands/simulation.md) |
| `simulate` | Evaluate constraints, state machines, action flows | |
| `export` | Export FMI 3.0, Modelica, SSP artifacts | |
| **Lifecycle** | | [lifecycle](docs/commands/lifecycle.md) |
| `verify` | Verification case management, coverage, interactive execution | |
| `risk` | Risk management, matrix, FMEA, interactive risk creation | |
| `tol` | Tolerance stack-up analysis (worst-case, RSS, Monte Carlo) | |
| `bom` | Bill of materials rollup, where-used, export | |
| `source` | Supplier management, RFQ, approved source lists | |
| `mfg` | Manufacturing routings, SPC, lot tracking, step execution | |
| `qc` | Quality control, sampling plans, Cp/Cpk | |
| `quality` | Quality management (NCR, CAPA, Process Deviation, RCA) | |
| **Project** | | [project](docs/commands/project.md) |
| `init` | Initialize a `.sysml/` project | |
| `index` | Build or rebuild project index | |
| `pipeline` | Run named validation pipelines from config | |
| `report` | Cross-domain reports (dashboard, traceability, gate) | |
| `guide` | Built-in help topics and tutorials | |
| `completions` | Generate shell completion scripts | |

## Domain Libraries

The tool ships with SysML v2 domain libraries that provide base types for lifecycle workflows. Users specialize these types in their models — the tool recognizes all specializations automatically.

| Library | Package | Purpose |
|---------|---------|---------|
| `sysml-verification-ext.sysml` | `SysMLVerification` | Verification status, methods, acceptance criteria |
| `sysml-risk.sysml` | `SysMLRisk` | Severity/likelihood enums, RiskDef, MitigationDef |
| `sysml-tolerance.sysml` | `SysMLTolerance` | ToleranceDef, DimensionChainDef, GD&T types |
| `sysml-bom.sysml` | `SysMLBOM` | PartIdentity, MassProperty, SupplierDef |
| `sysml-manufacturing.sysml` | `SysMLManufacturing` | ProcessDef, RoutingDef, WorkInstructionDef |
| `sysml-quality.sysml` | `SysMLQuality` | InspectionPlanDef, GaugeRRDef, sampling |
| `sysml-capa.sysml` | `SysMLCAPA` | NCR/CAPA/Deviation lifecycles, categories, dispositions |
| `sysml-project.sysml` | `SysMLProject` | Phase gates, milestone definitions |

See [Domain Libraries](docs/domain-libraries.md) for detailed type references and usage patterns.

## Further Documentation

| Document | Contents |
|----------|----------|
| [Validation & Diagnostics](docs/validation.md) | 9 lint checks, diagnostic codes, output formats |
| [CI & Editor Integration](docs/ci-integration.md) | GitHub Actions workflow, Emacs sysml2-mode, JSON output |
| [Architecture](docs/architecture.md) | Crate structure, design decisions, 12-crate workspace |

## License

GPL-3.0-or-later
