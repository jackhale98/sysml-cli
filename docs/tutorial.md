# Tutorial: Building, Editing, and Analyzing a SysML v2 Model

This tutorial walks through a complete systems engineering workflow using the `sysml` command-line tool. You will build a model from scratch, validate it, generate diagrams, run simulations, and use lifecycle management features.

We will model a **weather station** — a small embedded system with sensors, a controller, and a display.

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

Create a new directory and initialize it as a sysml project:

```sh
mkdir weather-station && cd weather-station
sysml init
```

This creates a `.sysml/` directory with a `config.toml` file. The tool auto-detects your model root and project name from the directory.

### 1.2 Explore help topics

If you are new to SysML v2, read the built-in guides:

```sh
sysml guide                    # list available topics
sysml guide getting-started    # first-time tutorial
sysml guide sysml-basics       # SysML v2 language overview
sysml guide requirements       # requirements management
```

### 1.3 Generate an example project

To see what a complete SysML project looks like, generate one of the built-in examples:

```sh
sysml example --list
sysml example brake-system -o /tmp/brake-example
```

This creates multiple `.sysml` files with teaching comments. Feel free to explore them.

## Part 2: Creating the Model

### 2.1 Create the main model file

SysML v2 uses a textual notation. Create a file called `model.sysml`:

```sysml
// Weather Station Model
// A small embedded system for environmental monitoring

package WeatherStation {

    // ---------------------------------------------------------------
    // Attribute definitions — reusable value types
    // ---------------------------------------------------------------

    attribute def TemperatureValue;
    attribute def HumidityValue;
    attribute def PressureValue;
    attribute def WindSpeedValue;

    // ---------------------------------------------------------------
    // Enumeration — fixed set of choices
    // ---------------------------------------------------------------

    enum def DisplayMode {
        enum summary;
        enum detailed;
        enum alert;
    }

    enum def SensorStatus {
        enum ok;
        enum degraded;
        enum failed;
    }

    // ---------------------------------------------------------------
    // Port definitions — interaction points
    // ---------------------------------------------------------------

    port def SensorDataPort {
        out item reading : ScalarValues::Real;
    }

    port def DisplayDataPort {
        in item displayValue : ScalarValues::Real;
    }

    port def PowerPort {
        in item voltage : ScalarValues::Real;
    }

    // ---------------------------------------------------------------
    // Part definitions — reusable component types
    // ---------------------------------------------------------------

    abstract part def Sensor {
        doc /* Base type for all sensors */
        attribute status : SensorStatus;
        attribute sampleRate : ScalarValues::Real;
        port dataOut : SensorDataPort;
        port power : PowerPort;
    }

    part def TemperatureSensor :> Sensor {
        doc /* Measures ambient temperature in degrees Celsius */
        attribute range_min : ScalarValues::Real;
        attribute range_max : ScalarValues::Real;
    }

    part def HumiditySensor :> Sensor {
        doc /* Measures relative humidity as a percentage */
        attribute accuracy : ScalarValues::Real;
    }

    part def PressureSensor :> Sensor {
        doc /* Measures barometric pressure in hPa */
    }

    part def WindSensor :> Sensor {
        doc /* Measures wind speed in m/s and direction */
        attribute maxSpeed : ScalarValues::Real;
    }

    part def Controller {
        doc /* Central processing unit that reads sensor data and drives the display */
        port tempIn : ~SensorDataPort;
        port humidIn : ~SensorDataPort;
        port pressIn : ~SensorDataPort;
        port windIn : ~SensorDataPort;
        port displayOut : ~DisplayDataPort;
        port power : PowerPort;
        attribute firmware_version : ScalarValues::String;
    }

    part def Display {
        doc /* LCD display for showing weather readings */
        port dataIn : DisplayDataPort;
        port power : PowerPort;
        attribute mode : DisplayMode;
        attribute brightness : ScalarValues::Real;
    }

    part def PowerSupply {
        doc /* Solar-powered battery pack */
        attribute capacity_ah : ScalarValues::Real;
        attribute voltage : ScalarValues::Real;
    }

    part def Enclosure {
        doc /* Weather-resistant outdoor housing */
        attribute material : ScalarValues::String;
        attribute ip_rating : ScalarValues::String;
    }

    // ---------------------------------------------------------------
    // Connection definition
    // ---------------------------------------------------------------

    connection def SensorConnection {
        end source : Sensor;
        end target : Controller;
    }

    // ---------------------------------------------------------------
    // Main assembly
    // ---------------------------------------------------------------

    part def WeatherStationUnit {
        doc /* Complete weather station assembly */

        part tempSensor : TemperatureSensor;
        part humiditySensor : HumiditySensor;
        part pressureSensor : PressureSensor;
        part windSensor : WindSensor;
        part controller : Controller;
        part display : Display;
        part power : PowerSupply;
        part enclosure : Enclosure;

        // Sensor-to-controller connections
        connection tempConn : SensorConnection
            connect tempSensor.dataOut to controller.tempIn;

        connection humidConn : SensorConnection
            connect humiditySensor.dataOut to controller.humidIn;

        connection pressConn : SensorConnection
            connect pressureSensor.dataOut to controller.pressIn;

        connection windConn : SensorConnection
            connect windSensor.dataOut to controller.windIn;

        // Controller-to-display connection
        connection displayConn
            connect controller.displayOut to display.dataIn;
    }
}
```

