# Analysis Commands

Commands for validating, inspecting, and querying SysML v2 models.

## check

Validate SysML v2 files against structural rules.

```sh
sysml check model.sysml
sysml check src/*.sysml                       # Multiple files
sysml check -f json model.sysml               # JSON output
sysml check model.sysml -I lib/               # Ad-hoc include path
sysml check --severity warning model.sysml    # Only warnings+
sysml check --disable unused,unresolved model.sysml
cat generated.sysml | sysml check -           # Read from stdin
```

| Option | Description |
|--------|-------------|
| `-d, --disable <CHECKS>` | Disable checks (comma-separated). See [Validation](../validation.md). |
| `-s, --severity <LEVEL>` | Minimum severity: `note`, `warning`, `error` (default: `note`) |

When unresolved type references or connection targets have a close match among known definitions, `check` suggests the closest match:
```
model.sysml:5:1: warning[W004]: type `Vehicel` is not defined in this file
  help: did you mean `Vehicle`?
```

Exit codes: `0` = no errors, `1` = errors found.

## list

List model elements with optional filters. Alias: `ls`.

```sh
sysml list model.sysml
sysml list --kind parts model.sysml          # Only part definitions
sysml list --kind port model.sysml           # Only port usages
sysml list --name Vehicle model.sysml        # Name search
sysml list --doc "shall operate" model.sysml # Doc-comment search
sysml list --parent Vehicle model.sysml      # Children of Vehicle
sysml list --unused model.sysml              # Unreferenced defs
sysml list -f json model.sysml               # JSON output
```

| Option | Description |
|--------|-------------|
| `-k, --kind <KIND>` | Filter by kind. `parts` shows both defs and usages. `part-def` / `part-usage` restricts to one. Also: `ports`, `actions`, `states`, `requirements`, `constraints`, `connections`, `attributes`, `items`, `enums`, `all`, `definitions`, `usages` |
| `-n, --name <PATTERN>` | Substring name filter |
| `--doc <TEXT>` | Substring filter on documentation comments |
| `-p, --parent <NAME>` | Filter by parent definition |
| `--unused` | Show only unreferenced definitions |
| `--abstract-only` | Show only abstract definitions |
| `--visibility <VIS>` | Filter by `public`, `private`, `protected` |
| `--view <NAME>` | Apply a SysML v2 view definition as a filter preset |

### Metadata queries (Ch 36)

Filter by metadata annotations and their values:

```sh
sysml list --metadata Status model.sysml                       # annotated elements
sysml list --metadata Status --where status=draft model.sysml  # value constraint
```

Values compare loosely: `status=draft` matches both `"draft"` and
`StatusKind::draft`. `--where` is repeatable; all clauses must match.

## show

Show detailed information about a specific element.

```sh
sysml show model.sysml Vehicle
sysml show model.sysml REQ2                   # Look up by <ID> short name
sysml show -f json model.sysml Engine
sysml show --raw model.sysml Vehicle          # Print raw SysML source text
```

Displays: kind, visibility, parent, documentation, type, children, relationships.

| Option | Description |
|--------|-------------|
| `--raw` | Print the original SysML source text of the element to stdout |

## trace

Generate a requirements traceability matrix.

```sh
sysml trace model.sysml
sysml trace --check model.sysml       # CI gate (threshold from the model)
sysml trace -f json model.sysml
```

Requirement *usages* are first-class rows, labeled with their `<ID>`
short names (`<REQ2> uavFlightTime`); requirement defs appear as rows
only when no usage types them. Satisfy/verify statements match by
simple name, qualified name, feature chain (`reqs.REQ2`), or `<ID>`.
JSON output includes an `id` field per requirement.

The table prints through the same pipeline as `view`, so `-f csv` and
`-f md` work directly. A base requirement def satisfied only through a
specializing requirement (satisfy `Derived :> Base` covers `Base`) shows
`(via specialization)` — trace, `coverage`, and the W002/W003 checks all
apply the same resolution.

