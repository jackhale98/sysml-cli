# sysml2-cli

A fast, standalone SysML v2 model validator, simulator, and FMI export tool for CI pipelines and editor integration.

Built on [tree-sitter](https://tree-sitter.github.io/) for reliable parsing of SysML v2 textual notation. Includes structural linting, a behavioral simulation engine, and FMI 3.0 export capabilities.

> **Note**: This project was previously named `sysml-lint`. The binary is now `sysml2-cli`.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
- [Linting](#linting)
- [Simulation](#simulation)
- [Export](#export)
- [Checks](#checks)
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
git clone https://github.com/jackhale98/sysml-lint.git
cd sysml-lint
cargo install --path .
```

Or manually:

```sh
cargo build --release
cp target/release/sysml2-cli ~/.local/bin/
```

The build compiles the [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar from source. The grammar must be available at `./tree-sitter-sysml/src/` (vendored) or `../tree-sitter-sysml/src/` (sibling directory).

## Usage

sysml2-cli has three subcommands: `lint`, `simulate`, and `export`.

```sh
# Lint SysML files
sysml2-cli lint model.sysml

# Simulate constructs
sysml2-cli simulate list model.sysml

# Export FMI interfaces
sysml2-cli export interfaces model.sysml --part Engine
```

### Global Options

```
-f, --format <FORMAT>      Output format: text, json [default: text]
-q, --quiet                Suppress summary line on stderr
-I, --include <PATH>       Additional files/directories for import resolution
-h, --help                 Print help
-V, --version              Print version
```

## Linting

```sh
# Lint a single file
sysml2-cli lint model.sysml

# Lint multiple files (imports auto-resolve between them)
sysml2-cli lint src/*.sysml

# Include additional files for import resolution
sysml2-cli lint model.sysml -I lib/

# JSON output for tooling
sysml2-cli lint --format json model.sysml

# Only show warnings and errors
sysml2-cli lint --severity warning model.sysml

# Disable specific checks
sysml2-cli lint --disable unused,unresolved model.sysml
```

### Lint Options

```
sysml2-cli lint [OPTIONS] <FILES>...

Arguments:
  <FILES>...  SysML v2 files to validate

Options:
  -d, --disable <DISABLE>    Disable specific checks (comma-separated)
  -s, --severity <SEVERITY>  Minimum severity: note, warning, error [default: note]
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No errors found (may have warnings or notes) |
| 1 | One or more errors found, or a file could not be read |

## Simulation

sysml2-cli includes a built-in simulation engine that can evaluate constraints, run calculations, simulate state machines, and execute action flows.

### List Simulatable Constructs

```sh
sysml2-cli simulate list model.sysml
```

### Evaluate Constraints and Calculations

```sh
# Evaluate a constraint
sysml2-cli simulate eval model.sysml -b speed=100 -n SpeedLimit

# Evaluate a calculation
sysml2-cli simulate eval model.sysml -b mass=1500,velocity=30 -n KineticEnergy

# JSON output
sysml2-cli -f json simulate eval model.sysml -b speed=100
```

### Simulate State Machines

Supports `state def`, `exhibit state` (inside `part def` bodies), and nested
state regions (e.g. parallel orthogonal states like `operatingStates` and
`healthStates` inside an exhibit). Initial pseudo-transitions
(`transition initial then off;`) are recognized automatically.

```sh
# Simulate with events
sysml2-cli simulate state-machine model.sysml -n TrafficLight -e next,next,next

# With guard variable bindings
sysml2-cli simulate state-machine model.sysml -n Controller -b temperature=150

# Simulate a nested state region from an exhibit state
sysml2-cli simulate state-machine vehicle.sysml -n operatingStates -e ignitionCmd,VehicleOnSignal
```

### Execute Action Flows

```sh
# Execute an action flow
sysml2-cli simulate action-flow model.sysml -n ProcessOrder

# With variable bindings for conditionals
sysml2-cli simulate action-flow model.sysml -n Workflow -b priority=high
```

## Export

sysml2-cli extracts FMI 3.0 interface contracts, generates Modelica stubs, and produces SSP (System Structure and Parameterization) XML — all from tree-sitter AST analysis.

### List Exportable Parts

```sh
sysml2-cli export list model.sysml
```

Output:
```
Exportable Parts:
  Engine (3 ports, 2 attributes, 0 connections)
  Transmission (1 ports, 1 attributes, 0 connections)
```

### Extract FMI Interfaces

```sh
# Text output
sysml2-cli export interfaces model.sysml --part Engine

# JSON output (for editor integration)
sysml2-cli -f json export interfaces model.sysml --part Engine
```

Output:
```
FMI Interface: Engine
------------------------------------------------------------
  Name            Direction  SysML Type   FMI Type   Causality    Port
  ----------------------------------------------------------------------
  fuelFlow        in         Real         Float64    input        fuelIn
  torque          in         Real         Float64    input        driveOut
  speed           in         Real         Float64    input        driveOut
  ignitionOn      in         Boolean      Boolean    input        ignition

  Attributes:
    displacement : Real
    cylinders : Integer
```

The extraction handles:
- Port definitions with `in item` / `out item` declarations
- Conjugation (`~`) — flips direction (e.g., `port driveOut : ~DrivePort` makes `out` items become `in`)
- SysML → FMI 3.0 type mapping (`Real` → `Float64`, `Integer` → `Int32`, etc.)
- Attributes as FMI parameters

### Generate Modelica Stubs

```sh
# Print to stdout
sysml2-cli export modelica model.sysml --part Engine

# Write to file
sysml2-cli export modelica model.sysml --part Engine --output Engine.mo
```

Output:
```modelica
partial model Engine
  "Generated from SysML v2 part def Engine"
  Modelica.Blocks.Interfaces.RealInput fuelFlow "From port fuelIn";
  Modelica.Blocks.Interfaces.RealInput torque "From port driveOut";
  Modelica.Blocks.Interfaces.RealInput speed "From port driveOut";
  Modelica.Blocks.Interfaces.BooleanInput ignitionOn "From port ignition";
  parameter Real displacement "From SysML attribute";
  parameter Integer cylinders "From SysML attribute";
equation
  // Equations to be filled by model developer
end Engine;
```

### Generate SSP XML

```sh
# Print SystemStructureDescription XML to stdout
sysml2-cli export ssp model.sysml

# Write to file
sysml2-cli export ssp model.sysml --output system.ssd
```

Extracts part usages as components and connections as SSP wiring, splitting dotted references (`engine.driveOut`) into element/connector pairs.

## Checks

sysml2-cli ships with 9 validation checks. Each can be individually disabled with `--disable <name>`.

| Check | Name | Severity | Description |
|-------|------|----------|-------------|
| Syntax | `syntax` | Error | Reports tree-sitter parse errors and missing syntax elements |
| Duplicates | `duplicates` | Error | Detects definitions of the same kind with identical names |
| Unused | `unused` | Note | Definitions that are never referenced anywhere in the file |
| Unresolved | `unresolved` | Warning | Type references and connection/allocation targets that don't resolve |
| Unsatisfied | `unsatisfied` | Warning | Requirement definitions with no corresponding `satisfy` statement |
| Unverified | `unverified` | Warning | Requirement definitions with no corresponding `verify` statement |
| Port Types | `port-types` | Warning | Connected ports with incompatible types |
| Constraints | `constraints` | Warning | Constraint definitions with a body but no constraint expression |
| Calculations | `calculations` | Warning | Calculation definitions with a body but no return statement |

## Diagnostic Codes

### Errors

| Code | Check | Message |
|------|-------|---------|
| E001 | syntax | `Syntax error: near <context>` or `Missing expected syntax element: near <context>` |
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

      - name: Install sysml2-cli
        run: |
          git clone https://github.com/jackhale98/tree-sitter-sysml.git
          git clone https://github.com/jackhale98/sysml-lint.git
          cd sysml-lint
          cargo build --release
          echo "$PWD/target/release" >> $GITHUB_PATH

      - name: Lint SysML models
        run: sysml2-cli lint --severity warning models/**/*.sysml
```

## Editor Integration

### Emacs (sysml2-mode)

sysml2-cli integrates with [sysml2-mode](https://github.com/jackhale98/sysml2-mode) for Flymake diagnostics, interactive simulation, and FMI export. With `sysml2-cli` on your `$PATH`:

- **Flymake**: Diagnostics appear inline as you edit
- **Simulation**: `M-x sysml2-simulate` for constraints, state machines, action flows
- **FMI Export**: `M-x sysml2-fmi-extract-interfaces` uses tree-sitter AST extraction via sysml2-cli
- **Modelica**: `M-x sysml2-fmi-generate-modelica` generates stubs via sysml2-cli
- **SSP**: `M-x sysml2-cosim-generate-ssp` generates SystemStructureDescription via sysml2-cli

## Building from Source

### Prerequisites

- Rust 1.70+ (stable)
- C compiler (gcc or clang) for tree-sitter grammar compilation
- [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar source

```sh
git clone https://github.com/jackhale98/tree-sitter-sysml.git
git clone https://github.com/jackhale98/sysml-lint.git
cd sysml-lint
cargo build --release
cargo test
```

## Architecture

```
src/
  main.rs          CLI entry point (clap subcommands: lint, simulate, export)
  lib.rs           Public module exports
  parser.rs        Tree-sitter FFI + Model extraction (direction, conjugation, scope)
  model.rs         Model types: definitions, usages, connections, flows, etc.
  diagnostic.rs    Diagnostic/Severity types and error codes
  output.rs        Text and JSON formatters
  resolver.rs      Multi-file import resolution
  checks/          9 validation checks (syntax, duplicates, references, etc.)
  sim/             Simulation engine (constraints, state machines, action flows)
  export/
    mod.rs         Export module declarations
    fmi.rs         FMI 3.0 interface extraction and type mapping
    modelica.rs    Modelica partial model stub generation
    ssp.rs         SSP SystemStructureDescription XML generation
tests/
  integration.rs   Integration tests
test/
  fixtures/        SysML v2 example files for testing
```

## License

GPL-3.0-or-later