### 2.2 Validate the model

Run the linter to check for structural issues:

```sh
sysml lint model.sysml
```

You should see output like:

```
model.sysml:X:Y: note[W001]: part def `Enclosure` is defined but never referenced
...
Found 0 errors, N warnings, M notes.
```

Notes about unused definitions are normal at this stage — we have not added requirements or verification yet.

To suppress notes and only see warnings and errors:

```sh
sysml lint --severity warning model.sysml
```

### 2.3 List model elements

View all definitions and usages:

```sh
sysml list model.sysml
```

Filter by kind:

```sh
sysml list --kind parts model.sysml       # part definitions only
sysml list --kind ports model.sysml       # port definitions only
sysml list --kind requirements model.sysml # (none yet)
```

Filter by parent:

```sh
sysml list --parent WeatherStationUnit model.sysml
```

### 2.4 Inspect a specific element

```sh
sysml show model.sysml WeatherStationUnit
```

Output shows the kind, location, members, and relationships. To extract the raw SysML source text:

```sh
sysml show --raw model.sysml TemperatureSensor
```

### 2.5 View model statistics

```sh
sysml stats model.sysml
```

This shows element counts by kind, relationship counts, documentation coverage, and nesting depth.

## Part 3: Adding Elements with the CLI

The `sysml add` command can insert elements into files without hand-editing SysML syntax.

### 3.1 Add a new part definition

```sh
sysml add model.sysml part-def RainGauge --doc "Measures rainfall in mm/hr" --extends Sensor
```

This inserts a new `part def RainGauge :> Sensor` at the end of the file.

### 3.2 Add a usage inside a definition

```sh
sysml add model.sysml part rainGauge -t RainGauge --inside WeatherStationUnit
```

This adds `part rainGauge : RainGauge;` inside the `WeatherStationUnit` definition body.

### 3.3 Preview changes with --dry-run

Before writing, preview the diff:

```sh
sysml add model.sysml part-def Anemometer --doc "Wind direction sensor" --dry-run
```

### 3.4 Generate to stdout

Generate SysML text without modifying any file:

```sh
sysml add --stdout part-def GPSSensor --doc "Location tracking" \
    -m "attribute latitude:Real" -m "attribute longitude:Real"
```

Output:

```sysml
part def GPSSensor {
    doc /* Location tracking */
    attribute latitude : Real;
    attribute longitude : Real;
}
```

### 3.5 Learn SysML syntax with --teach

If you are learning SysML v2, add `--teach` to get explanatory comments:

```sh
sysml add --stdout --teach part-def Motor
```

### 3.6 Remove and rename elements

Remove an element:

```sh
sysml remove model.sysml RainGauge --dry-run    # preview first
sysml remove model.sysml RainGauge              # apply
```

Rename an element and update all references:

```sh
sysml rename model.sysml WindSensor Anemometer --dry-run
```

## Part 4: Requirements and Traceability

### 4.1 Add requirements

Create a file called `requirements.sysml`:

