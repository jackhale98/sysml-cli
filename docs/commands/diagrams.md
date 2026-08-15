# Diagram Commands

Generate diagrams from SysML v2 models in Mermaid, PlantUML, DOT, or D2 format.

## diagram

```sh
sysml diagram -t bdd model.sysml
sysml diagram -t ibd --scope Vehicle model.sysml
sysml diagram -t trace -r plantuml model.sysml
sysml diagram -t bdd --view StructureView model.sysml
sysml diagram -t act --scope Drive -d LR model.sysml
```

| Option | Description |
|--------|-------------|
| `-t, --type <TYPE>` | Diagram type. See table below. Optional when `--view` names a view def with a `render as` clause — the view's declared rendering is used. |
| `-r, --renderer <FMT>` | Diagram renderer: `mermaid` (default), `plantuml`/`puml`, `dot`/`graphviz`, `d2`/`terrastruct` |
| `-s, --scope <NAME>` | Focus on a specific definition. Required for `ibd`. |
| `--view <NAME>` | Apply a SysML v2 view definition as a filter preset. |
| `-d, --direction <DIR>` | Layout direction: `TB` (default), `LR`, `BT`, `RL` |
| `--depth <N>` | Maximum nesting depth to display. |

### Standard SysML v2 Diagram Types

| Type | Name | Description |
|------|------|-------------|
| `bdd` | Block Definition Diagram | Definitions, specialization, and composition relationships. |
| `ibd` | Internal Block Diagram | Internal structure of a part: blocks, ports, connections, flows. Requires `--scope`. |
| `stm` | State Machine Diagram | States and transitions. Uses rich parser with entry/do/exit actions and transition labels. |
| `act` | Activity Diagram | Action flow with decisions, forks/joins, loops, and control flow. |
| `req` | Requirements Diagram | Requirements with satisfy and verify relationships. |
| `pkg` | Package Diagram | Packages, containment hierarchy, and nested definitions. |
| `par` | Parametric Diagram | Constraint definitions with parameters and bindings. |

### MBSE Analysis Diagram Types

These diagrams support model-based systems engineering (MBSE) analysis workflows.

| Type | Name | Description |
|------|------|-------------|
| `trace` | Traceability Diagram | V-model chain: requirements, satisfying designs, and verification cases. Highlights unsatisfied/unverified requirements. |
| `alloc` | Allocation Diagram | Logical-to-physical mapping: actions/use-cases allocated to parts. Shows unallocated functions. |
| `ucd` | Use Case Diagram | Use case definitions, actors, and include relationships. |

### View Filtering

The `--view` flag applies a SysML v2 view definition as a filter. The view's `expose` and `filter` clauses determine which elements appear in the diagram.

```sysml
view def PartsOnly {
    filter @SysML::PartDefinition;
}

view def VehicleScope {
    expose Vehicle::*;
}
```

```sh
sysml diagram -t bdd --view PartsOnly model.sysml    # Only part definitions
sysml diagram -t bdd --view VehicleScope model.sysml  # Only Vehicle children
```

View filters work with all diagram types and all renderers. If the
view def declares `render as` (e.g. `render asTreeDiagram`), `-t` may
be omitted — the view supplies the diagram type.

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

**IBD with connections** — shows internal block structure with ports and connections:
```sh
sysml diagram -t ibd --scope Vehicle model.sysml
```

**Filtered BDD** — show only part definitions using a view:
```sh
sysml diagram -t bdd --view PartsOnly model.sysml -d LR
```
