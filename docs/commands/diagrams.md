# Diagram Commands

Generate ad-hoc diagrams from SysML v2 models in Mermaid, PlantUML, DOT,
or D2 format. Model-declared diagrams — a `view def` with a `render as`
clause — render through [`sysml view <name>`](views.md#diagram-views)
instead, with the view's `expose`/`filter` clauses selecting content.

## diagram

```sh
sysml diagram -t gv model.sysml
sysml diagram -t iv --scope Vehicle model.sysml
sysml diagram -t trace -r plantuml model.sysml
sysml diagram -t afv --scope Drive --direction LR model.sysml
```

| Option | Description |
|--------|-------------|
| `-t, --type <TYPE>` | Diagram type (required). See table below. |
| `-r, --renderer <FMT>` | Diagram renderer: `mermaid` (default), `plantuml`/`puml`, `dot`/`graphviz`, `d2`/`terrastruct` |
| `--scope <NAME>` | Focus on a specific definition. Required for `iv`. |
| `--direction <DIR>` | Layout direction: `TB` (default), `LR`, `BT`, `RL` |
| `--depth <N>` | Maximum nesting depth to display. |

Output is always the diagram source text on stdout (the global `-f`
format flag does not apply to diagrams).

### SysML v2 Standard View Types

The canonical names follow the SysML v2 standard view definitions;
the legacy SysML v1-style aliases remain accepted.

| Type | Alias | Name | Description |
|------|-------|------|-------------|
| `gv` | `bdd` | General View | Definitions, specialization, and composition relationships. |
| `iv` | `ibd` | Interconnection View | Internal structure of a part: parts, ports, connections, flows. Requires `--scope`. |
| `stv` | `stm` | State Transition View | States and transitions, with entry/do/exit actions and transition labels. |
| `afv` | `act` | Action Flow View | Action flow with decisions, forks/joins, loops, and control flow. |
| `sv` | | Sequence View | Lifelines and messages from flows. |
| `bv` | `pkg` | Browser View | Packages, containment hierarchy, and nested definitions. |
| `req` | | Requirements grid | Requirements with satisfy and verify relationships. |
| `par` | | Parametric | Constraint definitions with parameters and bindings. |

### MBSE Analysis Types

| Type | Name | Description |
|------|------|-------------|
| `trace` (also `grv`) | Traceability | V-model chain: requirements, satisfying designs, and verification cases. Highlights unsatisfied/unverified requirements. |
| `alloc` | Allocation | Logical-to-physical mapping: actions/use-cases allocated to parts. Shows unallocated functions. |
| `ucd` | Use Case | Use case definitions, actors, and include relationships. |

### Output Formats

| Format | Aliases | Rendering |
|--------|---------|-----------|
| `mermaid` | `mmd` | GitHub, Obsidian, Mermaid Live Editor |
| `plantuml` | `puml` | PlantUML Server, IDE plugins |
| `dot` | `graphviz` | Graphviz (`dot` command) |
| `d2` | `terrastruct` | D2 / Terrastruct |

### Examples

**Traceability diagram for a review** — shows which requirements are satisfied and verified:
```sh
sysml diagram -t trace model.sysml -r plantuml > trace.puml
```

**Allocation diagram** — shows which logical functions are mapped to physical parts:
```sh
sysml diagram -t alloc model.sysml
```

**Interconnection view** — internal structure with ports and connections:
```sh
sysml diagram -t iv --scope Vehicle model.sysml
```

**Model-declared diagram** — the view def picks the type and filters the
content; the CLI just renders it:
```sysml
view def SafetyFeatureView {
    filter @Safety;
    render asTreeDiagram;
}
```
```sh
sysml view SafetyFeatureView -r d2 model.sysml
```