```sysml
// Weather Station Requirements

package WeatherStationRequirements {

    import WeatherStation::*;

    requirement def TemperatureAccuracy {
        doc /* The temperature sensor shall measure temperature
               with an accuracy of +/- 0.5 degrees Celsius */
        subject station : WeatherStationUnit;
    }

    requirement def OperatingRange {
        doc /* The weather station shall operate in temperatures
               from -40C to +60C */
        subject station : WeatherStationUnit;
    }

    requirement def BatteryLife {
        doc /* The weather station shall operate for at least
               72 hours without solar charging */
        subject station : WeatherStationUnit;
    }

    requirement def UpdateRate {
        doc /* The display shall update readings at least
               every 5 seconds */
        subject station : WeatherStationUnit;
    }

    requirement def IPRating {
        doc /* The enclosure shall achieve IP65 or higher rating */
        subject station : WeatherStationUnit;
    }

    // Satisfaction: link requirements to implementation
    satisfy requirement TemperatureAccuracy by WeatherStationUnit;
    satisfy requirement OperatingRange by WeatherStationUnit;
    satisfy requirement BatteryLife by WeatherStationUnit;
    satisfy requirement UpdateRate by WeatherStationUnit;
    satisfy requirement IPRating by WeatherStationUnit;
}
```

### 4.2 Generate the traceability matrix

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

All requirements are satisfied but none are verified yet. To use this as a CI gate:

```sh
sysml trace --check --min-coverage 80 requirements.sysml
```

### 4.3 Check model coverage

```sh
sysml coverage model.sysml
```

This reports documentation coverage, typed usages, requirement satisfaction, and an overall quality score.

## Part 5: Verification Cases

### 5.1 Add verification cases

Create `verification.sysml`:

```sysml
// Verification Cases

package WeatherStationVerification {

    import WeatherStation::*;
    import WeatherStationRequirements::*;

    verification case def TestTemperatureAccuracy {
        doc /* Verify temperature sensor accuracy against reference thermometer */
        subject station : WeatherStationUnit;
        objective {
            verify requirement TemperatureAccuracy;
        }
    }

    verification case def TestOperatingRange {
        doc /* Environmental chamber test across full temperature range */
        subject station : WeatherStationUnit;
        objective {
            verify requirement OperatingRange;
        }
    }

    verification case def TestBatteryLife {
        doc /* Continuous operation test without solar input */
        subject station : WeatherStationUnit;
        objective {
            verify requirement BatteryLife;
        }
    }
}
```

### 5.2 Check verification coverage

```sh
sysml verify coverage verification.sysml requirements.sysml
sysml verify list verification.sysml
sysml verify status verification.sysml requirements.sysml
```

### 5.3 Execute a verification case interactively

> **Note:** This command requires an interactive terminal — it prompts you step-by-step.

```sh
sysml verify run verification.sysml --case TestTemperatureAccuracy --author "Jane Smith"
```

The tool walks you through each step of the test, collects your pass/fail judgments, and writes a TOML execution record to `.sysml/records/`.

## Part 6: Diagrams

### 6.1 Block Definition Diagram (BDD)

Shows definitions and their relationships:

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

Shows the internal structure of a specific part — requires `--scope`:

```sh
sysml diagram -t ibd --scope WeatherStationUnit model.sysml
```

This shows the parts inside WeatherStationUnit and their connections.

### 6.3 Requirements Diagram

```sh
sysml diagram -t req requirements.sysml
```

Shows requirements with their satisfaction and verification links.

### 6.4 State Machine Diagram

First, add a state machine to the model. Append to `model.sysml`:

```sysml
    // Add inside the WeatherStation package:

    state def StationStates {
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

Then generate the diagram:

```sh
sysml diagram -t stm --scope StationStates model.sysml
```

### 6.5 Activity Diagram

Add an action definition and generate:

```sh
sysml diagram -t act --scope ReadSensors model.sysml
```

> **Note:** The `--scope` flag is needed when a file contains multiple action definitions.

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

Create `constraints.sysml`:

```sysml
constraint def TemperatureLimit {
    in temp : Real;
    temp >= -40 and temp <= 60;
}

constraint def PowerBudget {
    in consumption : Real;
    consumption <= 500;
}

calc def BatteryRuntime {
    in capacity : Real;
    in consumption : Real;
    return hours : Real;
    capacity * 1000 / consumption
}
```

Evaluate constraints with variable bindings:

```sh
sysml simulate eval constraints.sysml -n TemperatureLimit -b temp=25
# Output: constraint TemperatureLimit: satisfied

sysml simulate eval constraints.sysml -n TemperatureLimit -b temp=70
# Output: constraint TemperatureLimit: violated

