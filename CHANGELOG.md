# Changelog

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
