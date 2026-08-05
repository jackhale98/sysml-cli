# Vendored ls-types 0.0.6 with one patch

This is ls-types 0.0.6 (MIT, from crates.io — the lsp-types fork used by
tower-lsp-server) vendored with a single addition:

- `ServerCapabilities.type_hierarchy_provider` — in the LSP 3.17
  specification (`typeHierarchyProvider`) but missing from every
  published lsp-types/ls-types release, which makes it impossible for
  servers to advertise type hierarchy support through the typed API.

Wired via `[patch.crates-io]` in the workspace Cargo.toml. Remove the
vendored copy once the field lands upstream
(https://github.com/tower-lsp-community).