sysml simulate eval constraints.sysml -n BatteryRuntime -b capacity=12,consumption=200
# Output: calc BatteryRuntime: 60
```

> **Known limitation:** The constraint evaluator has issues with compound boolean
> expressions using `and`/`or` when evaluating all constraints at once. Use `-n` to
> evaluate specific constraints individually for reliable results.

### 7.2 State machine simulation

Using the StationStates defined earlier:

```sh
sysml simulate sm model.sysml -n StationStates -e powerOn,alertTrigger,clearAlert,lowBattery,charged
```

Output shows the step-by-step state transitions:

```
State Machine: StationStates
Initial state: off

  Step 0: off -- [powerOn]--> initializing
  Step 1: initializing --> monitoring
  Step 2: monitoring -- [alertTrigger]--> alerting
  Step 3: alerting -- [clearAlert]--> monitoring
  Step 4: monitoring -- [lowBattery]--> lowPower
  ...
```

Without `--events`, the tool prompts you interactively to select events from the available triggers.

### 7.3 Action flow execution

```sh
sysml simulate af model.sysml -n ReadSensors
```

Traces through action steps following `first ... then ...` succession links.

### 7.4 List simulatable elements

```sh
sysml simulate list model.sysml
```

Shows all constraints, calculations, state machines, and actions in the file.

## Part 8: Analysis Commands

### 8.1 Dependency analysis

See what an element depends on and what references it:

```sh
sysml deps model.sysml WeatherStationUnit
sysml deps model.sysml TemperatureSensor --reverse   # what references this
sysml deps model.sysml Controller --forward           # what this depends on
```

### 8.2 Interface analysis

List all ports and find unconnected ones:

```sh
sysml interfaces model.sysml
sysml interfaces --unconnected model.sysml
```

Unconnected ports represent interface gaps — things that should probably be connected.

### 8.3 Allocation analysis

If your model uses allocation relationships (mapping logical actions to physical parts):

```sh
sysml allocation model.sysml
sysml allocation --check model.sysml    # fail if unallocated items exist
```

### 8.4 Semantic diff

Compare two versions of a model:

```sh
cp model.sysml model-v2.sysml
# (make some changes to model-v2.sysml)
sysml diff model.sysml model-v2.sysml
```

Unlike text-based diff, this compares at the model level — detecting structural changes regardless of formatting.

## Part 9: Formatting

### 9.1 Format files

The formatter uses the Concrete Syntax Tree (CST) for accurate, structure-aware formatting:

```sh
sysml fmt model.sysml
```

Preview changes without writing:

```sh
sysml fmt --diff model.sysml
```

Use in CI to enforce formatting:

```sh
sysml fmt --check model.sysml    # exit 1 if not formatted
```

Customize indentation:

```sh
sysml fmt --indent-width 2 model.sysml
```

## Part 10: Lifecycle Management

These commands work with domain library types. The tool ships with library files in `libraries/` that define base types for risk, tolerance, BOM, manufacturing, and quality.

### 10.1 Risk Management

Create a risk model file `risks.sysml` using the domain library patterns:

```sysml
package WeatherStationRisks {
    import WeatherStation::*;

    part def riskMoistureIngress :> SysMLRisk::RiskDef {
        doc /* Moisture entering the enclosure could damage electronics */
        attribute redefines severity = SysMLRisk::SeverityLevel::critical;
        attribute redefines likelihood = SysMLRisk::LikelihoodLevel::occasional;
    }

    part def riskSolarFailure :> SysMLRisk::RiskDef {
        doc /* Solar panel degradation reduces charging capability */
        attribute redefines severity = SysMLRisk::SeverityLevel::moderate;
        attribute redefines likelihood = SysMLRisk::LikelihoodLevel::remote;
    }
}
```

Analyze risks:

```sh
sysml risk list risks.sysml -I libraries/
sysml risk matrix risks.sysml -I libraries/
sysml risk fmea risks.sysml -I libraries/
```

> **Interactive:** `sysml risk add` launches a wizard that prompts for severity, likelihood, and other fields, then generates the SysML text for you.

### 10.2 Tolerance Analysis

For models with tolerance dimension chains (using the `SysMLTolerance` library):

```sh
sysml tol analyze model.sysml -I libraries/                     # worst-case
sysml tol analyze model.sysml -I libraries/ --method rss        # root sum of squares
sysml tol analyze model.sysml -I libraries/ --method monte-carlo --iterations 50000
sysml tol sensitivity model.sysml -I libraries/
```

### 10.3 Bill of Materials

For models with BOM attributes (using the `SysMLBOM` library):

```sh
sysml bom rollup model.sysml --root WeatherStationUnit -I libraries/
sysml bom rollup model.sysml --root WeatherStationUnit --include-mass --include-cost -I libraries/
sysml bom where-used model.sysml --part TemperatureSensor -I libraries/
sysml bom export model.sysml --root WeatherStationUnit -I libraries/   # CSV output
```

### 10.4 Supplier Management

```sh
sysml source list model.sysml -I libraries/     # list suppliers
sysml source asl model.sysml -I libraries/      # approved source list
sysml source rfq --part TemperatureSensor --quantity 1000 --description "Industrial temp sensor, -40 to +60C"
```

The `rfq` command generates a request-for-quotation document.

### 10.5 Manufacturing Execution

For models with manufacturing routing definitions:

```sh
sysml mfg list model.sysml -I libraries/
```

Statistical Process Control on measurement data:

```sh
sysml mfg spc --parameter SensorCalibration --values 0.48,0.52,0.50,0.49,0.51,0.50,0.53,0.47,0.51,0.49
```

Output includes mean, standard deviation, UCL/LCL control limits, and a visual SPC chart.

> **Interactive:** `sysml mfg start-lot` creates a production lot record, and `sysml mfg step <lot-id>` advances through each routing step with parameter recording.

### 10.6 Quality Control

ANSI Z1.4 sampling plan lookup:

```sh
sysml qc sample-size --lot-size 500
sysml qc sample-size --lot-size 500 --aql 0.65 --level tightened
```

Process capability (Cp/Cpk) analysis:

```sh
sysml qc capability --usl 10.05 --lsl 9.95 --values 10.01,9.99,10.02,9.98,10.00,10.01,9.99,10.00,10.02,9.98
```

### 10.7 Quality Management (NCR, CAPA, Deviation)

The tool manages three quality item types with distinct lifecycles:

- **NCR** (Nonconformance Report): documents observed problems
- **CAPA** (Corrective/Preventive Action): formal improvement programs
- **Process Deviation**: approved departures from standard processes

```sh
sysml quality list                    # show item types and workflows
sysml quality create --type ncr       # interactive NCR creation wizard
sysml quality create --type capa      # interactive CAPA creation
sysml quality create --type deviation # interactive deviation request
```

Root cause analysis:

```sh
sysml quality rca --source NCR-001 --method five-why
sysml quality rca --source NCR-001 --method fishbone
```

Add corrective actions to a CAPA:

```sh
sysml quality action --capa CAPA-001
```

Trend analysis:

```sh
sysml quality trend --group-by category
sysml quality trend --group-by severity model.sysml
```

## Part 11: Export

### 11.1 FMI 3.0 interfaces

Extract interface items for co-simulation:

```sh
sysml export interfaces model.sysml --part Controller
sysml export list model.sysml    # list exportable parts
```

### 11.2 Modelica stubs

Generate a Modelica partial model:

```sh
sysml export modelica model.sysml --part Controller -o Controller.mo
```

### 11.3 SSP (System Structure Package)

Generate SystemStructureDescription XML:

```sh
sysml export ssp model.sysml -o system.ssd
```

## Part 12: Cross-Domain Reports

### 12.1 Project dashboard

```sh
sysml report dashboard model.sysml requirements.sysml verification.sysml
```

Shows an executive summary combining model statistics, requirement coverage, risk status, and quality items.

### 12.2 Requirement traceability thread

Trace a single requirement through the full lifecycle:

```sh
sysml report traceability requirements.sysml verification.sysml --requirement TemperatureAccuracy
```

### 12.3 Gate readiness review

Check whether a project milestone is ready:

```sh
sysml report gate model.sysml requirements.sysml verification.sysml \
    --gate-name CDR --min-coverage 80
