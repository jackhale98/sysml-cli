# Validation & Diagnostics

## Validation Checks

`sysml` ships with 17 validation checks. Each can be individually disabled with `--disable <name>`.

| Check | Name | Severity | Description |
|-------|------|----------|-------------|
| Value Constraints | `value-constraints` | Warning | Model-declared `assert constraint`s evaluated against concrete values (W017) |
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
| Missing Docs | `missing-docs` | Note | Public top-level definitions without a `doc /* ... */` block |
| Naming | `naming` | Note | Definitions not in PascalCase, usages not in camelCase |
| Orphan Req | `orphan-req` | Warning | Requirement defs that are never satisfied, verified, or specialized |
| Self-Specialization | `self-specialization` | Error | A definition naming itself as its own super-type (`part def X :> X`) |
| Unbound Port | `unbound-port` | Warning | Port usages declared inside a part but never connected |

## Diagnostic Codes

### Errors

| Code | Check | Message |
|------|-------|---------|
| E001 | syntax | `Syntax error: near <context>` |
| E002 | duplicates | `duplicate <kind> '<name>' (first defined at line <n>)` |

### Errors (continued)

| Code | Check | Message |
|------|-------|---------|
| W015 | self-specialization | `<kind> '<name>' specializes itself: ':> <name>' would cause infinite recursion` — an Error despite the W code: it fails `sysml check` |

### Warnings

| Code | Check | Message |
|------|-------|---------|
| W001 | unused | `<kind> '<name>' is defined but never referenced` |
| W002 | unsatisfied | `requirement def '<name>' has no corresponding satisfy statement` |
| W003 | unverified | `requirement def '<name>' has no corresponding verify statement` |
| W004 | unresolved | `type '<name>' cannot be resolved (not defined, imported, or reachable from the root namespace)` |
| W005 | unresolved | `reference '<name>' does not resolve to any definition or usage` |
| W006 | port-types | `connected ports have different types` |
| W007 | constraints | `constraint def '<name>' has a body but no constraint expression` |
| W008 | calculations | `calc def '<name>' has a body but no return statement` |
| W009 | port-types | `connected ports have incompatible directions` |
| W010 | import-cycles | `package '<name>' imports itself` / `circular import` |
| W011 | multiplicity | `multiplicity lower bound exceeds upper bound` |
| W012 | missing-docs | `<kind> '<name>' has no documentation comment` |
| W013 | naming | `<kind> name '<name>' should start with an uppercase letter (PascalCase)` |
| W014 | orphan-req | `requirement def '<name>' is never satisfied, verified, or specialized` |
| W016 | unbound-port | `port '<name>' (in '<parent>') is declared but never connected` |
| W017 | value-constraints | `<value> violates constraint '<name>' of '<type>' (<expression>)` |

### Multi-file semantics

When several files are checked in one invocation (or `-I` include paths
are given), they share a root namespace, per SysML v2 name resolution:

- Fully-qualified references (`LIB::Widget`) resolve without an import.
- Package short names (`package <LIB> 'Library Package'`) resolve in
  imports and references, quoted or not.
- Imports expose package *members* as well as definitions: subsetting or
  redefining an inherited member of an imported def
  (`attribute x :> contributions` where `contributions` lives on an
  imported `analysis def`) resolves without a W004.
- W001 (unused) counts references from sibling files as uses.
- W002/W003/W014 treat a requirement def as traced when a satisfy/verify
  anywhere in the invocation targets it — directly, through a usage typing
  it (or that usage's `<ID>` short name), or through a def that
  specializes it: satisfying `Derived :> Base` satisfies `Base`.
- W017 evaluates `assert constraint`s against concrete values: metadata
  annotation fields (`@Fmea { severity = 12; }`), typed usages with
  direct values, and typed usages with body values (multi-attribute
  constraints like `lower <= nominal and nominal <= upper`). The
  constraints come from the value's declared type and its supertypes —
  domain libraries add validation rules by writing SysML, not tool code.
  Constraints with unresolved variables are skipped, never guessed.
- W013 (naming) exempts metadata annotations (`@Fmea { ... }`), which take
  their metadata def's PascalCase name by design.

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

Tabular and diagnostic commands support `-f json` for structured output suitable for editor integration and CI pipelines (`diagram` and diagram-rendering views always emit diagram source text).
