# Architecture

## Workspace Structure

`sysml` is a Cargo workspace with 3 crates. The `sysml-core` library is the foundation — both the CLI and language server depend only on it.

```
crates/
  sysml-core/         Core library (parser, model, checks, simulation, codegen)
  sysml-cli/          CLI frontend (clap, command dispatch, output formatting)
  sysml-lsp/          Language server (LSP) for editor integration
tree-sitter-sysml/    Grammar (git submodule)
test/fixtures/        SysML v2 test files
```

## Crate Dependency Graph

```
sysml-core
    |
    +-- sysml-cli
    +-- sysml-lsp
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
  sim/                    Simulation and calculation engine
    state_parser.rs       State machine model extraction
    state_sim.rs          State machine simulation
    action_parser.rs      Action flow model extraction
    action_exec.rs        Action flow execution
    constraint_eval.rs    Constraint/calculation evaluation
    expr.rs               Expression types and environment
    resolve.rs            Attribute resolution across part hierarchy
    rollup.rs             Generic rollup engine (sum, RSS, product, min, max)
    what_if.rs            What-if scenarios and parametric sweeps
    units.rs              Unit conversion system (mass, length, power, etc.)
    analysis.rs           Analysis case extraction and evaluation
  codegen/                Code generation and editing
    template.rs           SysML definition template generation
    edit.rs               Byte-accurate surgical text edits
    format.rs             CST-aware source formatting
  diagram/                Diagram generation (10 types, 4 formats)
  export/                 FMI 3.0, Modelica, SSP export
  query.rs                Model querying (list, show, trace, stats, deps, diff, allocation, coverage)
```

## sysml-lsp

The language server is a standalone binary that communicates over stdio using the Language Server Protocol. It depends only on `sysml-core` and `tower-lsp`.

```
src/
  main.rs               Tokio entrypoint, stdio transport
  server.rs             LanguageServer trait impl, request dispatch
  state.rs              WorldState: per-file models, workspace def index (DashMap)
  convert.rs            Span conversion (1-based sysml-core ↔ 0-based LSP)
  diagnostics.rs        Run all_checks() → LSP PublishDiagnostics
  document_symbols.rs   Hierarchical outline (definitions + nested usages)
  goto_definition.rs    Jump to definition (in-file + cross-file via workspace index)
  references.rs         Find all references across open files
  hover.rs              Kind, type, doc, qualified name, members on hover
  completion.rs         File defs + workspace defs + stdlib names
  workspace_symbols.rs  Filter workspace defs by query
  semantic_tokens.rs    Tree-sitter highlights.scm → LSP semantic tokens
  code_actions.rs       Quick-fix edits from diagnostic suggestions
  formatting.rs         CST-aware document formatting via sysml-core
  document_highlight.rs Highlight all occurrences of symbol under cursor
  folding.rs            Folding ranges for definition blocks and comments
  rename.rs             Cross-file symbol rename with word-boundary matching
```

State is managed with `DashMap` for concurrent access — tower-lsp dispatches requests concurrently. Full text sync (`TextDocumentSyncKind::FULL`) with full reparse on every change; tree-sitter is fast and SysML files are small. On `initialize`, the server scans the workspace for `.sysml`/`.kerml` files to build the cross-file definition index.

## Design Principles

**Model vs Records vs Tool**: SysML files define types and structure. TOML records capture operational data (what happened, when, by whom). The tool provides execution logic, validation, and reporting.

**Flat command namespace**: All commands are top-level (`sysml risk matrix`, not `sysml lifecycle risk matrix`). Designed for non-software engineers who shouldn't need to memorize a command hierarchy.

**Progressive enhancement**: The tool works with zero configuration for pure SysML v2 analysis. The `.sysml/` project and cache are opt-in.