```

## Part 13: Pipelines and CI

### 13.1 Define a CI pipeline

Add to `.sysml/config.toml`:

```toml
[[pipeline]]
name = "ci"
steps = [
    "lint model.sysml requirements.sysml verification.sysml",
    "fmt --check model.sysml requirements.sysml verification.sysml",
    "trace --check --min-coverage 80 requirements.sysml",
    "coverage --check --min-score 60 model.sysml"
]

[[pipeline]]
name = "pre-commit"
steps = [
    "lint model.sysml",
    "fmt --check model.sysml"
]
```

Or create one interactively:

```sh
sysml pipeline create ci
```

### 13.2 Run a pipeline

```sh
sysml pipeline list                  # show defined pipelines
sysml pipeline run ci --dry-run      # preview commands
sysml pipeline run ci                # execute all steps in order
```

The pipeline stops at the first failing step with a non-zero exit code — suitable for CI integration.

### 13.3 GitHub Actions example

```yaml
name: SysML Model Validation
on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install sysml
        run: cargo install --path crates/sysml-cli

      - name: Run CI pipeline
        run: sysml pipeline run ci
```

### 13.4 JSON output for tooling

Most commands support JSON output for integration with other tools:

```sh
sysml lint -f json model.sysml
sysml list -f json model.sysml
sysml trace -f json requirements.sysml
sysml stats -f json model.sysml
sysml coverage -f json model.sysml
```

## Part 14: Multi-File Models

### 14.1 Cross-file import resolution

When definitions are spread across multiple files, pass all files together:

```sh
sysml lint model.sysml requirements.sysml verification.sysml
```

Or use the `-I` flag to include additional directories:

```sh
sysml lint model.sysml -I libraries/
```

### 14.2 Standard library path

Set a standard library location via config, flag, or environment variable:

```sh
# Flag
sysml lint model.sysml --stdlib-path /path/to/sysml-stdlib

