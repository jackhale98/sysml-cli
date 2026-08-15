# Editor Setup

Full SysML v2 editing takes two pieces, both from this project family:

1. **[tree-sitter-sysml](https://github.com/jackhale98/tree-sitter-sysml)**
   — the grammar: syntax highlighting, indentation, folding, structural
   navigation. The first (and to our knowledge only) tree-sitter grammar
   for SysML v2 textual notation; parses the complete official OMG
   corpus.
2. **`sysml-lsp`** (this repo) — the language server: diagnostics,
   go-to-definition, references, hover with rollup values, completions
   (standard-library-aware), rename, code lens, and the rest of its 17
   capabilities.

They are independent — an editor can use either alone — but they are
built for each other: the LSP embeds the same grammar for parsing, so
what highlights is what analyzes.

Install the server once, for any editor:

```sh
cargo install --path crates/sysml-lsp     # from a sysml-cli checkout
# or download a prebuilt binary from GitHub Releases
```

## Emacs

The easy path is **[sysml2-mode](https://github.com/jackhale98/sysml2-mode)**,
which bundles everything: font-lock with a regex fallback, tree-sitter
activation when the grammar is installed, LSP auto-configuration for
both eglot and lsp-mode (with `sysml-lsp` as the default server), plus
Flymake, diagrams, org-babel, and CLI integration.

```elisp
;; 1. Install the grammar (once; Emacs 29.1+)
(add-to-list 'treesit-language-source-alist
             '(sysml "https://github.com/jackhale98/tree-sitter-sysml" nil "src"))
(treesit-install-language-grammar 'sysml)

;; 2. Install sysml2-mode (from a checkout)
(add-to-list 'load-path "/path/to/sysml-mode")
(require 'sysml2-mode)
;; .sysml / .kerml files now get the mode; with sysml-lsp on PATH,
;; eglot (or lsp-mode) connects automatically.
```

Without sysml2-mode, a minimal eglot setup against any prog-mode:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(my-sysml-mode . ("sysml-lsp" "--stdio"))))
```

## Vim / Neovim

**Grammar** (Neovim, via nvim-treesitter):

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.sysml = {
  install_info = {
    url = "https://github.com/jackhale98/tree-sitter-sysml",
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "sysml",
}
-- then :TSInstall sysml
```

Copy the grammar repo's `queries/` directory to
`~/.config/nvim/queries/sysml/` for highlights, folds, and indents.

**Filetype detection and LSP:**

```lua
vim.filetype.add({
  extension = { sysml = "sysml", kerml = "sysml" },
})

vim.api.nvim_create_autocmd("FileType", {
  pattern = { "sysml" },
  callback = function()
    vim.lsp.start({
      name = "sysml-lsp",
      cmd = { "sysml-lsp" },
      root_dir = vim.fs.dirname(
        vim.fs.find({ ".sysml", ".git" }, { upward = true })[1]
      ),
    })
  end,
})
```

Classic Vim has no tree-sitter; use an LSP client plugin (vim-lsp,
coc.nvim) pointed at the `sysml-lsp` binary over stdio, and highlighting
comes from the server's semantic tokens where the client supports them.

## VS Code

VS Code does not consume tree-sitter grammars directly — highlighting
comes from the language server's **semantic tokens**, which sysml-lsp
generates from the same grammar queries. Configure a generic LSP client
extension to launch `sysml-lsp` via stdio for `.sysml`/`.kerml`:

```jsonc
// with a generic LSP client extension
{
  "genericLSP.serverCommand": "sysml-lsp",
  "genericLSP.languageId": "sysml",
  "genericLSP.fileExtensions": [".sysml", ".kerml"]
}
```

For a dedicated extension, the server side is done — `sysml-lsp` is a
standard stdio language server; the extension only needs to declare the
language and spawn the binary.

## Helix

Helix has native tree-sitter support: copy the grammar repo's `queries/`
to your runtime directory as `sysml/`, then in
`~/.config/helix/languages.toml`:

```toml
[[language]]
name = "sysml"
scope = "source.sysml"
file-types = ["sysml", "kerml"]
language-servers = ["sysml-lsp"]

[language-server.sysml-lsp]
command = "sysml-lsp"
```

## Zed

```jsonc
// settings.json
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

## Capabilities

See [CI & Editor Integration](ci-integration.md#capabilities) for the
full capability table. Two behaviors worth knowing in any editor:

- Diagnostics run the same 16 checks as `sysml check` (W017
  value-constraint evaluation is CLI-only), with cross-file resolution
  built from a workspace scan on initialize.
- Completions are standard-library-aware: `attribute m : ` offers
  `Real` and ISQ quantities ranked below your workspace types, and
  `ISQ::` lists the package members.
