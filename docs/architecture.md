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
  interactive.rs          Wizard framework (WizardStep, WizardRunner)
  checks/                 17 validation checks (16 registered + W017 value-constraints)
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
    uncertainty.rs        Uncertainty propagation math (worst-case, RSS, Monte Carlo + histogram)
    uncertainty_model.rs  Uncertainty case extraction (feature chains, targets, settings)
    eval.rs               Expression evaluation
    expr_parser.rs        Expression parsing (shared by views, gates, W017)
    state_machine.rs      State machine types
    action_flow.rs        Action flow types
  codegen/                Code generation and editing
    template.rs           SysML definition template generation
    edit.rs               Byte-accurate surgical text edits
    format.rs             CST-aware source formatting
  diagram/                Diagram generation (11 types, 4 formats)
  export/                 Modelica stub and SSP export (FMI interface extraction)
  view_render.rs          Model-defined view engine (@TableRendering: row providers, computed columns)
  stdlib.rs               Embedded SysML v2 standard library (member-checked references)
  query.rs                Model querying (list, show, trace, deps, diff, allocation, coverage, gates)
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
  inlay_hints.rs        Multiplicity and inferred-type hints
  type_hierarchy.rs     Supertype/subtype navigation across files
  code_lens.rs          Satisfy / verify / usage counts above definitions
  document_link.rs      Clickable imports and supertypes
```

State is managed with `DashMap` for concurrent access — tower-lsp dispatches requests concurrently. Full text sync (`TextDocumentSyncKind::FULL`) with full reparse on every change; tree-sitter is fast and SysML files are small. On `initialize`, the server scans the workspace for `.sysml`/`.kerml` files to build the cross-file definition index.

## Design Principles

**Model vs Tool**: SysML files define types, structure, and domain semantics (calcs, constraints, view specs). The tool provides execution logic, validation, and rendering.

**Flat command namespace**: All commands are top-level (`sysml rollup compute`, not `sysml analysis rollup compute`). Designed for non-software engineers who shouldn't need to memorize a command hierarchy.

**Progressive enhancement**: The tool works with zero configuration for pure SysML v2 analysis. The `.sysml/` project config is opt-in.
