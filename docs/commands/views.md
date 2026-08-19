# Views

Render model-defined views. A view is a SysML v2 `view def` declaring
either a table (a `@TableRendering` metadata annotation from the
`Reporting` domain library: rows, columns, sorting, filtering,
pivoting) or a diagram (a standard `render as` clause, with `expose`/
`filter` selecting the content) — the entire report specification
lives in the model, so projects add reports by writing view defs, not
tool code.

## view

```sh
sysml view                                    # list available views
sysml view FmeaWorksheet
sysml view RiskMatrix -I libraries model.sysml
sysml view StackupSummary -f csv > stackups.csv
sysml view ModelStats -f md
sysml view SafetyFeatureView -r d2            # diagram view, D2 output
```

With no name, `sysml view` lists every view def in scope, marking
table views and diagram views (`(diagram: asTreeDiagram)`). For table
views, `-f csv` and `-f md` emit CSV or Markdown; `-f json` emits
structured rows.

## Diagram views

A view def with a `render as` clause renders as a diagram — the
official SysML v2 syntax:

```sysml
view def SafetyFeatureView {
    filter @Safety;
    render asTreeDiagram;
}
```

`sysml view SafetyFeatureView` builds the diagram declared by the
render clause, restricted to the elements the view's `expose`/`filter`
clauses select. `-r/--renderer` chooses the output language: mermaid
(default), plantuml, dot, d2. Ad-hoc diagrams without a view def stay
on `sysml diagram -t <type>`.

## Standard views

The bundled `libraries/` ship ready-made views:

| View | Library | Rows |
|------|---------|------|
| `FmeaWorksheet` | RiskAnalysis | `@Fmea` annotations with derived RPN, sorted descending |
| `RiskMatrix` | RiskAnalysis | severity × likelihood pivot of `@Fmea` line items |
| `HazardLog` | HazardAnalysis | hazards with severity and mitigation status |
| `StackupSummary` | Tolerancing | evaluated tolerance stackups (worst-case, RSS, Cp/Cpk) |
| `FitTable` | Tolerancing | mates (`relation:connection:Mate`), source and target |
| `PortTable` | Reporting | port usages (owner, type, direction) |
| `ConnectionTable` | Reporting | connections (source, target) |
| `AllocationMatrix` | Reporting | allocation pairs |
| `RequirementsTraceMatrix` | Reporting | requirement coverage (satisfied/verified) |
| `ModelStats` | Reporting | element counts by kind |
| `Bom` | Reporting | composition tree with per-level and extended quantities |

## The @TableRendering convention

```sysml
view def FmeaWorksheet {
    @TableRendering {
        rows = "@Fmea";
        columns = "element; failureMode; severity; likelihood;
                   detection; rpn = severity*likelihood*detection";
        sortBy = "-rpn";
    }
}
```

`rows` selects a row provider (`@Metadata` annotations, `type:Def`
usages, `kind:port`,
`relation:satisfy|verify|allocation|connection|succession`,
`relation:connection:<Type>`, `relation:succession:<Type>`,
`composition` / `composition:<Root>`,
`trace`, `kindcounts`, `uncertainty`); `columns` copies row fields or
computes derived values with the standard expression language; `where`
filters rows; `sortBy` orders (`-` prefix for descending); `pivot`
renders a count grid. The full contract — every provider and its
fields — is documented in `libraries/Reporting.sysml`.

`where` compares numbers and strings. A nested string needs escaping,
and enum values match on their simple name:

```sysml
where = "severity * likelihood * detection >= 80";
where = "category == \"software\"";   // matches RiskCategory::software
```

### Verdicts come from the model

The PASS / MARGINAL / FAIL of an evaluated analysis is not arithmetic —
it is a project's judgement about how close to a limit is too close, so
it is model content: `UncertaintyAnalysis::marginalFraction` (default
0.10) is the fraction of the tolerance band within which a positive
margin still reports MARGINAL. Override it per analysis, or across a
class of them:

```sysml
analysis def CriticalFit :> ToleranceStackup {
    attribute :>> marginalFraction = 0.25;
}
```

### Filtering connections by type

`relation:connection` returns every connection in the model. Naming a
type restricts it to connections of that type or a specialization, which
is what separates a tolerance mate from a hazard causation:

```sysml
view def FitTable {
    @TableRendering {
        rows = "relation:connection:Mate";
        columns = "connection; source; target";
    }
}
```

### Dependency edges

`relation:succession` returns the succession edges of an action flow —
`succession s1 first a then b`, the anonymous `first a then b`, and the
feature-chain form `first a.inner then b.inner`. Columns are
`succession`, `type`, `source`, `target`, `parent`, `file`.

A succession's type is its dependency kind, so the same type filter that
connections use applies and closes over specialization:

```sysml
view def Dependencies {
    @TableRendering {
        rows = "relation:succession";
        columns = "succession; type; source; target";
    }
}
```

This is the graph a critical-path or Gantt export reads.

### Bills of materials

`composition` walks `part` and `item` usages — connections are
structure, not content — and reports `element`, `type`, `parent`,
`path`, `depth`, `quantity` (from the usage multiplicity) and
`extended` (quantity multiplied down the tree, so a part used 2x inside
an assembly used 3x reports 6). Any attribute declared on a row's type
can be named as a column:

```sysml
view def Bom {
    @TableRendering {
        rows = "composition:Vehicle";   // omit the root to walk every top
        columns = "path; type; quantity; extended; mass; partNumber";
        sortBy = "path";
    }
}
```

Export with `sysml view Bom model.sysml -f csv > bom.csv`.
