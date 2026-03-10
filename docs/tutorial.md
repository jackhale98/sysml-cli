# Tutorial: Building, Editing, and Analyzing a SysML v2 Model

This tutorial walks through a complete systems engineering workflow using the `sysml` command-line tool. You will build a model from scratch, validate it, generate diagrams, run simulations, and use lifecycle management features.

We will model a **weather station** — a small embedded system with sensors, a controller, and a display. Every model element is created through CLI commands.

> **Tip:** If you prefer not to type flags, run `sysml add` with no arguments to launch an interactive wizard that guides you through creating any element.

## Prerequisites

Install the tool from source:

```sh
git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git
cd sysml-cli
cargo install --path crates/sysml-cli
```

Verify the installation:

```sh
sysml --version
```

## Part 1: Project Setup

### 1.1 Initialize a project

```sh
mkdir weather-station && cd weather-station
sysml init
```

This creates a `.sysml/` directory with a `config.toml` file.

### 1.2 Explore help topics

```sh
sysml guide                    # list available topics
sysml guide getting-started    # first-time tutorial
sysml guide sysml-basics       # SysML v2 language overview
```

### 1.3 Generate an example project

```sh
sysml example --list
sysml example brake-system -o /tmp/brake-example
```

### 1.4 Learn SysML syntax with --teach

If you are new to SysML v2, the `--teach` flag adds explanatory comments to generated code:

```sh
sysml add --stdout --teach part-def Motor
```

Use this on any element kind to understand the syntax before building your model.

## Part 2: Building the Model

In SysML v2, **definitions** (e.g., `part def Sensor`) are reusable types, while **usages** (e.g., `part tempSensor : Sensor`) are instances of those types placed inside an assembly. This tutorial creates definitions first, then assembles them with usages.

### 2.1 Create the model file

Generate the package and redirect it into the model file:

```sh
sysml add --stdout package WeatherStation --doc "Weather station model" > model.sysml
```

### 2.2 Add enum definitions with members

Enums provide fixed sets of choices. Use `-m` to add members directly:

```sh
sysml add model.sysml enum-def DisplayMode -m summary -m detailed -m alert
sysml add model.sysml enum-def SensorStatus -m ok -m degraded -m failed
```

### 2.3 Add port definitions

Ports define interaction points. Use `-m` with direction modifiers:

```sh
sysml add model.sysml port-def SensorDataPort \
    -m "out item reading:ScalarValues::Real"

sysml add model.sysml port-def DisplayDataPort \
    -m "in item displayValue:ScalarValues::Real"

sysml add model.sysml port-def PowerPort \
    -m "in item voltage:ScalarValues::Real"
```

### 2.4 Add part definitions (reusable types)

A **part definition** defines a reusable component type. Create an abstract base sensor with attributes and ports:

```sh
sysml add model.sysml part-def Sensor --abstract \
    --doc "Base type for all sensors" \
    -m "attribute status:SensorStatus" \
    -m "attribute sampleRate:ScalarValues::Real" \
    -m "port dataOut:SensorDataPort" \
    -m "port power:PowerPort"
```

Create specialized sensors that extend the base. `--extends` adds `:> Sensor` specialization:

```sh
sysml add model.sysml part-def TemperatureSensor --extends Sensor \
    --doc "Measures ambient temperature in degrees Celsius" \
    -m "attribute range_min:ScalarValues::Real" \
    -m "attribute range_max:ScalarValues::Real"

sysml add model.sysml part-def HumiditySensor --extends Sensor \
    --doc "Measures relative humidity as a percentage" \
    -m "attribute accuracy:ScalarValues::Real"

sysml add model.sysml part-def PressureSensor --extends Sensor \
    --doc "Measures barometric pressure in hPa"

sysml add model.sysml part-def WindSensor --extends Sensor \
    --doc "Measures wind speed in m/s and direction" \
    -m "attribute maxSpeed:ScalarValues::Real"
```

Create the remaining component types:

