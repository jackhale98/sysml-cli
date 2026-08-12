# Changelog

## Unreleased

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