# Environment variable
export SYSML_STDLIB_PATH=/path/to/sysml-stdlib
sysml lint model.sysml

# Config (.sysml/config.toml)
# [project]
# stdlib_path = "/path/to/sysml-stdlib"
```

### 14.3 Building the project index

For large projects, build a cache of all elements:

```sh
sysml index
sysml index --stats
```

With the optional SQLite feature (`--features sqlite` at build time), the index persists to `.sysml/cache.db` for faster startup.

## Part 15: Shell Completions

Generate tab-completion for your shell:

```sh
sysml completions bash > ~/.local/share/bash-completion/completions/sysml
sysml completions zsh > ~/.zfunc/_sysml
sysml completions fish > ~/.config/fish/completions/sysml.fish
```

## Quick Reference

| Task | Command |
|------|---------|
| Validate a model | `sysml lint model.sysml` |
| List all elements | `sysml list model.sysml` |
| Show element details | `sysml show model.sysml Vehicle` |
| Generate BDD diagram | `sysml diagram -t bdd model.sysml` |
| Generate IBD diagram | `sysml diagram -t ibd --scope Part model.sysml` |
| Generate STM diagram | `sysml diagram -t stm --scope Machine model.sysml` |
| Simulate state machine | `sysml simulate sm model.sysml -e event1,event2` |
| Evaluate constraint | `sysml simulate eval model.sysml -n Name -b var=value` |
| Requirements trace | `sysml trace requirements.sysml` |
| Model coverage | `sysml coverage model.sysml` |
| Add element to file | `sysml add model.sysml part-def Name` |
| Add via wizard | `sysml add` |
| Generate to stdout | `sysml add --stdout part-def Name` |
| Remove element | `sysml remove model.sysml Name` |
| Rename element | `sysml rename model.sysml Old New` |
| Format file | `sysml fmt model.sysml` |
| Risk matrix | `sysml risk matrix model.sysml` |
| BOM rollup | `sysml bom rollup model.sysml --root Part` |
| SPC analysis | `sysml mfg spc --parameter Name --values 1,2,3` |
| Cp/Cpk analysis | `sysml qc capability --usl 10.05 --lsl 9.95 --values ...` |
| Run CI pipeline | `sysml pipeline run ci` |
| Initialize project | `sysml init` |
| JSON output | Add `-f json` to most commands |

## Known Limitations and Future Work

- **Constraint evaluator**: Compound boolean expressions (`a >= x and a <= y`) may not evaluate correctly when run across all constraints simultaneously. Use `-n` to target specific constraints.
- **Action flow simulation**: The action flow simulator may produce duplicate step entries in some succession patterns. This affects display output only.
- **Import resolution**: The `import` keyword is recognized but cross-package type resolution depends on passing all relevant files via `-I` or command-line arguments. There is no automatic package discovery from `import` statements alone.
- **View filtering**: The `--view` flag reads view definitions from the model but only supports `filter @SysML::PartUsage`-style patterns — not arbitrary constraint expressions.
- **BDD diagram**: When a file contains both a package and a part definition with the same name, the diagram may generate duplicate class entries.
- **Interactive commands**: Commands requiring user input (`add` wizard, `verify run`, `quality create`, `mfg start-lot`, `mfg step`) require a TTY. They cannot run in non-interactive CI environments. Use flags to bypass interactivity where available.