```sh
sysml add model.sysml part-def Controller \
    --doc "Central processing unit" \
    -m "port tempIn:SensorDataPort" \
    -m "port humidIn:SensorDataPort" \
    -m "port pressIn:SensorDataPort" \
    -m "port windIn:SensorDataPort" \
    -m "port displayOut:DisplayDataPort" \
    -m "port power:PowerPort" \
    -m "attribute firmware_version:ScalarValues::String"

sysml add model.sysml part-def Display \
    --doc "LCD display for weather readings" \
    -m "port dataIn:DisplayDataPort" \
    -m "port power:PowerPort" \
    -m "attribute mode:DisplayMode" \
    -m "attribute brightness:ScalarValues::Real"

sysml add model.sysml part-def PowerSupply \
    --doc "Solar-powered battery pack" \
    -m "attribute capacity_ah:ScalarValues::Real" \
    -m "attribute voltage:ScalarValues::Real"

sysml add model.sysml part-def Enclosure \
    --doc "Weather-resistant outdoor housing" \
    -m "attribute material:ScalarValues::String" \
    -m "attribute ip_rating:ScalarValues::String"
```

### 2.5 Add a connection definition

```sh
sysml add model.sysml connection-def SensorConnection \
    -m "part source:Sensor" -m "part target:Controller"
```

### 2.6 Build the main assembly with part usages

A **part usage** creates a specific instance of a part definition inside an assembly. Create the top-level assembly and populate it:

```sh
sysml add model.sysml part-def WeatherStationUnit \
    --doc "Complete weather station assembly"

# Part usages — these are instances of the definitions above.
# -t sets the type reference, --inside places them in the assembly.
sysml add model.sysml part tempSensor -t TemperatureSensor --inside WeatherStationUnit
sysml add model.sysml part humiditySensor -t HumiditySensor --inside WeatherStationUnit
sysml add model.sysml part pressureSensor -t PressureSensor --inside WeatherStationUnit
sysml add model.sysml part windSensor -t WindSensor --inside WeatherStationUnit
sysml add model.sysml part controller -t Controller --inside WeatherStationUnit
sysml add model.sysml part display -t Display --inside WeatherStationUnit
sysml add model.sysml part power -t PowerSupply --inside WeatherStationUnit
sysml add model.sysml part enclosure -t Enclosure --inside WeatherStationUnit
```

### 2.7 Add connections between parts

Use `--connect` to wire parts together inside the assembly:

```sh
sysml add model.sysml connection tempConn -t SensorConnection \
    --connect "tempSensor.dataOut to controller.tempIn" --inside WeatherStationUnit

sysml add model.sysml connection humidConn -t SensorConnection \
    --connect "humiditySensor.dataOut to controller.humidIn" --inside WeatherStationUnit

sysml add model.sysml connection pressConn -t SensorConnection \
    --connect "pressureSensor.dataOut to controller.pressIn" --inside WeatherStationUnit

sysml add model.sysml connection windConn -t SensorConnection \
    --connect "windSensor.dataOut to controller.windIn" --inside WeatherStationUnit

sysml add model.sysml connection displayConn \
    --connect "controller.displayOut to display.dataIn" --inside WeatherStationUnit
```

### 2.8 Validate and explore

```sh
sysml lint model.sysml
sysml list model.sysml
sysml list --kind parts model.sysml        # part definitions only
sysml list --kind ports model.sysml        # port definitions only
sysml list --parent WeatherStationUnit model.sysml   # usages inside the assembly
sysml show model.sysml WeatherStationUnit
sysml show --raw model.sysml TemperatureSensor       # raw SysML source text
sysml stats model.sysml
```

### 2.9 Using interactive mode

Instead of typing all these flags, `sysml add` launches an interactive wizard:

```sh
sysml add                  # full wizard — choose what to create, name it, pick a file
sysml add model.sysml      # guided mode — wizard with model-aware type suggestions
```

The wizard shows available types from your model and supports all element kinds including connections, imports, satisfy/verify relationships, and enum definitions with members.

## Part 3: Editing the Model

### 3.1 Add a new sensor

```sh
sysml add model.sysml part-def RainGauge --doc "Measures rainfall in mm/hr" --extends Sensor
sysml add model.sysml part rainGauge -t RainGauge --inside WeatherStationUnit
```

### 3.2 Preview with --dry-run

```sh
sysml add model.sysml part-def Anemometer --doc "Wind direction sensor" --dry-run
```

### 3.3 Generate to stdout

```sh
sysml add --stdout part-def GPSSensor --doc "Location tracking" \
    -m "attribute latitude:Real" -m "attribute longitude:Real"
```

### 3.4 Multiplicity

Use bracket notation on member specs for cardinality:

```sh
sysml add --stdout part-def Vehicle \
    -m "part wheels:Wheel[4]" -m "attribute doors:Door[2..5]"
```

### 3.5 Remove and rename

