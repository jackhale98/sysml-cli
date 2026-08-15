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
| `FitTable` | Tolerancing | mates with computed fits |
| `PortTable` | Reporting | port usages (owner, type, direction) |
| `ConnectionTable` | Reporting | connections (source, target) |
| `AllocationMatrix` | Reporting | allocation pairs |
| `RequirementsTraceMatrix` | Reporting | requirement coverage (satisfied/verified) |
| `ModelStats` | Reporting | element counts by kind |

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
usages, `kind:port`, `relation:satisfy|verify|allocation|connection`,
`trace`, `kindcounts`, `uncertainty`); `columns` copies row fields or
computes derived values with the standard expression language; `where`
filters rows; `sortBy` orders (`-` prefix for descending); `pivot`
renders a count grid. The full contract — every provider and its
fields — is documented in `libraries/Reporting.sysml`.
