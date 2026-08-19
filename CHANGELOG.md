# Changelog

## Unreleased

### Fixed
- **Naming a succession discarded the edge it declared.** `succession s1
  first a then b;` recorded the name and dropped both endpoints, because
  endpoints were stored by overloading the `name` and `type_ref` fields —
  which the name and type immediately took back. `Usage` now has real
  `source` and `target` fields, and all three forms populate them: named,
  anonymous (`first a then b;`), and typed (`succession s : Dependency
  first a then b;`). Endpoints may be feature chains, and a succession's
  type is no longer mistaken for an endpoint.
- An anonymous `succession first a then b;` was dropped entirely for
  having no name. It is still an edge, so it is recorded with an empty
  name.

### Added
- `relation:succession` row provider, with a `relation:succession:<Type>`
  filter that closes over specialization exactly as `connection:<Type>`
  does. Columns: `succession`, `type`, `source`, `target`, `parent`,
  `file`. The dependency graph of an action flow was previously
  unreachable from `sysml view` in every declaration form — this is what
  a critical-path or Gantt export reads.

## 0.9.3 — 2026-08-18

### Changed
- **The PASS / MARGINAL / FAIL threshold comes from the model.** "Within
  10% of a limit is too close to call a pass" was a constant in the
  analyzer; it is now `UncertaintyAnalysis::marginalFraction`, which a
  project overrides on one analysis (`attribute :>> marginalFraction =
  0.25;`) or across a class of them by specializing the def. A verdict
  is an acceptance policy, not arithmetic, so it belongs where the
  thresholds a team argues about already live.
- Analysis settings (`sigmaLevel`, `meanShiftK`, `iterations`,
  `marginalFraction`) resolve case body → type-chain defaults →
  built-in. The library's `default` values were previously
  documentation, with the real defaults duplicated in Rust: editing the
  library changed the docs and nothing else.

### Fixed
- **A `where` clause could not compare strings.** `unquote` used
  `trim_matches('"')`, which strips *every* trailing quote — so a spec
  nesting a string, `where = "category == \"software\""`, lost the
  inner closing quote and failed to parse, silently filtering every row
  out. Escapes are now resolved after a single layer of quoting is
  removed.
- Enum values bind to `where` by their simple name, so
  `category == "software"` matches `RiskCategory::software` — the loose
  comparison `list --metadata --where` already used.

## 0.9.2 — 2026-08-18

Generic primitives for questions the libraries used to have to answer.

### Added
- **`list --type <Type>`** filters by declared type, following the
  specialization closure: `--type Hazard` finds every usage typed by
  `Hazard` or any subtype, and every definition specializing it. The
  closure resolves across the include path, so a project type declared
  against a library supertype still matches. There was no way to ask
  "show me everything that is an X" — `--kind` only knows SysML
  metaclasses like `part` and `port`.
- **`relation:connection:<Type>` row provider.** Connections now carry
  their declared type, so a view can separate a tolerance mate from a
  hazard causation. `FitTable` was listing the whole hazard chain as
  fits.
- **`composition` / `composition:<Root>` row provider** — a bill of
  materials. Walks `part` and `item` usages (connections are structure,
  not content), reporting `path`, `depth`, `quantity` from the usage
  multiplicity, and `extended` — quantity multiplied down the tree, so a
  part used 2x inside an assembly used 3x reports 6. Attributes declared
  on a row's type can be named as columns. `sysml view Bom -f csv`
  exports it. The library ships a `Bom` view; the columns that matter
  are the model's business, not the tool's.
- `deps` follows connections as **directed** edges labelled with the
  connection's type and name, and names the element on the other end
  rather than the connection itself — so `deps X --forward --transitive`
  walks a causal chain to its end. It previously reported the connection
  and stopped there, which made a hazard-to-harm trace impossible.

### Fixed
- **Tolerance stackup views returned no rows.** 0.9.1 scoped view rows
  to the target files but scoped *type resolution* with them, so
  `ToleranceStackup :> UncertaintyAnalysis` became unreachable and every
  stackup vanished unless the library was also named on the command
  line. Rows come from the target; types resolve against everything.
  Same fix for `type:` rows.
- A bare `//` line — the blank separator inside a comment block — was a
  syntax error (grammar submodule).

### Changed
- `view` and `deps` sort their positionals by what they are rather than
  where they sit: an argument naming an existing path is a file, the one
  that does not is the view name or target. `sysml view m.sysml Fmea`
  and `sysml view Fmea m.sysml` both work now. Every other command takes
  files first, and that habit used to produce "cannot read `Fmea`".

## 0.9.1 — 2026-08-18