```sh
sysml remove model.sysml RainGauge --dry-run    # preview first
sysml remove model.sysml RainGauge              # apply
sysml rename model.sysml WindSensor Anemometer --dry-run
```

## Part 4: Requirements and Traceability

### 4.1 Create the requirements file

```sh
sysml add --stdout package WeatherStationRequirements \
    --doc "Weather station requirements" > requirements.sysml
```

Add an import so requirements can reference model elements:

```sh
sysml add requirements.sysml import "WeatherStation::*"
```

### 4.2 Add requirements

```sh
sysml add requirements.sysml requirement TemperatureAccuracy \
    --doc "The temperature sensor shall measure with +/- 0.5C accuracy"

sysml add requirements.sysml requirement OperatingRange \
    --doc "The station shall operate from -40C to +60C"

sysml add requirements.sysml requirement BatteryLife \
    --doc "The station shall operate 72 hours without solar charging"

sysml add requirements.sysml requirement UpdateRate \
    --doc "The display shall update readings every 5 seconds"

sysml add requirements.sysml requirement IPRating \
    --doc "The enclosure shall achieve IP65 or higher"
```

### 4.3 Link requirements to implementation with satisfy

```sh
sysml add requirements.sysml satisfy TemperatureAccuracy -t WeatherStationUnit
sysml add requirements.sysml satisfy OperatingRange -t WeatherStationUnit
sysml add requirements.sysml satisfy BatteryLife -t WeatherStationUnit
sysml add requirements.sysml satisfy UpdateRate -t WeatherStationUnit
sysml add requirements.sysml satisfy IPRating -t WeatherStationUnit
```

Or use the flag syntax: `sysml add --satisfy TemperatureAccuracy --by WeatherStationUnit`

### 4.4 Generate the traceability matrix

```sh
sysml trace requirements.sysml
```

Output:

```
Requirement          Satisfied By         Verified By
------------------------------------------------------------
TemperatureAccuracy  WeatherStationUnit   -
OperatingRange       WeatherStationUnit   -
BatteryLife          WeatherStationUnit   -
UpdateRate           WeatherStationUnit   -
IPRating             WeatherStationUnit   -

Coverage: 5/5 satisfied (100%), 0/5 verified (0%)
```

Use as a CI gate:

```sh
sysml trace --check --min-coverage 80 requirements.sysml
```

### 4.5 Check model coverage

```sh
sysml coverage model.sysml
```

## Part 5: Verification Cases

### 5.1 Create the verification file

```sh
sysml add --stdout package WeatherStationVerification \
    --doc "Verification cases" > verification.sysml

sysml add verification.sysml import "WeatherStation::*"
sysml add verification.sysml import "WeatherStationRequirements::*"
```

### 5.2 Add verification case definitions

Verification cases have a complex internal structure (`objective { verify requirement ... }`) that requires hand-editing after creation. Create the skeleton with `add`, then edit the objective block:

```sh
sysml add verification.sysml requirement TestTemperatureAccuracy \
    --doc "Verify temperature sensor accuracy against reference thermometer"

sysml add verification.sysml requirement TestOperatingRange \
    --doc "Environmental chamber test across full temperature range"

sysml add verification.sysml requirement TestBatteryLife \
    --doc "Continuous operation test without solar input"
```

> **Note:** Full `verification case def` syntax with `objective { verify requirement ... }` is not yet supported by `sysml add`. For proper verification case structure, edit the generated definitions to use `verification case def` and add the objective blocks. See `sysml add --teach --stdout requirement TestCase` for syntax guidance.

### 5.3 Check verification coverage

```sh
sysml verify coverage verification.sysml requirements.sysml
sysml verify list verification.sysml
sysml verify status verification.sysml requirements.sysml
```

### 5.4 Execute a verification case interactively

```sh
sysml verify run verification.sysml --case TestTemperatureAccuracy --author "Jane Smith"
```

The tool walks you through each step, collects pass/fail judgments, and writes a TOML record to `.sysml/records/`.

## Part 6: Diagrams

### 6.1 Block Definition Diagram (BDD)

```sh
sysml diagram -t bdd model.sysml
```

Output is Mermaid format by default (renderable in GitHub, Obsidian, etc.). Other formats:

```sh
sysml diagram -t bdd -o plantuml model.sysml
sysml diagram -t bdd -o dot model.sysml
sysml diagram -t bdd -o d2 model.sysml
```

### 6.2 Internal Block Diagram (IBD)

```sh
sysml diagram -t ibd --scope WeatherStationUnit model.sysml
```

