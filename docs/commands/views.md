# Views

Render model-defined views as tables. A view is a SysML v2 `view def`
carrying a `@TableRendering` metadata annotation (from the `Reporting`
domain library) that declares rows, columns, sorting, filtering, and
pivoting — the entire report specification lives in the model, so
projects add reports by writing view defs, not tool code.

## view

```sh
sysml view                                    # list available views
sysml view FmeaWorksheet
sysml view RiskMatrix -I libraries model.sysml
sysml view StackupSummary -f csv > stackups.csv
sysml view ModelStats -f md
```

With no name, `sysml view` lists every view def in scope and marks
which carry `@TableRendering`. `-f csv` and `-f md` emit the table as
CSV or Markdown; `-f json` emits structured rows.

## Standard views

The bundled `libraries/` ship ready-made views:

| View | Library | Rows |
|------|---------|------|
| `FmeaWorksheet` | RiskAnalysis | `@Fmea` annotations with derived RPN, sorted descending |
| `RiskMatrix` | RiskAnalysis | severity × occurrence pivot of `@Fmea` line items |
| `HazardLog` | HazardAnalysis | hazards with severity and mitigation status |
| `StackupSummary` | Tolerancing | evaluated tolerance stackups (worst-case, RSS, Cp/Cpk) |
| `FitTable` | Tolerancing | mates with computed fits |
| `PortTable` | StandardViews | port usages (owner, type, direction) |
| `ConnectionTable` | StandardViews | connections (source, target) |
| `AllocationMatrix` | StandardViews | allocation pairs |
| `RequirementsTraceMatrix` | StandardViews | requirement coverage (satisfied/verified) |
| `ModelStats` | StandardViews | element counts by kind |

## The @TableRendering convention

```sysml
view def FmeaWorksheet {
    @TableRendering {
        rows = "@Fmea";
        columns = "element; failureMode; severity; occurrence;
                   detection; rpn = severity*occurrence*detection";
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
