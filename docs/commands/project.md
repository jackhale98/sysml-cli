# Project Commands

Commands for project initialization, indexing, pipelines, cross-domain reporting, help, and shell completions.

## init

Initialize a SysML project by creating a `.sysml/` directory with a `config.toml`.

```sh
sysml init
sysml init --force    # Overwrite existing config
```

| Option | Description |
|--------|-------------|
| `--force` | Overwrite existing `.sysml/config.toml` if present |

Creates:
- `.sysml/config.toml` — project configuration (name, model root, library paths, defaults)
- Adds `.sysml/cache.db*` to `.gitignore`

The config file supports:

```toml
[project]
name = "BrakeSystem"
model_root = "model/"
library_paths = ["libraries/"]

[defaults]
author = "jhale"
output_dir = "records/"
format = "text"
```

**Precedence:** CLI flags > env vars (`SYSML_MODEL_ROOT`, etc.) > config file > defaults.

## index

Build or rebuild the project index (in-memory cache).

```sh
sysml index
sysml index --stats    # Show index statistics
```

| Option | Description |
|--------|-------------|
| `--full <BOOL>` | Rebuild everything including records (default: true) |
| `--stats` | Show index statistics after building |

The index accelerates cross-file queries. All commands work without it — the index is a performance optimization.

When built with the `sqlite` feature, `sysml index` also persists the cache to `.sysml/cache.db`. The SQLite cache stores a git HEAD hash so stale caches can be detected automatically.

## pipeline

Run named validation pipelines defined in `.sysml/config.toml`. Pipelines are sequences of sysml commands that run in order, stopping on the first failure.

```sh
sysml pipeline list                       # List all defined pipelines
sysml pipeline run ci                     # Run the "ci" pipeline
sysml pipeline run ci --dry-run           # Preview without executing
sysml pipeline create pre-commit          # Create a new pipeline with example steps
```

### pipeline list

Show all pipelines defined in the project config.

### pipeline run

Run a named pipeline. Each step is executed as a separate `sysml` invocation.

| Option | Description |
|--------|-------------|
| `<NAME>` | Pipeline name to run (required) |
| `--dry-run` | Preview commands without executing them |

### pipeline create

Add a new pipeline to `.sysml/config.toml` with example steps (`lint *.sysml`, `fmt --check *.sysml`). Edit the config file to customize.

| Option | Description |
|--------|-------------|
| `<NAME>` | Pipeline name (required) |

### Config format

Pipelines are defined as `[[pipeline]]` entries in `.sysml/config.toml`:

```toml
[[pipeline]]
name = "ci"
steps = ["lint *.sysml", "fmt --check *.sysml"]

[[pipeline]]
name = "pre-commit"
steps = ["lint *.sysml", "check *.sysml"]
```

## report

Cross-domain reports that aggregate data from all lifecycle domains.

### report dashboard

Project health summary: requirement coverage, open risks, NCR counts, BOM status.

```sh
sysml report dashboard model.sysml
sysml report dashboard -f json model.sysml
```

### report traceability

Full lifecycle thread for a requirement: satisfaction, verification, parts, risks, manufacturing, quality.

```sh
sysml report traceability model.sysml --requirement SafetyReq
```

| Option | Description |
|--------|-------------|
| `--requirement <REQ>` | Requirement name to trace (required) |

### report gate

Design review readiness check against milestone criteria.

```sh
sysml report gate model.sysml --gate-name PDR
sysml report gate model.sysml --gate-name CDR --min-coverage 90
```

| Option | Description |
|--------|-------------|
| `--gate-name <NAME>` | Gate name: `SRR`, `PDR`, `CDR`, `TRR`, `FAR`, `PRR` (required) |
| `--min-coverage <PCT>` | Minimum verification coverage % (default: 80.0) |

Resolves `MilestoneDef` from the project library, checks verification coverage thresholds, counts open critical risks and NCRs, and produces a pass/fail readiness report.

## guide

Built-in help topics and tutorials. Displays explanatory articles about systems engineering concepts in context of the tool.

```sh
sysml guide                      # List all topics
sysml guide getting-started      # Step-by-step tutorial
sysml guide mbse                 # MBSE overview
```

Available topics include: `getting-started`, `mbse`, `sysml-basics`, `requirements`, `verification`, `risk-management`, `tolerance`, and more.

## completions

Generate shell completion scripts.

```sh
sysml completions bash
sysml completions zsh
sysml completions fish
sysml completions elvish
sysml completions powershell
```

Install completions:

```sh
# Bash
sysml completions bash > ~/.local/share/bash-completion/completions/sysml

# Zsh
sysml completions zsh > ~/.zfunc/_sysml

# Fish
sysml completions fish > ~/.config/fish/completions/sysml.fish
```