Shows parts inside WeatherStationUnit and their connections.

### 6.3 Requirements Diagram

```sh
sysml diagram -t req requirements.sysml
```

### 6.4 State Machine Diagram

Add a state machine using `sysml add`. The definition can be created via CLI, but transitions require hand-editing since they use complex `first ... accept ... then` syntax:

```sh
sysml add model.sysml state-def StationStates \
    --doc "Weather station operating states"
```

Then add the states and transitions by editing `model.sysml` to fill in the body:

```sysml
state def StationStates {
    doc /* Weather station operating states */
    entry; then off;

    state off;
    state initializing;
    state monitoring;
    state alerting;
    state lowPower;

    transition first off accept powerOn then initializing;
    transition first initializing then monitoring;
    transition first monitoring accept alertTrigger then alerting;
    transition first alerting accept clearAlert then monitoring;
    transition first monitoring accept lowBattery then lowPower;
    transition first lowPower accept charged then monitoring;
}
```

> **Note:** State transitions and `entry/then` syntax are not yet supported by `sysml add`. This is the one area where hand-editing is required.

Generate the diagram:

```sh
sysml diagram -t stm --scope StationStates model.sysml
```

### 6.5 Activity Diagram

Create an action definition:

```sh
sysml add model.sysml action-def ReadSensors \
    --doc "Read all sensor data and update display"
```

Action successions (`first ... then ...`) require hand-editing inside the action body.

```sh
sysml diagram -t act --scope ReadSensors model.sysml
```

### 6.6 Other diagram types

```sh
sysml diagram -t pkg model.sysml      # Package diagram
sysml diagram -t par model.sysml      # Parametric diagram (constraints)
sysml diagram -t trace model.sysml    # V-model traceability diagram
sysml diagram -t alloc model.sysml    # Allocation diagram
sysml diagram -t ucd model.sysml      # Use case diagram
```

## Part 7: Simulation

### 7.1 Constraint evaluation

Create constraint and calc definitions:

```sh
sysml add --stdout constraint-def TemperatureLimit \
    --doc "Operating temperature range" > constraints.sysml

sysml add constraints.sysml constraint-def PowerBudget \
    --doc "Maximum power consumption"

sysml add constraints.sysml calc-def BatteryRuntime \
    --doc "Calculate battery runtime in hours"
```

Constraint expressions and calc bodies require hand-editing. Edit `constraints.sysml` to add:

```sysml
constraint def TemperatureLimit {
    doc /* Operating temperature range */
    in temp : Real;
    temp >= -40 and temp <= 60;
}

constraint def PowerBudget {
    doc /* Maximum power consumption */
    in consumption : Real;
    consumption <= 500;
}

calc def BatteryRuntime {
    doc /* Calculate battery runtime in hours */
    in capacity : Real;
    in consumption : Real;
    return hours : Real;
    capacity * 1000 / consumption
}
```

> **Note:** Constraint expressions and calc return bodies are not yet supported by `sysml add`. The CLI creates the definition skeleton; you add the math.

Evaluate with variable bindings:

```sh
sysml simulate eval constraints.sysml -n TemperatureLimit -b temp=25
# Output: constraint TemperatureLimit: satisfied

sysml simulate eval constraints.sysml -n TemperatureLimit -b temp=70
# Output: constraint TemperatureLimit: violated

sysml simulate eval constraints.sysml -n BatteryRuntime -b capacity=12,consumption=200
# Output: calc BatteryRuntime: 60
```

### 7.2 State machine simulation

```sh
sysml simulate sm model.sysml -n StationStates -e powerOn,alertTrigger,clearAlert,lowBattery,charged
```

Without `--events`, the tool prompts you interactively to select events from the available triggers.

### 7.3 Action flow execution

```sh
sysml simulate af model.sysml -n ReadSensors
```

### 7.4 List simulatable elements

```sh
sysml simulate list model.sysml
```

## Part 8: Analysis

```sh
# Dependency analysis
sysml deps model.sysml WeatherStationUnit
sysml deps model.sysml TemperatureSensor --reverse

# Interface analysis — find unconnected ports
sysml interfaces model.sysml
sysml interfaces --unconnected model.sysml

# Allocation analysis
sysml allocation model.sysml
sysml allocation --check model.sysml

# Semantic diff — compare two model versions
cp model.sysml model-v2.sysml
sysml diff model.sysml model-v2.sysml
```

## Part 9: Formatting