### Fixed
- **Reporting commands describe YOUR files, not the include path.**
  `trace --stdlib-path ...` listed the standard library's own
  requirements (`self`, `subrequirements`, `requirementChecks`,
  `satisfiedRequirementChecks`, ...) as if you had written them, and
  `view ModelStats` counted every definition in every library on the
  include path. Include paths exist so references resolve and so
  library-defined views can be found; their contents are not your
  model. `check` already drew this line — `trace`, `coverage`, and
  `view` now do too. Cross-file satisfy/verify still counts: the
  traceability closure is fed through `external_satisfied` /
  `external_verified`, so project-wide tracing is unaffected.
- Test fixtures are now conformant SysML, verified against the OMG
  pilot implementation (zero syntax errors, down from 89). Notably
  `satisfy requirement X by Y;` does not reference a requirement
  definition — the pilot reads it as declaring an untyped usage named
  X — which made `trace` report a phantom requirement and an
  unsatisfied one. The conformant forms are `satisfy X by Y;` and
  `satisfy requirement x : X by Y;`.
- `type:` view rows were still drawn from every model on the include
  path rather than the target files.

## 0.9.0 — 2026-08-17

Editor setup, model-chosen gate names, and one path for model-declared
renderings. Grammar unchanged (tree-sitter-sysml v0.6.0).

### Views are the one path for model-declared renderings
- 'sysml view <name>' renders diagram views too: a view def with a
  'render as' clause (official syntax, e.g. 'render asTreeDiagram')
  routes through the diagram machinery with the view's expose/filter
  selection; pick the output format with -r/--renderer (mermaid,
  plantuml, dot, d2). The view listing labels these '(diagram: ...)'
- 'diagram --view' removed: 'diagram' is the ad-hoc command (--type
  required); model-declared views render through 'view'
- 'analyze list' removed: 'sysml list -k analyses' enumerates cases,
  and 'analyze run' lists candidates when -n is ambiguous or missing

### Gate names are user vocabulary, not tool vocabulary
- 'coverage --check --gate <Name>' / 'trace --check --gate <Name>'
  choose which constraint def is the gate; '[gates] coverage/trace' in
  .sysml/config.toml sets project defaults; QualityGate/TraceGate stay
  as the conventional fallback names. Flag > config > convention
- `list -k analyses` lists analysis cases (defs and usages), replacing
  the removed `analyze list`

### Docs
- New [Editor Setup](docs/editors.md): the grammar and the language
  server together, per editor — Emacs, Vim/Neovim, VS Code (semantic
  tokens; no native tree-sitter), Helix, Zed
- A full docs-vs-code audit corrected the clap help itself
  (unparseable `diagram -s`/`--view` examples, a nonexistent `export
  interfaces` subcommand, a wrong REPL command list, completion install
  paths using the wrong binary name), the diagram reference (a third of
  it documented the removed `--view`; canonical SysML v2 view names are
  now primary and the Sequence View is documented), the check table in
  validation.md (port-types listed twice, value-constraints missing,
  W015 filed as a warning when it is an error), the `check` exit-code
  claim in the analysis guide (errors only — warnings never failed the
  build), LSP capability counts (17, not 19), and architecture.md's
  module listing. `repl` and `doc` gained reference sections.

## 0.8.0 — 2026-08-15

### Conformant quoted names
- The OMG pilot implementation requires quoted names when a member's
  spelling collides with a keyword (`'occurrence' = 3`,
  `RiskCategory::'use'`). The parser now normalizes them — metadata
  keys and name-path values, redefinition targets (`:>> 'occurrence'`),
  and expression identifiers all reach checks, views, and analyses
  unquoted (`model::normalize_name_path`)

### REPL dispatches into the real commands
- `check`, `view <name>`, and `analyze <case> [method]` inside the REPL
  call the batch implementations — same include-path resolution, same
  output
- The REPL loads through the shared `-I`/config-aware loader; its
  private loader ignored include paths, so imported libraries were
  invisible in REPL sessions

### Coverage and trace agree with the checks (breaking output)
- `coverage` and `trace` now resolve requirement satisfaction the same
  way W002/W003 do — through requirement usages and `<id>` short names,
  closed over specialization (`resolver::traced_requirement_defs`).
  Trace shows `(via specialization)` for base defs covered only through
  a specializing requirement
- A model-declared `calc def QualityScore` (parameters `documented`,
  `typedUsages`, `reqSatisfied`, `reqVerified`, each 0-100) supplies the
  overall coverage score; the built-in equal weighting is the fallback.
  `ModelQuality.sysml` in sysml-domain-libraries ships the default
- `trace` and `allocation` render through the same table pipeline as
  `view`: `-f csv|md` work directly, and text tables are content-width
  (the fixed 20/25-char columns are gone)
