# Editing Commands

Commands for generating, modifying, and formatting SysML v2 source files.

## add

Add an element to a SysML model — interactively, to a file, or to stdout.

The **interactive wizard** is the primary workflow. With no arguments it guides you through creating any element. With a file, it shows model-aware type suggestions. Flags provide the same functionality for scripting and CI.

```sh
sysml add                                                  # interactive wizard
sysml add model.sysml                                      # wizard with model context
sysml add model.sysml part-def Vehicle                     # insert into file
sysml add model.sysml part engine -t Engine --inside Vehicle  # usage inside def
sysml add --stdout part-def Vehicle                        # print to stdout
sysml add --stdout --teach part-def Vehicle                # with teaching comments
```

### Definitions (types)

```sh
sysml add model.sysml part-def Vehicle --extends Base --doc "A vehicle"
sysml add model.sysml port-def FuelPort -m "in item fuel:FuelType"
sysml add model.sysml enum-def Color -m red,green,blue
```

### State machines

```sh
sysml add model.sysml state-def EngineStates \
    -m "entry; then off;" \
    -m "state off,state starting,state running" \
    -m "transition first off accept startCmd then starting" \
    -m "transition first starting then running"
```

### Actions with successions

```sh
sysml add model.sysml action-def ReadSensors \
    -m "action readTemp,action processData" \
    -m "first readTemp then processData"
```

### Constraints with expressions

```sh
sysml add model.sysml constraint-def TempLimit \
    -m "in attribute temp:Real" \
    -m "constraint temp >= -40 and temp <= 60"
```

### Calculations with return type

```sh
sysml add model.sysml calc-def BatteryRuntime \
    -m "in attribute capacity:Real,in attribute consumption:Real" \
    -m "return hours:Real"
```

### Verification cases

```sh
sysml add model.sysml verification-def TestAccuracy \
    --doc "Verify sensor accuracy" \
    -m "subject testSubject" \
    -m "requirement tempReq:TemperatureAccuracy"
```

### Connections, satisfy, verify, imports

```sh
sysml add model.sysml connection tempConn -t SensorConnection \
    --connect "tempSensor.dataOut to controller.tempIn" --inside Assembly

sysml add model.sysml satisfy TemperatureAccuracy --by TemperatureSensor
sysml add model.sysml verify TemperatureAccuracy --by TestAccuracy
sysml add model.sysml import "WeatherStation::*"
```

### Multiplicity

```sh
sysml add model.sysml part-def Vehicle \
    -m "part wheels:Wheel[4],attribute doors:Door[2..5]"
```

**Available kinds:** `part-def`, `port-def`, `action-def`, `state-def`, `constraint-def`, `calc-def`, `requirement` (`req`), `enum-def`, `attribute-def` (`attr`), `item-def`, `view-def`, `viewpoint-def`, `package` (`pkg`), `use-case`, `connection-def`, `interface-def`, `flow-def`, `allocation-def`, `verification-def` (`vcase`)

Usage-level kinds (no `-def` suffix) generate `kind name [: type];` usages suitable for insertion inside a definition. Special kinds: `import`, `satisfy`, `verify`, `connection` (with `--connect`).

| Option | Description |
|--------|-------------|
| `-t, --type-ref <TYPE>` | Type reference (`: Type` for usages, `:> Type` for defs with `--extends`) |
| `--inside <NAME>` | Insert inside this definition (auto-detected for usages) |
| `--extends <TYPE>` | Specialization supertype (`:>` syntax) |
| `--abstract` | Mark as abstract |
| `--short-name <ALIAS>` | Short name (`<alias>` before the name) |
| `--doc <TEXT>` | Documentation comment (`doc /* text */`) |
| `-m, --member <SPEC>` | Add members (repeatable, comma-separated). Format: `"[dir] kind name[:type[mult]]"` |
| `--connect <ENDPOINTS>` | Connection binding: `"a.portOut to b.portIn"` |
| `--by <ELEMENT>` | Target element for satisfy/verify relationships |
| `--satisfy <REQ>` | Create satisfy relationship (with `--by`) |
| `--verify <REQ>` | Create verify relationship (with `--by`) |
| `--expose <PATTERN>` | (view-def) Expose clause: `"Vehicle::*"` |
| `--filter <KIND>` | (view-def) Filter by element kind |
| `--stdout` | Print to stdout without modifying files |
| `--teach` | Include teaching comments explaining every SysML v2 construct used |
| `--dry-run` | Preview changes as a unified diff without writing |

### Raw-line members

For transitions, successions, and constraint expressions, `-m` renders verbatim text:

| Pattern | Detected by first word | Example |
|---------|----------------------|---------|
| `transition ...` | `transition` | `transition first idle accept go then running` |
| `entry; then ...` | `entry` | `entry; then off;` |
| `exit ...` | `exit` | `exit action cleanup` |
| `first ... then ...` | `first` | `first step1 then step2` |
| `accept ...` | `accept` | `accept signal` |
| `send ...` | `send` | `send msg to target` |
| `constraint <expr>` | `constraint` + operators | `constraint x >= 0 and x <= 100` |

### Import hints

After inserting an element with a type reference not defined in the current file, `add` prints a hint:

```
Added `tempSensor` to model.sysml
  hint: `TemperatureSensor` is not defined in this file. You may need:
    sysml add model.sysml import '...::TemperatureSensor'
```

### Dispatch modes

| file | kind | name | `--stdout` | Behavior |
|------|------|------|------------|----------|
| None | None | None | false | Full interactive wizard |
| None | Some | Some | any | Stdout (infer `--stdout`) |
| Some | None | None | false | Guided file mode: parse file, wizard picks kind/name/parent |
| Some | Some | Some | false | Direct insert into file |

## remove

Remove a named element from a SysML file.

```sh
sysml remove model.sysml Engine --dry-run    # preview
sysml remove model.sysml Engine              # apply
```

| Option | Description |
|--------|-------------|
| `--dry-run` | Preview changes as a unified diff without writing |

## rename

Rename an element and update all whole-word references in the file.

```sh
sysml rename model.sysml Engine Motor --dry-run    # preview
sysml rename model.sysml Engine Motor              # apply
sysml rename model.sysml Engine Motor --project    # rename across all project files
```

| Option | Description |
|--------|-------------|
| `--dry-run` | Preview changes as a unified diff without writing |
| `--project` | Rename across all project files (not just the specified file) |

## fmt

Format SysML v2 files. CST-aware indentation that handles nested definitions, comments, and state machines correctly.

```sh
sysml fmt model.sysml
sysml fmt --check model.sysml         # CI: exit 1 if unformatted
sysml fmt --diff model.sysml          # Show diff without writing
sysml fmt --indent-width 2 model.sysml
cat generated.sysml | sysml fmt -     # Format stdin to stdout
```

| Option | Description |
|--------|-------------|
| `--check` | Check formatting without modifying (exit 1 if unformatted) |
| `--diff` | Print diff instead of writing files |
| `--indent-width <N>` | Indentation width (default: 4) |
