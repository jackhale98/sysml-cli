# Project Commands

Commands for project initialization, indexing, and shell completions.

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
- Auto-detects `libraries/` directory and adds it to `library_paths`
- Auto-detects `model/` directory and sets `model_root`

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

**Library auto-resolution:** When `library_paths` is set in config, all commands automatically include those paths for import resolution — no `-I` flag needed.

**Precedence:** CLI flags > env vars (`SYSML_MODEL_ROOT`, etc.) > config file > defaults.

## Validation pipelines

There is no built-in pipeline runner — compose `sysml` commands with
make, just, or your CI configuration directly:

```make
check:
	sysml check --severity warning src/*.sysml
	sysml fmt --check src/*.sysml
	sysml trace --check src/*.sysml
	sysml coverage --check src/*.sysml
```

See [CI & Editor Integration](../ci-integration.md) for a GitHub Actions example.

## Reports

Cross-domain reporting is model-defined: `sysml view` renders any view
def carrying a `@TableRendering` annotation as a table (see
[views](views.md)). Requirement status comes from `sysml trace`, model
quality from `sysml coverage`.

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