- **Gate thresholds live in the model**: `--min-score` and
  `--min-coverage` are gone. `coverage --check` / `trace --check`
  evaluate a model-declared constraint usage typed `QualityGate` /
  `TraceGate` (vocabulary in ModelQuality.sysml — thresholds as
  `default` attributes the usage overrides with `:>>`; the whole
  pattern is OMG-pilot-validated). No gate declared = strict. A
  constraint def's bare body expression is now extracted like nested
  asserts, so W017 and gates see `constraint def G { in x : Real;
  x >= 0.0 }` bodies

### Monte Carlo histogram
- 'analyze run' Monte Carlo results include the sample distribution:
  21 equal-width bins in JSON, an ASCII histogram in text output with
  LSL/USL markers when a spec limit falls inside the sampled range.
  Same seed, same histogram - it is part of the audit trail

### Fixed
- 'verify' statements now accept feature-chain targets
  ('verify overpressureProtection.minTravel') - nested requirements
  were silently dropped from verification tracing
- Wizard-test flake: MockRunner call counter is per-instance

### Docs
- The build-a-model-from-scratch tutorial is gone - write models in
  your editor. docs/analyzing-models.md replaces it: a real model run
  through the analysis commands with actual outputs (views, uncertainty
  methods with the histogram, trace/coverage with model-side gates)

### Removed (breaking)
- `index` and the cache stack (`cache.rs`, `index.rs`,
  `sqlite_cache.rs`, the `sqlite` feature — ~2.4k lines): the cache had
  one writer and no readers; whole-project parses take milliseconds
- `doc` is marked transitional in its help text (the fixed layout will
  move into the model as document views/templates)

## 0.7.0 — 2026-08-14

The lean-CLI release: reports are model-defined views, analysis is
generic uncertainty propagation, validation rules live in the libraries,
and eight overlapping commands are gone. Grammar pinned at
tree-sitter-sysml v0.6.0 (33% smaller parser, all examples parse clean).

### sysml view — reports are models
- New `sysml view <Name>` renders any `view def` carrying a
  `@TableRendering` annotation (the Reporting library convention:
  row providers, computed columns, `where`, `sortBy`, `pivot`) as a
  table; with no name it lists available views. `-f csv|md` join
  text/json for tabular output. The libraries ship `FmeaWorksheet`,
  `RiskMatrix`, `HazardLog`, `StackupSummary`, `FitTable`, `PortTable`,
  `ConnectionTable`, `AllocationMatrix`, `RequirementsTraceMatrix`,
  and `ModelStats` — projects add reports by writing view defs, never
  tool code
- `list --doc <text>` filters by documentation text; `diagram --view X`
  no longer requires `-t` when the view def declares `render as`

### CLI surface consolidation (breaking)
- Removed commands — each replaced by an existing or model-defined
  equivalent: `lint` (`check`), `interfaces` (`view PortTable`),
  `stats` (`view ModelStats`), `find` (`list -n` / `list --doc`),
  `rollup query` (`list -k attributes -n <attr>`), `export interfaces`,
  `guide`, and `pipeline` (chain commands in make/just/CI — every gate
  already exits non-zero)
- Removed flags: `check --lint-only`, `add -i/--interactive`,
  `index --full`, `diagram -o` (renderer is now `-r/--renderer`; `-o`
  is freed for output files), `rollup what-if -s` (use `--scenario`)

### Value-constraint evaluation (W017)
- `check` now evaluates `assert constraint`s against concrete model
  values: `@Fmea { severity = 12; }` is flagged because `FmeaRating`
  asserts `that >= 1 and that <= 10`, and a `LimitRange` with
  `lower > nominal` is flagged by its `wellOrdered` constraint. Rules
  live in the libraries; the checker is generic. Disable with
  `-d value-constraints`.
- Named assert constraints now carry their body expression in the model
  (previously only anonymous ones did)

### Robustness
- One shared model loader: every whole-project command honors
  `-I`/`--stdlib-path`/`.sysml/config.toml` (24 of 26 silently ignored
  them), merges all model fields, and errors on unreadable files;
  config discovery walks up the directory tree
- Loud failures: rollup unit-conversion mismatches are errors instead
  of silently adding grams to kilograms; guard/condition evaluation
  errors in simulations warn instead of acting as `false`;
  `--format` typos are rejected; `simulate eval` exits non-zero on
  violated constraints; interactive event prompts abort on non-TTY
  instead of simulating zero events
- `check -` and `fmt -` read from stdin (`fmt -` writes the formatted
  text to stdout); `--method` now works on `rollup
  sensitivity`/`sweep`/`what-if`, which aggregate through rollup's
  unit-aware path

