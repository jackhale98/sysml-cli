# CLI audit — 2026-08 (living checklist)

Full audit of the command surface against the project philosophy (lean CLI,
generic primitives, semantics in the model). No backwards-compatibility
constraint (pre-release). Items are checked off as they land.

## A. Cross-cutting fixes

- [x] One shared `load_model` / `load_per_file_models` honoring `-I`,
      `--stdlib-path`, and `.sysml/config.toml` uniformly (replaces 10
      divergent per-command merges, each missing different fields)
- [x] `--format` validated (`text|json`), instead of silent fallback
- [x] `-q` suppresses all non-essential chatter, including project-discovery
      notices
- [x] Flag normalization: `-s` only for `--severity`; `-d` only for
      `--disable`; diagram renderer flag no longer collides with export's
      output-file `-o`
- [x] Unreadable files are always an error (doc/find/repl silently skipped)
- [x] `simulate eval` exits non-zero on violated constraints
- [x] Guard-expression evaluation errors warn instead of silently acting
      as `false` (action/state simulation)
- [x] Unit-conversion failures in rollup are errors, not silent
      pass-through of unconverted values
- [x] `--method` on all rollup subcommands (sensitivity/sweep/what-if
      hard-coded Sum)
- [x] `what_if` aggregation unified with rollup's (unit-aware); the two
      disagreed on mixed-unit models
- [x] `analysis.rs` evaluates `value_expr` through the expression parser
      (was `str::parse::<f64>` — `mass * 2` computed nothing)
- [x] `prompt_events` aborts on non-TTY instead of simulating zero events
- [x] `render as` honored: `diagram --view X` defaults the type from the
      view def (`-t` optional)
- [x] stdin (`-`) support for `check` and `fmt`
- [x] NaN-safe sorting in rollup (landed with the analyzer)

## B. Dead surface — delete

- [x] `lint` (byte-identical to `check`)
- [x] `check --lint-only` (parsed, never read)
- [x] `add -i/--interactive` (parsed, never read)
- [x] `index --full` (defaults true, cannot be false)
- [x] `guide` + `help_topics.rs` (hidden, stale, documents nonexistent
      commands, contradicts `--help`)
- [x] `pipeline` (task runner with broken quoting; use make/just/CI)
- [x] `export interfaces` (debug view of modelica/ssp input)
- [x] `rollup query` (≡ `list -k attributes -n <attr>`)
- [x] `find` (strict subset of `list`; doc-substring filter on `list` still pending — Phase C)

## C. Views replace report commands

- [x] `sysml view <ViewName>` — generic renderer: expose/filter, row
      providers (metadata annotations, typed usages, relations,
      uncertainty results), columns/sort/pivot via `@TableRendering`
- [x] Library view defs get real bodies (FmeaWorksheet, RiskMatrix,
      HazardLog, StackupSummary, FitTable)
- [x] `StandardViews.sysml`: PortTable, AllocationMatrix,
      RequirementsTraceMatrix, ModelStats
- [ ] `interfaces` deleted (→ view PortTable)
- [ ] `stats` deleted (→ view ModelStats)
- [ ] `trace` / `coverage` / `allocation` render through the view engine;
      `--check` gates retained on the commands until model-declared
      constraints are evaluated by `check` (then the gates move into
      the model)
- [ ] `doc` marked transitional (→ document views/templates later)

## D. Evaluator consolidation (beyond the analyzer)

- [x] Parser extracts `:>> name = value;` statements (landed)
- [x] Uncertainty analyzer on one extraction path (landed)
- [ ] `simulate eval` and `analyze run` share the solve path (two
      evaluators with different capabilities today)
- [ ] REPL dispatches into the real command implementations (771-line
      parallel CLI with divergent filter semantics)

## E. Docs truth

- [ ] Help examples say `sysml`, not `sysml-cli` (6 commands)
- [ ] EXAMPLES blocks for stats/interfaces/export/list/show/trace/index
      (as they survive)
- [ ] `docs/tutorial.md`: remove `risk`/`bom`/`verify`/`report` families
      (~20 commands that don't exist)
- [ ] Retire `cli_expansion.md` domain-subcommand spec (contradicts the
      lean-CLI direction)
- [ ] `docs/commands/analysis.md`: `lint` → `check`

## Deferred (tracked, deliberately not now)

- JSON output envelope with a `command` discriminator on every command
  (do per-command as they are touched)
- rustc-style spans on all error messages (only `check` has spans today)
- `coverage` weights → `ModelQuality.sysml` calc defs (needs constraint
  evaluation in `check` first)
- Cache read path (`index` builds a cache nothing reads)
- `record.rs` / `project_model.rs` (1.5k lines unreachable) — keep or cut
  with the record-tooling decision
