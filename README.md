# sysml-lint

A fast, standalone SysML v2 model validator and linter for CI pipelines and editor integration.

Built on [tree-sitter](https://tree-sitter.github.io/) for reliable parsing of SysML v2 textual notation. Produces structured diagnostics in text or JSON format with configurable severity filtering.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
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
cargo build --release
cp target/release/sysml-lint ~/.local/bin/
```

The build compiles the [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar from source. The grammar must be available at `./tree-sitter-sysml/src/` (vendored) or `../tree-sitter-sysml/src/` (sibling directory).

## Usage

```sh
# Lint a single file
sysml-lint model.sysml

# Lint multiple files
sysml-lint src/*.sysml

# JSON output for tooling
sysml-lint --format json model.sysml

# Only show warnings and errors
sysml-lint --severity warning model.sysml

# Only show errors
sysml-lint --severity error model.sysml

# Disable specific checks
sysml-lint --disable unused,unresolved model.sysml

# Quiet mode (no summary on stderr)
sysml-lint --quiet model.sysml
```

### CLI Reference

```
sysml-lint [OPTIONS] <FILES>...

Arguments:
  <FILES>...  SysML v2 files to validate

Options:
  -f, --format <FORMAT>      Output format: text, json [default: text]
  -d, --disable <DISABLE>    Disable specific checks (comma-separated)
  -s, --severity <SEVERITY>  Minimum severity to report: note, warning, error [default: note]
  -q, --quiet                Suppress summary line on stderr
  -h, --help                 Print help
  -V, --version              Print version
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No errors found (may have warnings or notes) |
| 1 | One or more errors found, or a file could not be read |

## Checks

sysml-lint ships with 9 validation checks. Each can be individually disabled with `--disable <name>`.

| Check | Name | Severity | Description |
|-------|------|----------|-------------|
| Syntax | `syntax` | Error | Reports tree-sitter parse errors and missing syntax elements |
| Duplicates | `duplicates` | Error | Detects definitions of the same kind with identical names |
| Unused | `unused` | Note | Definitions that are never referenced anywhere in the file |
| Unresolved | `unresolved` | Warning | Type references and connection/allocation targets that don't resolve to any definition or usage |
| Unsatisfied | `unsatisfied` | Warning | Requirement definitions with no corresponding `satisfy` statement |
| Unverified | `unverified` | Warning | Requirement definitions with no corresponding `verify` statement |
| Port Types | `port-types` | Warning | Connected ports with incompatible types |
| Constraints | `constraints` | Warning | Constraint definitions with a body but no constraint expression |
| Calculations | `calculations` | Warning | Calculation definitions with a body but no return statement |

### Check Details

#### Syntax (`syntax`)

Reports nodes where tree-sitter encountered a parse error (`ERROR`) or expected a syntax element that was missing (`MISSING`). These indicate genuinely broken syntax, not style issues.

#### Duplicates (`duplicates`)

Flags when two definitions of the same kind share the same name within a file. For example, two `part def Vehicle` declarations. The diagnostic references the line number of the first definition.

#### Unused (`unused`)

Finds definitions that are never referenced by any usage, specialization, connection, flow, satisfaction, verification, or allocation in the file. Package definitions are excluded since they are structural containers rather than referenceable types.

#### Unresolved (`unresolved`)

Checks that type references (`: SomeType`), connection endpoints, and allocation targets resolve to a known name. Known names include all definitions and usages in the file, plus the SysML v2 standard library types (ScalarValues, ISQ, SI units, base types). Qualified stdlib references like `ISQ::MassValue` or `SI::kg` are recognized automatically.

#### Unsatisfied (`unsatisfied`)

Checks that every `requirement def` in the file has at least one `satisfy` statement referencing it. Missing satisfaction traces are a common MBSE gap.

#### Unverified (`unverified`)

Checks that every `requirement def` in the file has at least one `verify` statement referencing it. Verification traceability is critical for systems engineering V&V.

#### Port Types (`port-types`)

When two ports are connected, checks that their declared types are compatible. Types are compatible if they are identical, or if one is the conjugate of the other (prefixed with `~`). For example, `FuelPort` and `~FuelPort` are compatible, but `FuelPort` and `ElectricalPort` are not.

#### Constraints (`constraints`)

Checks that `constraint def` declarations with a body block actually contain a constraint expression. A constraint definition like `constraint def C { in x : Real; }` that declares parameters but has no expression (`x > 0;`) is likely incomplete. Forward-declared constraints (`constraint def C;`) are not flagged.

#### Calculations (`calculations`)

Checks that `calc def` declarations with a body block contain a `return` statement. A calculation definition like `calc def F { in x : Real; }` with parameters but no `return` is likely incomplete. Forward-declared calcs (`calc def F;`) are not flagged.

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
| W006 | port-types | `connected ports have different types: '<a>' is '<typeA>' but '<b>' is '<typeB>'` |
| W007 | constraints | `constraint def '<name>' has a body but no constraint expression` |
| W008 | calculations | `calc def '<name>' has a body but no return statement` |

## Output Formats

### Text (default)

Standard compiler-style diagnostics:

```
model.sysml:12:5: warning[W002]: requirement def `MassReq` has no corresponding satisfy statement
model.sysml:20:5: note[W001]: part def `Engine` is defined but never referenced
```

Format: `file:line:col: severity[code]: message`

### JSON

Structured output for editor integration and CI tooling:

```json
[
  {
    "file": "model.sysml",
    "span": {
      "start_row": 12,
      "start_col": 5,
      "end_row": 15,
      "end_col": 6,
      "start_byte": 234,
      "end_byte": 310
    },
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

      - name: Install sysml-lint
        run: |
          git clone https://github.com/jackhale98/tree-sitter-sysml.git
          git clone https://github.com/jackhale98/sysml-lint.git
          cd sysml-lint
          cargo build --release
          echo "$PWD/target/release" >> $GITHUB_PATH

      - name: Lint SysML models
        run: sysml-lint --severity warning models/**/*.sysml
```

### GitLab CI

```yaml
sysml-lint:
  stage: test
  script:
    - sysml-lint --format json --severity warning models/*.sysml > lint-results.json
  artifacts:
    reports:
      codequality: lint-results.json
```

### Pre-commit Hook

```sh
#!/bin/sh
# .git/hooks/pre-commit
sysml_files=$(git diff --cached --name-only --diff-filter=ACM | grep '\.sysml$')
if [ -n "$sysml_files" ]; then
    sysml-lint --severity error $sysml_files
fi
```

## Editor Integration

### Emacs (Flymake)

sysml-lint integrates with [sysml2-mode](https://github.com/jackhale98/sysml2-mode) via Flymake. With `sysml-lint` on your `$PATH`, Flymake will show diagnostics inline as you edit.

### Generic (JSON pipe)

Any editor that can run an external command and parse JSON can integrate with sysml-lint:

```sh
sysml-lint --format json --quiet model.sysml
```

The JSON output includes byte offsets (`start_byte`, `end_byte`) for precise highlighting and line/column positions for gutter markers.

## Building from Source

### Prerequisites

- Rust 1.70+ (stable)
- C compiler (gcc or clang) for tree-sitter grammar compilation
- [tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml) grammar source

### Build

```sh
# Clone with grammar as sibling
git clone https://github.com/jackhale98/tree-sitter-sysml.git
git clone https://github.com/jackhale98/sysml-lint.git
cd sysml-lint

# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

### Vendoring the Grammar

To make the project self-contained, copy the grammar source into the project:

```sh
mkdir -p tree-sitter-sysml/src
cp ../tree-sitter-sysml/src/parser.c tree-sitter-sysml/src/
cp ../tree-sitter-sysml/src/tree_sitter/ tree-sitter-sysml/src/ -r
```

The build script checks `./tree-sitter-sysml/src/` first, then falls back to `../tree-sitter-sysml/src/`.

## Architecture

```
src/
  main.rs          CLI entry point (clap)
  lib.rs           Public module exports
  parser.rs        Tree-sitter FFI + parse tree → Model extraction
  model.rs         Model types: definitions, usages, connections, flows, etc.
  diagnostic.rs    Diagnostic/Severity types and error codes
  output.rs        Text and JSON formatters
  checks/
    mod.rs         Check trait + registry
    syntax.rs      E001: parse errors
    duplicates.rs  E002: duplicate definitions
    references.rs  W001/W004/W005: unused defs, unresolved types
    requirements.rs W002/W003: unsatisfied/unverified requirements
    ports.rs       W006: port type mismatches
    constraints.rs W007: empty constraint bodies
    calculations.rs W008: calc missing return
tests/
  integration.rs   Integration tests (17 tests)
test/
  fixtures/        SysML v2 example files for testing
```

### Parser

The parser uses tree-sitter-sysml via FFI (`extern "C"`) to parse SysML v2 textual notation into a concrete syntax tree. A single-pass recursive walk extracts structural model elements:

- **Definitions** (27 SysML/KerML kinds): `part def`, `port def`, `action def`, etc.
- **Usages**: `part`, `port`, `action`, `state`, `binding`, `succession`, etc.
- **Connections**: From `connect_clause` in connection usages
- **Flows**: `flow of Type from A to B`
- **Satisfactions**: `satisfy Requirement` with optional `by`
- **Verifications**: `require` statements inside verification definitions
- **Allocations**: `allocate A to B`

### Model

The extracted model provides two key queries used by checks:

- `defined_names()` — All definition names in the file
- `referenced_names()` — All names referenced via typing, specialization, connections, flows, satisfactions, verifications, allocations, and explicit type references

### Standard Library

sysml-lint recognizes 49 built-in SysML v2 standard library types so they are not flagged as unresolved. These cover:

- **ScalarValues**: `Boolean`, `String`, `Integer`, `Real`, `Natural`, `Complex`, `Rational`
- **ISQ quantities**: `MassValue`, `LengthValue`, `TimeValue`, `PowerValue`, `ForceValue`, and 15 others
- **SI units**: `kg`, `m`, `s`, `A`, `K`, `mol`, `cd`, `N`, `Pa`, `J`, `W`, `Hz`
- **Base types**: `Anything`, `Nothing`, `Object`, `Part`, `Port`, `Action`, `State`, etc.
- **Libraries**: `TradeStudy`, `VerdictKind`, `SampledFunction`, etc.

Qualified standard library references (e.g., `ISQ::MassValue`, `SI::kg`, `ScalarValues::Real`) are also recognized via namespace prefix matching.

## License

GPL-3.0-or-later