### Generic uncertainty analyzer
- `sysml analyze run` now evaluates any analysis case whose type
  specializes `Uncertainty::UncertaintyAnalysis` (e.g.
  `Tolerancing::ToleranceStackup`) by uncertainty propagation:
  worst-case interval arithmetic, RSS variance propagation (Cp/Cpk,
  per-contribution sensitivity, Bender mean shift, yield), and seeded
  bit-for-bit-reproducible Monte Carlo (`--method`, `--iterations`,
  `--seed`; the seed used is always reported). Contributions are
  resolved through feature chains to the dimensions that own the
  values — nothing is restated in the analysis. Exit code is non-zero
  when any evaluated method fails its target, for CI gating.
- Parser: standalone redefinition statements (`:>> nominal = 50.0;`)
  are now extracted as model usages with values — the value style the
  domain libraries use throughout.
- `rollup` no longer panics on NaN subtotals when sorting
  contributions (sensitivity and compute paths).

### Name resolution & checks
- Imports expose package members, not just definitions: subsetting or
  redefining an inherited member of an imported def
  (`attribute x :> contributions`) no longer raises a false W004
- Requirement coverage (W002/W003/W014) is project-wide and closed over
  specialization: a requirement def satisfied/verified in a sibling file —
  including through a usage of a specializing def (`Derived :> Base`) —
  counts as traced
- W013 (naming) no longer flags metadata annotations (`@Fmea { ... }`),
  whose usages take the metadata def's PascalCase name by design

### Domain libraries
- `libraries/` now ships the engineering-analysis packages from the
  sysml-domain-libraries project — `Uncertainty`, `Tolerancing`,
  `RiskAnalysis`, and `HazardAnalysis` (RAAML-Core-aligned hazard chains
  driving FMEA severity) — replacing the earlier `sysml-tolerance` and
  `sysml-risk` sketches

## 0.6.0 — 2026-08-05

The book-review release: every finding from the systematic review
against the *SysML v2 Book* (Weilkiens, 2026-05) is fixed. 165 of the
book's 169 code listings parse cleanly (the remaining 4 are fragments/
errata in the book itself).

### Language & parsing
- Grammar updated to tree-sitter-sysml v0.5.0: 14 language gaps closed
  (function-literal body expressions, `then perform`, `accept when/at`,
  send transition effects, exhibit declarations, else-if action chains,
  `@`/`@@` classification operators, qualified metadata bodies, typed
  for-loops, and more); modifiers are named CST nodes
- MISSING-node syntax errors are reported; precise type-reference spans

### Name resolution & checks
- SysML v2 root-namespace resolution: fully-qualified cross-file
  references resolve without imports; package short names
  (`package <LIB> 'Library Package'`) work everywhere
- Stdlib member validation (`ISQ::doesNotExist` now warns); W001 is
  cross-file aware; E002 is package-scoped; clearer W004 wording
- `list --metadata TYPE --where key=value` metadata queries (Ch 36)

### Requirements & analysis
- `trace` treats requirement usages with `<ID>` short names as
  first-class rows; satisfy/verify match by name, qualified name,
  feature chain, or ID; `show <file> REQ2` looks up by ID
- `doc` emits requirement statements with IDs
- `analyze run` solves constraint systems by substitution or errors
  with the unbound variables; assert-constraint usages are extracted
  (also enabling `simulate eval` on them)

### Values, variants & simulation
- Unit brackets (`250 [SI::kg]`) parse everywhere; rollups convert
  mixed units and display the result unit
- Variants are first-class: `list --variations/--variants`,
  `rollup compute --variant POINT=CHOICE` (Ch 35)
- Action-flow simulation rewritten with source-order succession
  semantics: one decide branch executes, forks run once, nothing runs
  before `start`
- Parallel state regions simulate concurrently with broadcast events;
  `transition A then B;` source shorthand supported
- `diagram --view` errors on unknown views instead of silently
  rendering everything

### Language server
- Workspace-wide rename (closed files, imports, satisfy/connect/flow
  references); precise quickfix spans; UTF-16 position encoding
- Embedded stdlib visible to completion (`ISQ::` scoped, type position
  ranked below workspace) and hover
- Migrated to tower-lsp-server 0.23; type hierarchy advertised via
  LSP 3.17 `typeHierarchyProvider` (vendored ls-types patch — see
  third_party/ls-types/PATCH.md)

### Removed
- Abandoned lifecycle crates (bom/capa/mfg/qc/risk/scaffold/source/
  tol/verify/report) and their documentation

### CI
- Workflows check out submodules — release binaries now actually embed
  the standard library and the pinned grammar

## 0.5.0 and earlier

See git history.