```sh
sysml fmt model.sysml                   # format in place
sysml fmt --diff model.sysml            # preview changes
sysml fmt --check model.sysml           # CI mode — exit 1 if unformatted
sysml fmt --indent-width 2 model.sysml  # custom indent
```

## Part 10: Lifecycle Management

These commands work with domain library types shipped in `libraries/`.

### 10.1 Risk Management

Create risks interactively with the wizard:

```sh
sysml risk add
```

Or create a risk file with CLI commands:

```sh
sysml add --stdout package WeatherStationRisks \
    --doc "Risk register" > risks.sysml
sysml add risks.sysml import "WeatherStation::*"

sysml add risks.sysml part-def riskMoistureIngress \
    --extends "SysMLRisk::RiskDef" \
    --doc "Moisture entering the enclosure could damage electronics"

sysml add risks.sysml part-def riskSolarFailure \
    --extends "SysMLRisk::RiskDef" \
    --doc "Solar panel degradation reduces charging capability"
```

> **Note:** Risk severity/likelihood enum attribute values (`attribute redefines severity = ...`) require hand-editing after creation. The `sysml risk add` wizard handles this automatically.

Analyze risks:

```sh
sysml risk list risks.sysml -I libraries/
sysml risk matrix risks.sysml -I libraries/
sysml risk fmea risks.sysml -I libraries/
```

### 10.2 Tolerance Analysis

```sh
sysml tol analyze model.sysml -I libraries/
sysml tol analyze model.sysml -I libraries/ --method rss
sysml tol analyze model.sysml -I libraries/ --method monte-carlo --iterations 50000
sysml tol sensitivity model.sysml -I libraries/
```

### 10.3 Bill of Materials

```sh
sysml bom rollup model.sysml --root WeatherStationUnit -I libraries/
sysml bom rollup model.sysml --root WeatherStationUnit --include-mass --include-cost -I libraries/
sysml bom where-used model.sysml --part TemperatureSensor -I libraries/
sysml bom export model.sysml --root WeatherStationUnit -I libraries/
```

### 10.4 Supplier Management

```sh
sysml source list model.sysml -I libraries/
sysml source asl model.sysml -I libraries/
sysml source rfq --part TemperatureSensor --quantity 1000 --description "Industrial temp sensor"
```

### 10.5 Manufacturing

```sh
sysml mfg list model.sysml -I libraries/
sysml mfg spc --parameter SensorCalibration \
    --values 0.48,0.52,0.50,0.49,0.51,0.50,0.53,0.47,0.51,0.49
sysml mfg start-lot    # interactive lot creation
sysml mfg step          # advance through routing steps
```

### 10.6 Quality Control

```sh
sysml qc sample-size --lot-size 500
sysml qc sample-size --lot-size 500 --aql 0.65 --level tightened
sysml qc capability --usl 10.05 --lsl 9.95 \
    --values 10.01,9.99,10.02,9.98,10.00,10.01,9.99,10.00,10.02,9.98
```

### 10.7 Quality Management (NCR, CAPA, Deviation)

All created through interactive wizards:

```sh
sysml quality list                     # show item types and workflows
sysml quality create --type ncr        # interactive NCR creation
sysml quality create --type capa       # interactive CAPA creation
sysml quality create --type deviation  # interactive deviation request
sysml quality rca --source NCR-001 --method five-why
sysml quality rca --source NCR-001 --method fishbone
sysml quality action --capa CAPA-001
sysml quality trend --group-by category
```

## Part 11: Export

```sh
sysml export interfaces model.sysml --part Controller   # FMI 3.0 interfaces
sysml export list model.sysml                           # list exportable parts
sysml export modelica model.sysml --part Controller -o Controller.mo
sysml export ssp model.sysml -o system.ssd              # SSP XML
```

## Part 12: Cross-Domain Reports

```sh
sysml report dashboard model.sysml requirements.sysml verification.sysml
sysml report traceability requirements.sysml verification.sysml \
    --requirement TemperatureAccuracy
sysml report gate model.sysml requirements.sysml verification.sysml \
    --gate-name CDR --min-coverage 80
```

## Part 13: Pipelines and CI

### 13.1 Define a pipeline

```sh
sysml pipeline create ci
```

Or add manually to `.sysml/config.toml`:

```toml
[[pipeline]]
name = "ci"
steps = [
    "lint model.sysml requirements.sysml",
    "fmt --check model.sysml",
    "trace --check --min-coverage 80 requirements.sysml",
]
```