| Option | Description |
|--------|-------------|
| `--check` | CI gate: fail per the model's TraceGate (strict without one) |
The gate threshold lives in the model, not in a flag: declare a
constraint usage typed `TraceGate` / `QualityGate` (ship with
`ModelQuality.sysml` in
[sysml-domain-libraries](https://github.com/jackhale98/sysml-domain-libraries),
or write your own def with the same name) and its constraint expression
decides pass/fail:

```sysml
private import ModelQuality::*;
constraint qualityGate : QualityGate { :>> minScore = 80.0; }
constraint traceGate : TraceGate { :>> minVerified = 60.0; }
```

`sysml coverage --check` binds `score` plus the four metric percentages;
`sysml trace --check` binds `satisfied`, `verified`, and `traced` (each
0-100). With no gate declared, `--check` is strict: coverage requires a
perfect score and trace requires every requirement satisfied and
verified.


## Port interfaces

Port and connection listings are model-defined views (see [views](views.md)):

```sh
sysml view PortTable model.sysml
sysml view ConnectionTable model.sysml
```

Unconnected ports are flagged by `sysml check` (W016 `unbound-port`).

## deps

Analyze dependencies for a specific element — what it depends on and what references it.

```sh
sysml deps model.sysml Vehicle
sysml deps model.sysml Engine --reverse       # Only show "referenced by"
sysml deps model.sysml Engine --forward       # Only show "depends on"
sysml deps -f json model.sysml Vehicle
```

| Option | Description |
|--------|-------------|
| `--reverse` | Show only reverse dependencies (what references this element) |
| `--forward` | Show only forward dependencies (what this element depends on) |

## diff

Compare two SysML files and report semantic differences (added/removed/changed definitions, usages, connections).

```sh
sysml diff old.sysml new.sysml
sysml diff -f json v1.sysml v2.sysml
```

Unlike text-based diff, this compares at the model level — detecting renamed types, changed members, and structural modifications regardless of formatting changes.

## allocation

Display the logical-to-physical allocation matrix. In SysML v2, allocations map actions and use-cases to parts.

```sh
sysml allocation model.sysml
sysml allocation --unallocated model.sysml    # Only show gaps
sysml allocation --check model.sysml          # CI: exit 1 if gaps exist
sysml allocation -f json model.sysml
```

| Option | Description |
|--------|-------------|
| `--check` | Exit with error if unallocated elements exist |
| `--unallocated` | Show only unallocated elements |

## coverage

Generate a model quality report: documentation coverage, typed usages, populated definitions, requirement satisfaction/verification, and an overall score.

```sh
sysml coverage model.sysml
sysml coverage --check -I libraries model.sysml   # CI gate (threshold from the model)
sysml coverage -f json model.sysml
```

| Option | Description |
|--------|-------------|
| `--check` | Exit with error if score is below minimum |

**Reported metrics:**

| Metric | Description |
|--------|-------------|
| Documentation | Percentage of definitions with doc comments |
| Typed usages | Percentage of usages with explicit type references |
| Populated defs | Percentage of definitions with at least one member |
| Req satisfaction | Percentage of requirements with a satisfy statement |
| Req verification | Percentage of requirements with a verify statement |
| Overall score | Weighted average of the metrics |

The overall weighting is model-controllable: declare a `calc def
QualityScore` (or import `ModelQuality.sysml` from
[sysml-domain-libraries](https://github.com/jackhale98/sysml-domain-libraries)
and specialize it) and its return expression is evaluated instead of the
built-in equal weighting. The tool binds the fixed parameter vocabulary
`documented`, `typedUsages`, `reqSatisfied`, `reqVerified` (each 0-100);
the expression may weight any subset. The score line shows
`(model:QualityScore)` when a model calc supplied the score.

## Model statistics

Element counts by kind are a model-defined view (see [views](views.md)):

```sh
sysml view ModelStats src/*.sysml
```

## rollup

Aggregate a numeric attribute across the part hierarchy — mass, cost,
power, tolerance budgets.

```sh
sysml rollup compute model.sysml --root Vehicle --attr mass
sysml rollup compute model.sysml --root Vehicle --attr mass --method rss
sysml rollup budget model.sysml --root Vehicle --attr mass --limit 2000
sysml rollup sensitivity model.sysml --root Vehicle --attr mass
sysml rollup sweep model.sysml --root Vehicle --attr mass --param engine --from 100 --to 300 --steps 5
sysml rollup what-if model.sysml --root Vehicle --attr mass --scenario "light:engine=100" --scenario "heavy:engine=300"
```

To find every instance of an attribute across the model, use
`sysml list -k attributes -n <attr>`.

Attribute values may carry unit brackets (`attribute mass = 250 [SI::kg];`).
Mixed units convert automatically into the root's unit (kg/g, m/mm,
h/min, ...) and the result displays its unit.

**Variant configurations** (Ch 35 configure-then-compute):

```sh
sysml list --variants model.sysml
sysml rollup compute model.sysml --root Drone --attr mass --variant battery=powerBattery
```

| Option (compute) | Description |
|--------|-------------|
| `--root <DEF>` | Root part definition to start from |
| `--attr <NAME>` | Attribute to aggregate |
| `--method <M>` | `sum` (default), `rss`, `product`, `min`, `max` |
| `--variant POINT=CHOICE` | Select a variant for a variation point (repeatable). POINT is the part usage name or the variation def name; unselected points include all variants |
