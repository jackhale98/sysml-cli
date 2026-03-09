# Contributing to sysml-cli

## Project Structure

```
crates/
  sysml-core/      Core library (parser, model, checks, sim, export, codegen)
  sysml-cli/       CLI frontend (clap commands, output formatting)
tree-sitter-sysml/ Grammar (git submodule)
test/fixtures/     SysML v2 test files
```

## Adding a New Check

1. Create a new file in `crates/sysml-core/src/checks/` (e.g., `mycheck.rs`).

2. Implement the `Check` trait:

```rust
use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::Model;

pub struct MyCheck;

impl Check for MyCheck {
    fn name(&self) -> &'static str {
        "my-check"  // Used with --disable
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // ... validation logic ...
        diagnostics
    }
}
```

3. Add a diagnostic code in `crates/sysml-core/src/diagnostic.rs`:

```rust
pub mod codes {
    // ...existing codes...
    pub const MY_CHECK_CODE: &str = "W009";
}
```

4. Register the check in `crates/sysml-core/src/checks/mod.rs`:

```rust
mod mycheck;

pub fn all_checks() -> Vec<Box<dyn Check>> {
    vec![
        // ...existing checks...
        Box::new(mycheck::MyCheck),
    ]
}
```

5. Update the `--disable` help text in the CLI and add tests.

## Diagnostic Severity Guidelines

- **Error**: Definitely wrong. Parse errors, duplicate definitions.
- **Warning**: Likely a problem. Missing traceability, unresolved types, type mismatches.
- **Note**: Informational. Unused definitions that may be intentional.

## Code Conventions

- Core library (`sysml-core`) is frontend-agnostic — no I/O, no CLI dependencies.
- CLI (`sysml-cli`) is a thin frontend over core.
- Each check is a separate file in `checks/`.
- Use `simple_name()` from `model.rs` when comparing names.
- Diagnostics should have clear, actionable messages with relevant identifiers.
- Error codes: `E` for errors, `W` for warnings, numbered sequentially.

## Running Tests

```sh
cargo test                   # All tests (unit + integration + CLI)
cargo test -p sysml-core     # Core library tests only
cargo test -p sysml-cli      # CLI integration tests only
cargo test -- --nocapture    # With stdout/stderr output
```

## Testing Against Fixtures

```sh
# Run against all fixture files
cargo run -p sysml-cli -- lint test/fixtures/*.sysml

# Specific severity
cargo run -p sysml-cli -- lint --severity error test/fixtures/annex-a-simple-vehicle-model.sysml
```
