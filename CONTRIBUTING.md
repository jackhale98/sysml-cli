# Contributing to sysml-lint

## Adding a New Check

1. Create a new file in `src/checks/` (e.g., `src/checks/mycheck.rs`).

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

3. Add a diagnostic code in `src/diagnostic.rs`:

```rust
pub mod codes {
    // ...existing codes...
    pub const MY_CHECK_CODE: &str = "W009";
}
```

4. Register the check in `src/checks/mod.rs`:

```rust
mod mycheck;

pub fn all_checks() -> Vec<Box<dyn Check>> {
    vec![
        // ...existing checks...
        Box::new(mycheck::MyCheck),
    ]
}
```

5. Update the `--disable` help text in `src/main.rs` and add tests.

## Diagnostic Severity Guidelines

- **Error**: Definitely wrong. Parse errors, duplicate definitions.
- **Warning**: Likely a problem. Missing traceability, unresolved types, type mismatches.
- **Note**: Informational. Unused definitions that may be intentional.

## Code Conventions

- Each check is a separate file in `src/checks/`.
- Use `simple_name()` from `model.rs` when comparing names (handles qualified paths and feature chains).
- Diagnostics should have clear, actionable messages that include the relevant identifiers.
- Error codes follow the pattern: `E` for errors, `W` for warnings, numbered sequentially.

## Running Tests

```sh
cargo test              # All tests
cargo test -- --nocapture  # With stdout
```

## Testing Against Fixtures

```sh
# Run against all fixture files
cargo run -- test/fixtures/*.sysml

# Specific severity
cargo run -- --severity error test/fixtures/annex-a-simple-vehicle-model.sysml
```
