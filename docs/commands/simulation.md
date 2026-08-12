# Simulation & Export Commands

Commands for simulating SysML v2 models and exporting to external formats.

## simulate

Run simulations on SysML v2 models: evaluate constraints, simulate state machines, or execute action flows.

```sh
sysml simulate list model.sysml              # Discover simulatable items
```

### simulate eval

Evaluate constraints and calculations with variable bindings.

```sh
sysml simulate eval model.sysml -b speed=100
sysml simulate eval model.sysml -n SpeedLimit -b speed=120
sysml simulate eval model.sysml -b mass=1500,velocity=30 -n KineticEnergy
```

| Option | Description |
|--------|-------------|
| `-b, --bind <BINDINGS>` | Variable bindings: `name=value` (comma-separated) |
| `-n, --name <NAME>` | Evaluate only this constraint or calculation |

### simulate state-machine

Simulate a state machine step-by-step. Alias: `sm`.

Supports `state def`, `exhibit state` (inside part definitions), and nested state regions (parallel orthogonal states). If `--events` is omitted and the machine has signal triggers, you will be prompted interactively.

```sh
sysml simulate state-machine model.sysml -n TrafficLight -e next,next
sysml simulate sm model.sysml -n Controller -b temperature=150
sysml simulate sm model.sysml    # Interactive event selection
```

| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | State machine name (prompted if omitted) |
| `-e, --events <EVENTS>` | Events to inject (comma-separated signal names) |
| `-m, --max-steps <N>` | Max simulation steps (default: 100) |
| `-b, --bind <BINDINGS>` | Variable bindings for guard expressions |

### simulate action-flow

Execute an action flow step-by-step. Alias: `af`.

Walks through perform steps, decisions, forks/joins, accept/send actions, loops, and merge/terminate nodes, producing an execution trace.

```sh
sysml simulate action-flow model.sysml -n ProcessOrder
sysml simulate af model.sysml -b fuelLevel=80
```

| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | Action name (prompted if omitted) |
| `-m, --max-steps <N>` | Max execution steps (default: 1000) |
| `-b, --bind <BINDINGS>` | Variable bindings for conditionals |

## export

Export co-simulation artifacts from SysML models: Modelica stubs and SSP system structure descriptions.

```sh
sysml export list model.sysml                              # List exportable parts
sysml export modelica model.sysml --part Engine             # Modelica stub
sysml export modelica model.sysml --part Engine -o Engine.mo
sysml export ssp model.sysml                                # SSP XML
sysml export ssp model.sysml -o system.ssd
```

For a tabular listing of part interfaces, use `sysml view PortTable` (see [views](views.md)).

### export modelica

Generate a Modelica partial model stub from a part definition.

| Option | Description |
|--------|-------------|
| `-p, --part <PART>` | Part definition name (required) |
| `-o, --output <PATH>` | Output file path (default: stdout) |

### export ssp

Generate an SSP (System Structure and Parameterization) XML document.

| Option | Description |
|--------|-------------|
| `-o, --output <PATH>` | Output file path (default: stdout) |

### export list

List all exportable parts and their interfaces.
