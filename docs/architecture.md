# Architecture

## Workspace Structure

`sysml` is a Cargo workspace with 12 crates. The `sysml-core` library is the foundation — all domain crates depend only on it. No domain crate depends on any other domain crate.

```
crates/
  sysml-core/         Core library (parser, model, checks, simulation, codegen)
  sysml-cli/          CLI frontend (clap, command dispatch, output formatting)
  sysml-verify/       Verification domain
  sysml-scaffold/     Scaffolding and template generation
  sysml-risk/         Risk management
  sysml-tol/          Tolerance analysis
  sysml-bom/          Bill of materials
  sysml-source/       Supplier management
  sysml-mfg/          Manufacturing execution
  sysml-qc/           Quality control
  sysml-capa/         Quality management (NCR, CAPA, Process Deviation)
  sysml-report/       Cross-domain reporting
libraries/            Domain library .sysml files
tree-sitter-sysml/    Grammar (git submodule)
test/fixtures/        SysML v2 test files
```

## Crate Dependency Graph

```
sysml-core
    |
    +-- sysml-verify
    +-- sysml-scaffold
    +-- sysml-risk
    +-- sysml-tol
    +-- sysml-bom
    +-- sysml-source
    +-- sysml-mfg
    +-- sysml-qc
    +-- sysml-capa
    +-- sysml-report
    |
    +-- sysml-cli  (depends on sysml-core + all domain crates)
```

## sysml-core

The core library has no CLI dependencies and is frontend-agnostic.

```
src/
  parser.rs               Tree-sitter FFI + model extraction
  model.rs                Model types: definitions, usages, connections
  qualified_name.rs       QualifiedName type (Package::Element paths)
  diagnostic.rs           Diagnostic/severity types and error codes
  resolver.rs             Multi-file import resolution
  config.rs               Project configuration (.sysml/config.toml)
  project.rs              Project discovery (walk-up from CWD)
  record.rs               TOML record system (append-only records)
  cache.rs                In-memory cache (nodes, edges, records)
  sqlite_cache.rs         SQLite-backed persistent cache (optional, `sqlite` feature)
  index.rs                Indexer (populates cache from files)
  interactive.rs          Wizard framework (WizardStep, WizardRunner)
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
  diagram/                Diagram generation (10 types, 4 formats)
  export/                 FMI 3.0, Modelica, SSP export
  query.rs                Model querying (list, show, trace, stats, deps, diff, allocation, coverage)
```

## Design Principles

**Model vs Records vs Tool**: SysML files define types and structure. TOML records capture operational data (what happened, when, by whom). The tool provides execution logic, validation, and reporting.

**Flat command namespace**: All commands are top-level (`sysml risk matrix`, not `sysml lifecycle risk matrix`). Designed for non-software engineers who shouldn't need to memorize a command hierarchy.

**No cross-domain crate dependencies**: Domain crates communicate through the shared cache and record system in `sysml-core`. This keeps compile times fast and prevents circular dependencies.

**Git-native records**: Append-only TOML records use filenames that encode timestamp + author + hash, making merge conflicts impossible. Entity records use `BTreeMap` for deterministic key ordering.

**Progressive enhancement**: The tool works with zero configuration for pure SysML v2 analysis. The `.sysml/` project, cache, records, and domain libraries are opt-in.

**Optional SQLite cache**: When built with `--features sqlite`, the index command persists the in-memory cache to `.sysml/cache.db` using `rusqlite` (bundled). The `SqliteCache` has the same query API as the in-memory `Cache`. It stores a git HEAD hash so stale caches can be detected. The feature is opt-in — the default build uses only the in-memory cache.