### 13.2 Run a pipeline

```sh
sysml pipeline list
sysml pipeline run ci --dry-run      # preview
sysml pipeline run ci                # execute
```

### 13.3 GitHub Actions

```yaml
name: SysML Model Validation
on: [push, pull_request]
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { submodules: recursive }
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install --path crates/sysml-cli
      - run: sysml pipeline run ci
```

### 13.4 JSON output

```sh
sysml lint -f json model.sysml
sysml list -f json model.sysml
sysml trace -f json requirements.sysml
sysml stats -f json model.sysml
```

## Part 14: Multi-File Models

```sh
# Pass all files together
sysml lint model.sysml requirements.sysml verification.sysml

# Or include directories
sysml lint model.sysml -I libraries/

# Standard library path (flag, env var, or config)
sysml lint model.sysml --stdlib-path /path/to/sysml-stdlib
export SYSML_STDLIB_PATH=/path/to/sysml-stdlib

# Build a project index
sysml index
sysml index --stats
```

## Part 15: Shell Completions

```sh
sysml completions bash > ~/.local/share/bash-completion/completions/sysml
sysml completions zsh > ~/.zfunc/_sysml
sysml completions fish > ~/.config/fish/completions/sysml.fish
```

## Quick Reference

| Task | Command |
|------|---------|
| Interactive wizard | `sysml add` |
| Add definition to file | `sysml add model.sysml part-def Name` |
| Add usage inside def | `sysml add model.sysml part name -t Type --inside Parent` |
| Add connection | `sysml add model.sysml connection c1 --connect "a.x to b.y" --inside Assy` |
| Add enum with members | `sysml add model.sysml enum-def Color -m red -m green -m blue` |
| Add satisfy relationship | `sysml add model.sysml satisfy ReqName -t Element` |
| Add import | `sysml add model.sysml import "Pkg::*"` |
| Generate to stdout | `sysml add --stdout part-def Name` |
| Learn SysML syntax | `sysml add --stdout --teach part-def Name` |
| Remove element | `sysml remove model.sysml Name` |
| Rename element | `sysml rename model.sysml Old New` |
| Validate a model | `sysml lint model.sysml` |
| List all elements | `sysml list model.sysml` |
| Show element details | `sysml show model.sysml Vehicle` |
| BDD diagram | `sysml diagram -t bdd model.sysml` |
| IBD diagram | `sysml diagram -t ibd --scope Part model.sysml` |
| Simulate state machine | `sysml simulate sm model.sysml -e event1,event2` |
| Evaluate constraint | `sysml simulate eval model.sysml -n Name -b var=value` |
| Requirements trace | `sysml trace requirements.sysml` |
| Format file | `sysml fmt model.sysml` |
| Risk matrix | `sysml risk matrix model.sysml -I libraries/` |
| BOM rollup | `sysml bom rollup model.sysml --root Part` |
| SPC analysis | `sysml mfg spc --parameter Name --values 1,2,3` |
| Run CI pipeline | `sysml pipeline run ci` |
| JSON output | Add `-f json` to most commands |

## What Requires Hand-Editing

The `sysml add` command generates most SysML constructs, but some complex syntax currently requires manual editing after the CLI creates the skeleton:

| Construct | CLI creates | You add by hand |
|-----------|------------|-----------------|
| State machine transitions | `state-def` skeleton | `transition first A accept E then B;` |
| Action successions | `action-def` skeleton | `first step1 then step2;` |
| Constraint expressions | `constraint-def` skeleton | `x >= 0 and x <= 100;` |
| Calc return expressions | `calc-def` skeleton | `a * b + c` |
| Verification objectives | `requirement` skeleton | `objective { verify requirement R; }` |
| Attribute redefinitions | `part-def :> Base` | `attribute redefines severity = ...;` |

For all of these, use `sysml add --teach --stdout <kind> <name>` to see the full syntax with explanations before editing.

## Known Limitations

- **Constraint evaluator**: Compound boolean expressions may not evaluate correctly across all constraints simultaneously. Use `-n` to target specific constraints.
- **Action flow simulation**: May produce duplicate step entries in some succession patterns.
- **Import resolution**: Depends on passing all files via `-I` or command-line arguments. No automatic package discovery.
- **BDD diagram**: May generate duplicate entries when a package and part definition share the same name.
- **Interactive commands**: `add` wizard, `verify run`, `quality create`, `mfg start-lot` require a TTY. Use flags to bypass interactivity in CI.
