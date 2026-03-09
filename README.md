# sysml-cli

A fast, standalone SysML v2 command-line tool for validation, simulation, diagram generation, and model management.

Built on [tree-sitter](https://tree-sitter.github.io/) for reliable parsing of SysML v2 textual notation. Zero runtime dependencies — just a single binary.

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Commands](#commands)
  - [lint](#lint)
  - [list](#list)
  - [show](#show)
  - [diagram](#diagram)
  - [simulate](#simulate)
  - [trace](#trace)
  - [interfaces](#interfaces)
  - [new](#new)
  - [edit](#edit)
  - [stats](#stats)
  - [deps](#deps)
  - [diff](#diff)
  - [allocation](#allocation)
  - [coverage](#coverage)
  - [fmt](#fmt)
  - [export](#export)
  - [completions](#completions)
- [Validation Checks](#validation-checks)
- [Diagnostic Codes](#diagnostic-codes)
- [Output Formats](#output-formats)
- [CI Integration](#ci-integration)
- [Editor Integration](#editor-integration)
- [Building from Source](#building-from-source)
- [Architecture](#architecture)
- [License](#license)

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
cp target/release/sysml-cli ~/.local/bin/
```

The build compiles the [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar from source (included as a submodule).

### Shell completions

```sh
sysml-cli completions bash > ~/.local/share/bash-completion/completions/sysml-cli
sysml-cli completions zsh > ~/.zfunc/_sysml-cli
sysml-cli completions fish > ~/.config/fish/completions/sysml-cli.fish
```

## Quick Start

```sh
# Validate a SysML v2 model
sysml-cli lint model.sysml

# List all definitions and usages
sysml-cli list model.sysml

# Show details of a specific element
sysml-cli show model.sysml Vehicle

# Generate a block definition diagram (Mermaid)
sysml-cli diagram -t bdd model.sysml

# Simulate a state machine interactively
sysml-cli simulate state-machine model.sysml

# Generate a new part definition
sysml-cli new part-def Vehicle --doc "A vehicle" -m "part engine:Engine"

# Add a part usage to an existing file
sysml-cli edit add model.sysml part engine -t Engine

# Model statistics
sysml-cli stats model.sysml

# Dependency analysis
sysml-cli deps model.sysml Vehicle

# Model quality coverage report
sysml-cli coverage model.sysml

# Format a file
sysml-cli fmt model.sysml
```

### Global Options

| Flag | Description |
|------|-------------|
| `-f, --format <FORMAT>` | Output format: `text`, `json` (default: `text`) |
| `-q, --quiet` | Suppress summary line on stderr |
| `-I, --include <PATH>` | Additional files/directories for import resolution |

## Commands

### lint

Validate SysML v2 files against structural rules.

```sh
sysml-cli lint model.sysml
sysml-cli lint src/*.sysml                       # Multiple files
sysml-cli lint model.sysml -I lib/               # Include imports
sysml-cli lint -f json model.sysml               # JSON output
sysml-cli lint --severity warning model.sysml    # Only warnings+
sysml-cli lint --disable unused,unresolved model.sysml
```

| Option | Description |
|--------|-------------|
| `-d, --disable <CHECKS>` | Disable checks (comma-separated). See [Validation Checks](#validation-checks). |
| `-s, --severity <LEVEL>` | Minimum severity: `note`, `warning`, `error` (default: `note`) |

Exit codes: `0` = no errors, `1` = errors found.

### list

List model elements with optional filters. Alias: `ls`.

```sh
sysml-cli list model.sysml
sysml-cli list --kind parts model.sysml          # Only part definitions
sysml-cli list --kind port model.sysml           # Only port usages
sysml-cli list --name Vehicle model.sysml        # Name search
sysml-cli list --parent Vehicle model.sysml      # Children of Vehicle
sysml-cli list --unused model.sysml              # Unreferenced defs
sysml-cli list -f json model.sysml               # JSON output
```

| Option | Description |
|--------|-------------|
| `-k, --kind <KIND>` | Filter: `parts`, `ports`, `actions`, `states`, `requirements`, `constraints`, `all`, `definitions`, `usages` |
| `-n, --name <PATTERN>` | Substring name filter |
| `-p, --parent <NAME>` | Filter by parent definition |
| `--unused` | Show only unreferenced definitions |
| `--abstract` | Show only abstract definitions |
| `--visibility <VIS>` | Filter by `public`, `private`, `protected` |
| `--view <NAME>` | Apply a SysML v2 view definition as a filter preset |

### show

Show detailed information about a specific element.

```sh
sysml-cli show model.sysml Vehicle
sysml-cli show -f json model.sysml Engine
```

Displays: kind, visibility, parent, documentation, type, children, relationships.

### diagram

Generate diagrams in Mermaid, PlantUML, DOT, or D2 format.

```sh
sysml-cli diagram -t bdd model.sysml                    # Block definition
sysml-cli diagram -t ibd -s Vehicle model.sysml          # Internal block
sysml-cli diagram -t stm model.sysml                     # State machine
sysml-cli diagram -t act model.sysml                     # Activity
sysml-cli diagram -t req model.sysml                     # Requirements
sysml-cli diagram -t pkg model.sysml                     # Package
sysml-cli diagram -t par model.sysml                     # Parametric
sysml-cli diagram -t bdd -o plantuml model.sysml         # PlantUML output
sysml-cli diagram -t bdd -o dot model.sysml              # Graphviz DOT
sysml-cli diagram -t bdd -o d2 model.sysml               # D2
sysml-cli diagram -t bdd -d LR --depth 2 model.sysml    # Layout + depth
```

**Diagram types:**

| Type | Description |
|------|-------------|
| `bdd` | Block Definition Diagram — definitions and relationships |
| `ibd` | Internal Block Diagram — internal structure of a part |
| `stm` | State Machine Diagram — states and transitions |
| `act` | Activity Diagram — action flow with decisions and forks |
| `req` | Requirements Diagram — requirements and trace status |
| `pkg` | Package Diagram — packages and containment hierarchy |
| `par` | Parametric Diagram — constraints and parameters |

**Output formats:** `mermaid` (default), `plantuml`, `dot`, `d2`

| Option | Description |
|--------|-------------|
| `-t, --type <TYPE>` | Diagram type (required) |
| `-o, --output-format <FMT>` | Output format (default: `mermaid`) |
| `-s, --scope <NAME>` | Focus on a specific definition |
| `-d, --direction <DIR>` | Layout: `TB`, `LR`, `BT`, `RL` |
| `--depth <N>` | Maximum nesting depth |

### simulate

Run simulations on SysML v2 models: evaluate constraints, simulate state machines, or execute action flows.

```sh
sysml-cli simulate list model.sysml              # Discover simulatable items
```

#### simulate eval

Evaluate constraints and calculations with variable bindings.

```sh
sysml-cli simulate eval model.sysml -b speed=100
sysml-cli simulate eval model.sysml -n SpeedLimit -b speed=120
sysml-cli simulate eval model.sysml -b mass=1500,velocity=30 -n KineticEnergy
```

| Option | Description |
|--------|-------------|
| `-b, --bind <BINDINGS>` | Variable bindings: `name=value` (comma-separated) |
| `-n, --name <NAME>` | Evaluate only this constraint or calculation |

#### simulate state-machine

Simulate a state machine step-by-step. Alias: `sm`.

Supports `state def`, `exhibit state` (inside part definitions), and nested state regions (parallel orthogonal states). If `--events` is omitted and the machine has signal triggers, you will be prompted interactively.

```sh
sysml-cli simulate state-machine model.sysml -n TrafficLight -e next,next
sysml-cli simulate sm model.sysml -n Controller -b temperature=150
sysml-cli simulate sm model.sysml    # Interactive event selection
```

| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | State machine name (prompted if omitted) |
| `-e, --events <EVENTS>` | Events to inject (comma-separated signal names) |
| `-m, --max-steps <N>` | Max simulation steps (default: 100) |
| `-b, --bind <BINDINGS>` | Variable bindings for guard expressions |

#### simulate action-flow

Execute an action flow step-by-step. Alias: `af`.

Walks through perform steps, decisions, forks/joins, accept/send actions, loops, and merge/terminate nodes, producing an execution trace.

```sh
sysml-cli simulate action-flow model.sysml -n ProcessOrder
sysml-cli simulate af model.sysml -b fuelLevel=80
```

| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | Action name (prompted if omitted) |
| `-m, --max-steps <N>` | Max execution steps (default: 1000) |
| `-b, --bind <BINDINGS>` | Variable bindings for conditionals |

### trace

Generate a requirements traceability matrix.

```sh
sysml-cli trace model.sysml
sysml-cli trace --check --min-coverage 80 model.sysml    # CI gate
sysml-cli trace -f json model.sysml
```

| Option | Description |
|--------|-------------|
| `--check` | Exit with error if requirements lack satisfaction/verification |
| `--min-coverage <PCT>` | Minimum coverage percentage (with `--check`) |

### interfaces

Analyze port interfaces and identify unconnected ports.

```sh
sysml-cli interfaces model.sysml
sysml-cli interfaces --unconnected model.sysml
```

### new

Generate a SysML v2 definition template to stdout. Use this as a starting point or pipe into a file.

The `new` command creates template text — it does not modify existing files. To add elements to existing files, use [`edit add`](#edit).

```sh
sysml-cli new part-def Vehicle
sysml-cli new part-def Vehicle --extends Base --doc "A vehicle"
sysml-cli new part-def Vehicle -m "part engine:Engine" -m "part wheels:Wheel"
sysml-cli new port-def FuelPort -m "in item fuel:FuelType"
sysml-cli new view-def PartsView --expose "Vehicle::*" --filter part
sysml-cli new constraint-def SpeedLimit -m "in speed:Real"
sysml-cli new package VehiclePkg
```

**Available kinds:** `part-def`, `port-def`, `action-def`, `state-def`, `constraint-def`, `calc-def`, `requirement` (`req`), `enum-def`, `attribute-def` (`attr`), `item-def`, `view-def`, `viewpoint-def`, `package` (`pkg`), `use-case`, `connection-def`, `interface-def`, `flow-def`, `allocation-def`

| Option | Description |
|--------|-------------|
| `--extends <TYPE>` | Specialization supertype (`:>` syntax) |
| `--abstract` | Mark as abstract |
| `--short-name <ALIAS>` | Short name (`<alias>` before the name) |
| `--doc <TEXT>` | Documentation comment (`doc /* text */`) |
| `-m, --member <SPEC>` | Add member (repeatable): `"[dir] kind name[:type]"` |
| `--expose <PATTERN>` | (view-def) Expose clause: `"Vehicle::*"` |
| `--filter <KIND>` | (view-def) Filter by element kind |

### edit

Surgically modify SysML v2 files using CST-aware byte-accurate positions.

#### edit add

Add a definition or usage to an existing file. For usage-level elements (`part`, `port`, etc.), automatically inserts inside an existing definition body.

```sh
sysml-cli edit add model.sysml part engine -t Engine
sysml-cli edit add model.sysml port fuelIn -t FuelPort --inside Vehicle
sysml-cli edit add model.sysml part-def Wheel --dry-run
sysml-cli edit add model.sysml attribute mass -t Real --inside Vehicle
```

| Option | Description |
|--------|-------------|
| `-t, --type-ref <TYPE>` | Type reference (`: Type` for usages, `:>` for defs) |
| `--inside <NAME>` | Insert inside this definition (auto-detected for usages) |
| `--doc <TEXT>` | Documentation comment |
| `--extends <TYPE>` | Specialization supertype (definition kinds) |
| `--abstract` | Mark as abstract (definition kinds) |
| `--short-name <ALIAS>` | Short name alias |
| `-m, --member <SPEC>` | Add members (definition kinds) |
| `--dry-run` | Preview as unified diff |

#### edit remove

Remove an element by name.

```sh
sysml-cli edit remove model.sysml Engine --dry-run
sysml-cli edit remove model.sysml Engine
```

#### edit rename

Rename an element and update all references.

```sh
sysml-cli edit rename model.sysml Engine Motor --dry-run
sysml-cli edit rename model.sysml Engine Motor
```

### stats

Show aggregate model statistics: element counts by kind, relationship counts, documentation coverage, and nesting depth.

```sh
sysml-cli stats model.sysml
sysml-cli stats -f json model.sysml              # JSON output
sysml-cli stats src/*.sysml                       # Multiple files
```

Output includes definitions/usages by kind, connection/flow/satisfaction/verification/allocation counts, package count, abstract definitions, import count, max nesting depth, and documentation coverage percentage.

### deps

Analyze dependencies for a specific element — what it depends on and what references it.

```sh
sysml-cli deps model.sysml Vehicle
sysml-cli deps model.sysml Engine --reverse       # Only show "referenced by"
sysml-cli deps model.sysml Engine --forward       # Only show "depends on"
sysml-cli deps -f json model.sysml Vehicle
```

| Option | Description |
|--------|-------------|
| `--reverse` | Show only reverse dependencies (what references this element) |
| `--forward` | Show only forward dependencies (what this element depends on) |

### diff

Compare two SysML files and report semantic differences (added/removed/changed definitions, usages, connections).

```sh
sysml-cli diff old.sysml new.sysml
sysml-cli diff -f json v1.sysml v2.sysml
```

Unlike text-based diff, this compares at the model level — detecting renamed types, changed members, and structural modifications regardless of formatting changes.

### allocation

Display the logical-to-physical allocation matrix. In SysML v2, allocations map actions and use-cases to parts.

```sh
sysml-cli allocation model.sysml
sysml-cli allocation --unallocated model.sysml    # Only show gaps
sysml-cli allocation --check model.sysml          # CI: exit 1 if gaps exist
sysml-cli allocation -f json model.sysml
```

| Option | Description |
|--------|-------------|
| `--check` | Exit with error if unallocated elements exist |
| `--unallocated` | Show only unallocated elements |

### coverage

Generate a model quality report: documentation coverage, typed usages, populated definitions, requirement satisfaction/verification, and an overall score.

```sh
sysml-cli coverage model.sysml
sysml-cli coverage --check --min-score 80 model.sysml    # CI gate
sysml-cli coverage -f json model.sysml
```

| Option | Description |
|--------|-------------|
| `--check` | Exit with error if score is below minimum |
| `--min-score <PCT>` | Minimum overall score percentage (default: 0, used with `--check`) |

**Reported metrics:**

| Metric | Description |
|--------|-------------|
| Documentation | Percentage of definitions with doc comments |
| Typed usages | Percentage of usages with explicit type references |
| Populated defs | Percentage of definitions with at least one member |
| Req satisfaction | Percentage of requirements with a satisfy statement |
| Req verification | Percentage of requirements with a verify statement |
| Overall score | Weighted average of all metrics |

### fmt

Format SysML v2 files. CST-aware indentation that handles nested definitions, comments, and state machines correctly.

```sh
sysml-cli fmt model.sysml
sysml-cli fmt --check model.sysml         # CI: exit 1 if unformatted
sysml-cli fmt --diff model.sysml          # Show diff without writing
sysml-cli fmt --indent-width 2 model.sysml
```

### export

Export FMI/SSP artifacts from SysML models.

```sh
sysml-cli export list model.sysml                              # List exportable parts
sysml-cli export interfaces model.sysml --part Engine           # FMI 3.0 interfaces
sysml-cli export modelica model.sysml --part Engine             # Modelica stub
sysml-cli export modelica model.sysml --part Engine -o Engine.mo
sysml-cli export ssp model.sysml                                # SSP XML
sysml-cli export ssp model.sysml -o system.ssd
```

The FMI extraction handles port definitions with `in item`/`out item`, conjugation (`~`), SysML-to-FMI type mapping (`Real` -> `Float64`, `Integer` -> `Int32`, etc.), and attributes as parameters.

### completions

Generate shell completion scripts.

```sh
sysml-cli completions bash
sysml-cli completions zsh
sysml-cli completions fish
sysml-cli completions elvish
sysml-cli completions powershell
```

## Validation Checks

sysml-cli ships with 9 validation checks. Each can be individually disabled with `--disable <name>`.

| Check | Name | Severity | Description |
|-------|------|----------|-------------|
| Syntax | `syntax` | Error | Tree-sitter parse errors and missing syntax elements |
| Duplicates | `duplicates` | Error | Definitions of the same kind with identical names |
| Unused | `unused` | Note | Definitions never referenced in the file |
| Unresolved | `unresolved` | Warning | Type references and targets that don't resolve |
| Unsatisfied | `unsatisfied` | Warning | Requirements with no `satisfy` statement |
| Unverified | `unverified` | Warning | Requirements with no `verify` statement |
| Port Types | `port-types` | Warning | Connected ports with incompatible types |
| Constraints | `constraints` | Warning | Constraint defs with a body but no constraint expression |
| Calculations | `calculations` | Warning | Calc defs with a body but no return statement |

## Diagnostic Codes

### Errors

| Code | Check | Message |
|------|-------|---------|
| E001 | syntax | `Syntax error: near <context>` |
| E002 | duplicates | `duplicate <kind> '<name>' (first defined at line <n>)` |

### Warnings

| Code | Check | Message |
|------|-------|---------|
| W001 | unused | `<kind> '<name>' is defined but never referenced` |
| W002 | unsatisfied | `requirement def '<name>' has no corresponding satisfy statement` |
| W003 | unverified | `requirement def '<name>' has no corresponding verify statement` |
| W004 | unresolved | `type '<name>' is not defined in this file` |
| W005 | unresolved | `reference '<name>' does not resolve to any definition or usage` |
| W006 | port-types | `connected ports have different types` |
| W007 | constraints | `constraint def '<name>' has a body but no constraint expression` |
| W008 | calculations | `calc def '<name>' has a body but no return statement` |

## Output Formats

### Text (default)

```
model.sysml:12:5: warning[W002]: requirement def `MassReq` has no corresponding satisfy statement
```

### JSON

```json
[
  {
    "file": "model.sysml",
    "span": { "start_row": 12, "start_col": 5 },
    "severity": "warning",
    "code": "W002",
    "message": "requirement def `MassReq` has no corresponding satisfy statement"
  }
]
```

## CI Integration

### GitHub Actions

```yaml
name: SysML Lint
on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install sysml-cli
        run: |
          git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git /tmp/sysml-cli
          cd /tmp/sysml-cli
          cargo build --release
          echo "/tmp/sysml-cli/target/release" >> $GITHUB_PATH

      - name: Lint models
        run: sysml-cli lint --severity warning models/**/*.sysml

      - name: Check formatting
        run: sysml-cli fmt --check models/**/*.sysml

      - name: Check requirement coverage
        run: sysml-cli trace --check --min-coverage 80 models/**/*.sysml

      - name: Check model quality
        run: sysml-cli coverage --check --min-score 70 models/**/*.sysml

      - name: Check allocations
        run: sysml-cli allocation --check models/**/*.sysml
```

## Editor Integration

### Emacs (sysml2-mode)

sysml-cli integrates with [sysml2-mode](https://github.com/jackhale98/sysml2-mode) for Flymake diagnostics, interactive simulation, and FMI export. With `sysml-cli` on your `$PATH`:

- **Flymake**: Diagnostics appear inline as you edit
- **Simulation**: `M-x sysml2-simulate` for constraints, state machines, action flows
- **FMI Export**: `M-x sysml2-fmi-extract-interfaces` extracts interfaces via sysml-cli
- **Diagrams**: `M-x sysml2-diagram` generates diagrams inline

### JSON output for other editors

All commands support `-f json` for structured output suitable for editor integration:

```sh
sysml-cli lint -f json model.sysml          # Diagnostics as JSON array
sysml-cli list -f json model.sysml          # Element list as JSON
sysml-cli simulate list -f json model.sysml # Simulatable items as JSON
```

## Building from Source

### Prerequisites

- Rust 1.70+ (stable)
- C compiler (gcc or clang) for tree-sitter grammar compilation

```sh
git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git
cd sysml-cli
cargo build --release
cargo test
```

If you didn't clone with `--recurse-submodules`:

```sh
git submodule update --init
```

## Architecture

```
crates/
  sysml-core/                 Core library (no CLI dependencies)
    src/
      parser.rs               Tree-sitter FFI + model extraction
      model.rs                Model types: definitions, usages, connections
      diagnostic.rs           Diagnostic/severity types and error codes
      resolver.rs             Multi-file import resolution
      checks/                 9 validation checks
      sim/                    Simulation engine
        state_parser.rs       State machine model extraction
        state_sim.rs          State machine simulation
        action_parser.rs      Action flow model extraction
        action_exec.rs        Action flow execution
        constraint_eval.rs    Constraint/calculation evaluation
        expr.rs               Expression types and environment
      codegen/                Code generation and editing
        template.rs           SysML definition template generation
        edit.rs               Byte-accurate surgical text edits
        format.rs             CST-aware source formatting
      diagram/                Diagram generation (7 types, 4 formats)
      export/                 FMI 3.0, Modelica, SSP export
      query.rs                Model querying (list, show, trace, stats, deps, diff, allocation, coverage)
  sysml-cli/                  CLI frontend
    src/
      main.rs                 Clap command definitions and dispatch
      commands/               One module per command
      output.rs               Output formatting
    tests/
      cli.rs                  CLI integration tests
tree-sitter-sysml/            Grammar (git submodule)
test/fixtures/                SysML v2 test files
```

## License

GPL-3.0-or-later
