# Validation & Diagnostics

## Validation Checks

`sysml` ships with 12 validation checks. Each can be individually disabled with `--disable <name>`.

| Check | Name | Severity | Description |
|-------|------|----------|-------------|
| Syntax | `syntax` | Error | Tree-sitter parse errors and missing syntax elements |
| Duplicates | `duplicates` | Error | Definitions of the same kind with identical names |
| Unused | `unused` | Note | Definitions never referenced in the file |
| Unresolved | `unresolved` | Warning | Type references and targets that don't resolve |
| Unsatisfied | `unsatisfied` | Warning | Requirements with no `satisfy` statement |
| Unverified | `unverified` | Warning | Requirements with no `verify` statement |
| Port Types | `port-types` | Warning | Connected ports with incompatible types |
| Port Directions | `port-types` | Warning | Connected ports with incompatible directions (in/out) |
| Constraints | `constraints` | Warning | Constraint defs with a body but no constraint expression |
| Calculations | `calculations` | Warning | Calc defs with a body but no return statement |
| Import Cycles | `import-cycles` | Warning | Self-imports, bidirectional, and transitive import cycles |
| Multiplicity | `multiplicity` | Warning | Invalid multiplicity bounds (lower > upper, zero upper, negative) |

## Diagnostic Codes

### Errors

| Code | Check | Message |
|------|-------|---------|
| E001 | syntax | `Syntax error: near <context>` |
| E002 | duplicates | `duplicate <kind> '<name>' (first defined at line <n>)` |

### Warnings

| Code | Check | Message |
|------|-------|---------|
| W001 | unused | `<kind> '<name>' is defined but never referenced` |
| W002 | unsatisfied | `requirement def '<name>' has no corresponding satisfy statement` |
| W003 | unverified | `requirement def '<name>' has no corresponding verify statement` |
| W004 | unresolved | `type '<name>' is not defined in this file` |
| W005 | unresolved | `reference '<name>' does not resolve to any definition or usage` |
| W006 | port-types | `connected ports have different types` |
| W007 | constraints | `constraint def '<name>' has a body but no constraint expression` |
| W008 | calculations | `calc def '<name>' has a body but no return statement` |
| W009 | port-types | `connected ports have incompatible directions` |
| W010 | import-cycles | `package '<name>' imports itself` / `circular import` |
| W011 | multiplicity | `multiplicity lower bound exceeds upper bound` |

## Output Formats

### Text (default)

```
model.sysml:12:5: warning[W002]: requirement def `MassReq` has no corresponding satisfy statement
```

### JSON

```json
[
  {
    "file": "model.sysml",
    "span": { "start_row": 12, "start_col": 5 },
    "severity": "warning",
    "code": "W002",
    "message": "requirement def `MassReq` has no corresponding satisfy statement"
  }
]
```

All commands support `-f json` for structured output suitable for editor integration and CI pipelines.
