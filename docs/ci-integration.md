# CI & Editor Integration

## GitHub Actions

```yaml
name: SysML Lint
on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install sysml
        run: |
          git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git /tmp/sysml-cli
          cd /tmp/sysml-cli
          cargo build --release
          echo "/tmp/sysml-cli/target/release" >> $GITHUB_PATH

      - name: Initialize project
        run: sysml init --force

      - name: Lint models
        run: sysml lint --severity warning models/**/*.sysml

      - name: Check formatting
        run: sysml fmt --check models/**/*.sysml

      - name: Check requirement coverage
        run: sysml trace --check --min-coverage 80 models/**/*.sysml

      - name: Check model quality
        run: sysml coverage --check --min-score 70 models/**/*.sysml

      - name: Check allocations
        run: sysml allocation --check models/**/*.sysml
```

### CI gate commands

| Command | Purpose | Exit code |
|---------|---------|-----------|
| `sysml lint --severity error` | Block on syntax/duplicate errors | 1 if errors |
| `sysml fmt --check` | Enforce formatting | 1 if unformatted |
| `sysml trace --check --min-coverage 80` | Require requirement coverage | 1 if below threshold |
| `sysml coverage --check --min-score 70` | Require model quality score | 1 if below threshold |
| `sysml allocation --check` | Require all allocations | 1 if gaps exist |

## Editor Integration

### Language Server (`sysml-lsp`)

`sysml-lsp` is a full-featured language server for SysML v2 files with 17 capabilities. Install from source or download a prebuilt binary from [GitHub Releases](https://github.com/jackhale98/sysml-cli/releases).

```sh
cargo install --path crates/sysml-lsp
```

#### VS Code

Install a generic LSP client extension (e.g., [vscode-languageclient](https://github.com/ArturoManzoli/generic-lsp-client) or create a minimal extension), then configure it to launch `sysml-lsp` via stdio for `.sysml` and `.kerml` files:

```jsonc
// settings.json
{
  "sysml.lsp.path": "sysml-lsp"
}
```

Or with the generic LSP client:

```jsonc
{
  "genericLSP.serverCommand": "sysml-lsp",
  "genericLSP.languageId": "sysml",
  "genericLSP.fileExtensions": [".sysml", ".kerml"]
}
```

#### Neovim

```lua
-- init.lua or ftplugin/sysml.lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = { 'sysml' },
  callback = function()
    vim.lsp.start({
      name = 'sysml-lsp',
      cmd = { 'sysml-lsp' },
      root_dir = vim.fs.dirname(vim.fs.find({ '.sysml', '.git' }, { upward = true })[1]),
    })
  end,
})
```

Add filetype detection if needed:

```lua
vim.filetype.add({
  extension = {
    sysml = 'sysml',
    kerml = 'sysml',
  },
})
```

#### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "sysml"
scope = "source.sysml"
file-types = ["sysml", "kerml"]
language-servers = ["sysml-lsp"]

[language-server.sysml-lsp]
command = "sysml-lsp"
```

#### Zed

Add to Zed settings (`settings.json`):

```jsonc
{
  "lsp": {
    "sysml-lsp": {
      "binary": { "path": "sysml-lsp" }
    }
  },
  "languages": {
    "SysML": {
      "language_servers": ["sysml-lsp"]
    }
  }
}
```

#### Capabilities

| Feature | Description |
|---------|-------------|
| Diagnostics | 12 lint checks with error codes, severity, suggestions — published on open/change |
| Document symbols | Hierarchical outline (definitions as containers, usages as children) |
| Go-to-definition | In-file and cross-file navigation via workspace definition index |
| Find references | All references to a name across open files (type refs, supertypes, connections, flows) |
| Hover | Markdown with kind, name, supertype, doc comment, member list, computed rollup values |
| Completions | Context-aware: type defs after `:`, members after `.`, all defs + stdlib otherwise |
| Workspace symbols | Filter all workspace definitions by query (Ctrl+T / `#` in VS Code) |
| Semantic tokens | Full syntax highlighting via tree-sitter queries (keywords, types, variables, comments, operators) |
| Code actions | Quick-fix typos, remove unused definitions, add missing imports |
| Formatting | CST-aware document formatting respecting editor tab size (preserves comments) |
| Document highlight | Highlight all occurrences of the symbol under cursor |
| Folding ranges | Fold definition blocks and multi-line doc comments |
| Rename | Cross-file symbol rename with word-boundary matching (F2) |
| Type hierarchy | Navigate supertype/subtype chains across files |
| Inlay hints | Show multiplicity annotations and inferred types inline |

### Emacs (sysml2-mode)

`sysml` integrates with [sysml2-mode](https://github.com/jackhale98/sysml2-mode) for Flymake diagnostics, interactive simulation, and FMI export. With `sysml` on your `$PATH`:

- **Flymake**: Diagnostics appear inline as you edit
- **Simulation**: `M-x sysml2-simulate` for constraints, state machines, action flows
- **FMI Export**: `M-x sysml2-fmi-extract-interfaces` extracts interfaces
- **Diagrams**: `M-x sysml2-diagram` generates diagrams inline

### JSON output for other editors

All commands support `-f json` for structured output suitable for editor integration:

```sh
sysml lint -f json model.sysml          # Diagnostics as JSON array
sysml list -f json model.sysml          # Element list as JSON
sysml simulate list -f json model.sysml # Simulatable items as JSON
```

This works with any editor that can parse JSON from a subprocess — VS Code extensions, Neovim plugins, etc.

## Library resolution in CI

If your project has `.sysml/config.toml` with `library_paths` configured (set automatically by `sysml init` when `libraries/` exists), all commands resolve imports automatically — no `-I` flag needed in CI steps.

For projects without a config, run `sysml init --force` as a CI step (see GitHub Actions example above) or pass `-I` explicitly.
