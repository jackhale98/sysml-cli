# Tutorial: Building and Managing a SysML v2 Model

Build a complete systems engineering model from scratch using the `sysml` interactive wizard. You'll model a **weather station** — an embedded system with sensors, a controller, and a display — then validate, diagram, simulate, and analyze it.

> **Two ways to use sysml:** The interactive wizard (`sysml add`) is the primary workflow — it guides you through creating elements with model-aware suggestions. Every wizard action has an equivalent flag-based command for scripting and CI. This tutorial shows both.

## Prerequisites

```sh
git clone --recurse-submodules https://github.com/jackhale98/sysml-cli.git
cd sysml-cli && cargo install --path crates/sysml-cli
sysml --version
```

## Part 1: Project Setup

```sh
mkdir weather-station && cd weather-station
sysml init
```

This creates `.sysml/config.toml`. If a `libraries/` directory exists, it is automatically configured for import resolution.

Copy the domain libraries into your project:

```sh
cp -r /path/to/sysml-cli/libraries .
sysml init --force    # Re-init to detect libraries/
```

These are pure SysML v2 packages — usable from any conformant tool, not
just this CLI. The engineering-analysis set (`Tolerancing`, `Uncertainty`,
`RiskAnalysis`, `HazardAnalysis`) covers toleranced dimensions and GD&T,
tolerance stackups, FMEA, and RAAML-aligned hazard analysis; see
`libraries/README.md` for provenance and the design docs in the
sysml-domain-libraries project.

Explore built-in help with `sysml --help` and `sysml <command> --help`.

## Part 2: Building the Model

In SysML v2, **definitions** (`part def Sensor`) are reusable types. **Usages** (`part tempSensor : Sensor`) are instances placed inside an assembly.

### 2.1 Create the model file

```sh
sysml add --stdout package WeatherStation --doc "Weather station model" > model.sysml
```

> This is the one case where `--stdout >` is needed — we're creating the file itself. Everything else uses `sysml add model.sysml`.

### 2.2 Interactive mode — the fastest way to build

Launch the wizard with a file to get model-aware suggestions:

```
$ sysml add model.sysml
? Where will this element go? > Add to an existing file
? What are you creating? > Enumeration
? Name: SensorStatus
? Enum members (comma-separated): ok,degraded,failed

Preview:
  enum def SensorStatus {
      enum ok;
      enum degraded;
      enum failed;
  }

Wrote SensorStatus to model.sysml
```

Repeat for `DisplayMode`:

```
$ sysml add model.sysml
? What are you creating? > Enumeration
? Name: DisplayMode
? Enum members: summary,detailed,alert
```

### 2.3 Add port definitions

Using flags (faster for known structures):

```sh
sysml add model.sysml port-def SensorDataPort -m "out item reading:ScalarValues::Real"
sysml add model.sysml port-def DisplayDataPort -m "in item displayValue:ScalarValues::Real"
sysml add model.sysml port-def PowerPort -m "in item voltage:ScalarValues::Real"
```

### 2.4 Add part definitions

Create an abstract base sensor:

```sh
sysml add model.sysml part-def Sensor --abstract \
    --doc "Base type for all sensors" \
    -m "attribute status:SensorStatus,attribute sampleRate:ScalarValues::Real" \
    -m "port dataOut:SensorDataPort,port power:PowerPort"
```

Specialize it (the wizard shows available supertypes from your model):

```sh
sysml add model.sysml part-def TemperatureSensor --extends Sensor \
    --doc "Measures ambient temperature" \
    -m "attribute range_min:ScalarValues::Real,attribute range_max:ScalarValues::Real"

sysml add model.sysml part-def HumiditySensor --extends Sensor \
    --doc "Measures relative humidity"

sysml add model.sysml part-def PressureSensor --extends Sensor \
    --doc "Measures barometric pressure"

sysml add model.sysml part-def WindSensor --extends Sensor \
    --doc "Measures wind speed and direction"
```

Remaining components:

```sh
sysml add model.sysml part-def Controller \
    --doc "Central processing unit" \
    -m "port tempIn:SensorDataPort,port humidIn:SensorDataPort" \
    -m "port pressIn:SensorDataPort,port windIn:SensorDataPort" \
    -m "port displayOut:DisplayDataPort,port power:PowerPort"

sysml add model.sysml part-def Display \
    --doc "LCD display" \
    -m "port dataIn:DisplayDataPort,port power:PowerPort" \
    -m "attribute mode:DisplayMode"

sysml add model.sysml part-def PowerSupply \
    --doc "Solar-powered battery pack" \
    -m "attribute capacity_ah:ScalarValues::Real,attribute voltage:ScalarValues::Real"

sysml add model.sysml part-def Enclosure \
    --doc "Weather-resistant housing" \
    -m "attribute ip_rating:ScalarValues::String"
```

### 2.5 Build the assembly with part usages

Create the top-level assembly definition, then add part usages inside it:

```sh
sysml add model.sysml part-def WeatherStationUnit \
    --doc "Complete weather station assembly"

# Part usages (instances inside the assembly)
sysml add model.sysml part tempSensor -t TemperatureSensor --inside WeatherStationUnit
sysml add model.sysml part humiditySensor -t HumiditySensor --inside WeatherStationUnit
sysml add model.sysml part pressureSensor -t PressureSensor --inside WeatherStationUnit
sysml add model.sysml part windSensor -t WindSensor --inside WeatherStationUnit
sysml add model.sysml part controller -t Controller --inside WeatherStationUnit
sysml add model.sysml part display -t Display --inside WeatherStationUnit
sysml add model.sysml part power -t PowerSupply --inside WeatherStationUnit
sysml add model.sysml part enclosure -t Enclosure --inside WeatherStationUnit
```

### 2.6 Add connections

```sh
sysml add model.sysml connection tempConn \
    --connect "tempSensor.dataOut to controller.tempIn" --inside WeatherStationUnit

sysml add model.sysml connection humidConn \
    --connect "humiditySensor.dataOut to controller.humidIn" --inside WeatherStationUnit

sysml add model.sysml connection pressConn \
    --connect "pressureSensor.dataOut to controller.pressIn" --inside WeatherStationUnit

sysml add model.sysml connection windConn \
    --connect "windSensor.dataOut to controller.windIn" --inside WeatherStationUnit

sysml add model.sysml connection displayConn \
    --connect "controller.displayOut to display.dataIn" --inside WeatherStationUnit
```

### 2.7 Add a state machine

```sh
sysml add model.sysml state-def StationStates \
    --doc "Operating states" \
    -m "entry; then off;" \
    -m "state off,state initializing,state monitoring,state alerting,state lowPower" \
    -m "transition first off accept powerOn then initializing" \
    -m "transition first initializing then monitoring" \
    -m "transition first monitoring accept alertTrigger then alerting" \
    -m "transition first alerting accept clearAlert then monitoring" \
    -m "transition first monitoring accept lowBattery then lowPower" \
    -m "transition first lowPower accept charged then monitoring"
```

### 2.8 Add an action definition

```sh
sysml add model.sysml action-def ReadSensors \
    --doc "Read all sensors and update display" \
    -m "action readTemp,action readHumidity,action readPressure,action readWind" \
    -m "action processData,action updateDisplay" \
    -m "first readTemp then readHumidity" \
    -m "first readHumidity then readPressure" \
    -m "first readPressure then readWind" \
    -m "first readWind then processData" \
    -m "first processData then updateDisplay"
```

### 2.9 Add constraints and calculations

```sh
sysml add model.sysml constraint-def TemperatureLimit \
    --doc "Operating temperature range" \
    -m "in attribute temp:ScalarValues::Real" \
    -m "constraint temp >= -40 and temp <= 60"

sysml add model.sysml constraint-def PowerBudget \
    --doc "Maximum power consumption" \
    -m "in attribute consumption:ScalarValues::Real" \
    -m "constraint consumption <= 500"

sysml add model.sysml calc-def BatteryRuntime \
    --doc "Calculate battery runtime in hours" \
    -m "in attribute capacity:ScalarValues::Real" \
    -m "in attribute consumption:ScalarValues::Real" \
    -m "return hours:ScalarValues::Real"
```

### 2.10 Validate and explore

```sh
$ sysml check model.sysml
model.sysml:42:5: note[W001]: part def `WindSensor` is defined but never referenced
model.sysml:106:5: note[W001]: state def `StationStates` is defined but never referenced
Found 0 errors, 2 notes.
```

```sh
$ sysml list --kind parts model.sysml
  part def       Sensor (in WeatherStation) [model.sysml:31]
  part def       TemperatureSensor : Sensor (in WeatherStation) [model.sysml:36]
  part def       HumiditySensor : Sensor (in WeatherStation) [model.sysml:42]
  part def       PressureSensor : Sensor (in WeatherStation) [model.sysml:47]
  part def       WindSensor : Sensor (in WeatherStation) [model.sysml:52]
  part def       Controller (in WeatherStation) [model.sysml:56]
  part def       Display (in WeatherStation) [model.sysml:65]
  part def       PowerSupply (in WeatherStation) [model.sysml:70]
  part def       Enclosure (in WeatherStation) [model.sysml:75]
  part def       WeatherStationUnit (in WeatherStation) [model.sysml:80]
10 element(s) found.
```

```sh
$ sysml show model.sysml WeatherStationUnit
part def WeatherStationUnit
  parent: WeatherStation
  location: model.sysml:80:5
  doc: Complete weather station assembly
  members:
    part tempSensor : TemperatureSensor
    part humiditySensor : HumiditySensor
    part pressureSensor : PressureSensor
    part windSensor : WindSensor
    part controller : Controller
    part display : Display
    part power : PowerSupply
    part enclosure : Enclosure
    connection tempConn
    connection humidConn
    connection pressConn
    connection windConn
    connection displayConn
```

Model statistics are a model-defined view (`ModelStats`, from `libraries/StandardViews.sysml`):

```sh
$ sysml view ModelStats -I libraries model.sysml
kind             definitions  usages
------------------------------------
action def       1            6
calc def         1            0
constraint def   2            0
enum def         2            0
package          1            0
part def         10           8
port def         3            12
state def        1            5
```

## Part 3: Requirements and Traceability

### 3.1 Create the requirements file

```sh
sysml add --stdout package WeatherStationRequirements \
    --doc "Weather station requirements" > requirements.sysml

sysml add requirements.sysml import "WeatherStation::*"
```

### 3.2 Add requirements

```sh
sysml add requirements.sysml requirement TemperatureAccuracy \
    --doc "Temperature sensor shall measure with +/- 0.5C accuracy"

sysml add requirements.sysml requirement OperatingRange \
    --doc "Station shall operate from -40C to +60C"

sysml add requirements.sysml requirement BatteryLife \
    --doc "Station shall operate 72 hours without solar charging"

sysml add requirements.sysml requirement UpdateRate \
    --doc "Display shall update readings every 5 seconds"

sysml add requirements.sysml requirement IPRating \
    --doc "Enclosure shall achieve IP65 or higher"
```

### 3.3 Link requirements to implementation

```sh
sysml add requirements.sysml satisfy TemperatureAccuracy --by TemperatureSensor
sysml add requirements.sysml satisfy OperatingRange --by WeatherStationUnit
sysml add requirements.sysml satisfy BatteryLife --by PowerSupply
sysml add requirements.sysml satisfy UpdateRate --by Controller
sysml add requirements.sysml satisfy IPRating --by Enclosure
```

### 3.4 Traceability matrix

```sh
$ sysml trace requirements.sysml
Requirement          Satisfied By         Verified By
------------------------------------------------------------
TemperatureAccuracy  TemperatureSensor    -
OperatingRange       WeatherStationUnit   -
BatteryLife          PowerSupply          -
UpdateRate           Controller           -
IPRating             Enclosure            -

Coverage: 5/5 satisfied (100%), 0/5 verified (0%)
```

CI gate: `sysml trace --check --min-coverage 80 requirements.sysml`

## Part 4: Verification

### 4.1 Create verification file

```sh
sysml add --stdout package WeatherStationVerification \
    --doc "Verification cases" > verification.sysml

sysml add verification.sysml import "WeatherStation::*"
sysml add verification.sysml import "WeatherStationRequirements::*"
```

### 4.2 Add verification cases

A verification case defines *what* to verify and the *procedure* to follow. Sub-usages inside the verification def document the procedure steps — the model is the test plan.

```sh
sysml add verification.sysml verification-def TestTemperatureAccuracy \
    --doc "Verify temperature sensor accuracy against reference thermometer" \
    -m "subject testSubject" \
    -m "requirement tempReq:TemperatureAccuracy" \
    -m "action setup" \
    -m "attribute measureAccuracy" \
    -m "action evaluate"

sysml add verification.sysml verification-def TestOperatingRange \
    --doc "Environmental chamber test across full temperature range" \
    -m "subject testSubject" \
    -m "requirement rangeReq:OperatingRange" \
    -m "action configChamber" \
    -m "action runCycle" \
    -m "action checkFunction"

sysml add verification.sysml verification-def TestBatteryLife \
    --doc "Continuous operation test without solar input" \
    -m "subject testSubject" \
    -m "requirement batteryReq:BatteryLife" \
    -m "action disableSolar" \
    -m "action runUntilDepleted" \
    -m "attribute measureRuntime"
```

> Steps are just usages inside the verification def. Use `action` for pass/fail steps and `attribute` (or names with "measure"/"reading") for measurement steps that collect numeric data.

Link verification to requirements:

```sh
sysml add verification.sysml verify TemperatureAccuracy --by TestTemperatureAccuracy
sysml add verification.sysml verify OperatingRange --by TestOperatingRange
sysml add verification.sysml verify BatteryLife --by TestBatteryLife
```

### 4.3 Check verification coverage

The trace matrix now shows both satisfaction and verification:

```sh
$ sysml trace model.sysml requirements.sysml verification.sysml
Requirement          Satisfied By         Verified By
------------------------------------------------------------
TemperatureAccuracy  TemperatureSensor    TestTemperatureAccuracy
OperatingRange       WeatherStationUnit   TestOperatingRange
BatteryLife          PowerSupply          TestBatteryLife
UpdateRate           Controller           -
IPRating             Enclosure            -

Coverage: 5/5 satisfied (100%), 3/5 verified (60%)
```

The same data renders as a model-defined view (exportable with `-f csv` or `-f md`):

```sh
sysml view RequirementsTraceMatrix -I libraries model.sysml requirements.sysml verification.sysml
```

Unverified requirements are also flagged by `sysml check` (W003) and counted in `sysml coverage`.

## Part 5: Diagrams

Generate diagrams in mermaid, PlantUML, Graphviz DOT, or D2 format.

```sh
$ sysml diagram -t stm --scope StationStates model.sysml
---
title: stm [StationStates]
---
stateDiagram-v2
    off : off
    initializing : initializing
    monitoring : monitoring
    alerting : alerting
    lowPower : lowPower
    [*] --> off
    off --> initializing : powerOn
    initializing --> monitoring
    monitoring --> alerting : alertTrigger
    alerting --> monitoring : clearAlert
    monitoring --> lowPower : lowBattery
    lowPower --> monitoring : charged
```

All 7 diagram types:

```sh
sysml diagram -t bdd model.sysml                              # Block definition
sysml diagram -t ibd --scope WeatherStationUnit model.sysml    # Internal blocks
sysml diagram -t stm --scope StationStates model.sysml         # State machine
sysml diagram -t act --scope ReadSensors model.sysml           # Activity
sysml diagram -t req requirements.sysml                        # Requirements
sysml diagram -t pkg model.sysml                               # Package
sysml diagram -t trace model.sysml                             # V-model traceability
```

Other renderers:

```sh
sysml diagram -t bdd -r plantuml model.sysml
sysml diagram -t bdd -r dot model.sysml
sysml diagram -t bdd -r d2 model.sysml
```

## Part 6: Simulation

### 6.1 State machine simulation

```sh
$ sysml simulate sm model.sysml -n StationStates -e powerOn,alertTrigger,clearAlert
State Machine: StationStates
Initial state: off
  Step 0: off -- [powerOn]--> initializing
  Step 1: initializing --> monitoring
  Step 2: monitoring -- [alertTrigger]--> alerting
  Step 3: alerting -- [clearAlert]--> monitoring
```

Without `-e`, it prompts interactively for events.

### 6.2 Constraint evaluation

```sh
$ sysml simulate eval model.sysml -n TemperatureLimit -b temp=25
constraint TemperatureLimit: satisfied

$ sysml simulate eval model.sysml -n TemperatureLimit -b temp=70
constraint TemperatureLimit: violated

$ sysml simulate eval model.sysml -n BatteryRuntime -b capacity=12,consumption=200
calc BatteryRuntime: 60
```

### 6.3 Action flow

```sh
$ sysml simulate af model.sysml -n ReadSensors
Action: ReadSensors

  Step 0: [perform] perform readTemp
  Step 1: [perform] perform readHumidity
  Step 2: [perform] perform readPressure
  Step 3: [perform] perform readWind
  Step 4: [perform] perform processData
  Step 5: [perform] perform updateDisplay

Status: completed (6 steps)
```

## Part 7: Analysis

### 7.1 Dependency analysis

```sh
$ sysml deps model.sysml WeatherStationUnit
Dependency Analysis: WeatherStationUnit
========================================

Referenced by (0):
  (none)

Depends on (8):
  TemperatureSensor (part) via type_ref
  HumiditySensor (part) via type_ref
  PressureSensor (part) via type_ref
  WindSensor (part) via type_ref
  Controller (part) via type_ref
  Display (part) via type_ref
  PowerSupply (part) via type_ref
  Enclosure (part) via type_ref

$ sysml deps model.sysml TemperatureSensor --reverse
Dependency Analysis: TemperatureSensor
========================================

Referenced by (1):
  WeatherStationUnit (part) via type_ref
```

### 7.2 Interface analysis

Port listings are a model-defined view:

```sh
$ sysml view PortTable -I libraries model.sysml
element     parent      type            direction
-------------------------------------------------
dataOut     Sensor      SensorDataPort
power       Sensor      PowerPort
tempIn      Controller  SensorDataPort
...
```

Unconnected ports are flagged by `sysml check` as W016 (`unbound-port`).

### 7.3 Model coverage

```sh
$ sysml coverage model.sysml
Model Coverage Report
==================================================

Unverified requirements (2):
  TemperatureAccuracy
  OperatingRange

Summary:
  Documentation:       67%
  Typed usages:        91%
  Populated defs:      80%
  Req satisfaction:    100%
  Req verification:    0%
  Overall score:       64%
```

### 7.4 Other analysis commands

```sh
sysml allocation model.sysml                         # Allocation matrix
sysml diff model.sysml model-v2.sysml                # Semantic diff
```

## Part 8: Risk Management (FMEA)

Risk analysis is model data, not tool state. The `RiskAnalysis` domain library (`libraries/RiskAnalysis.sysml`) defines an `@Fmea` metadata annotation with the AIAG/VDA worksheet fields, and view defs that render worksheets from it — `sysml view` does the rendering, the model holds everything else.

### 8.1 Annotate failure modes in the model

Attach `@Fmea` annotations to the part the failure mode belongs to (in your editor — models are text):

```sysml
private import RiskAnalysis::*;

part enclosure : Enclosure {
    @Fmea {
        failureMode = "Moisture ingress past IP seal";
        cause = "Seal degradation from UV exposure";
        effect = "Corrosion of internal electronics";
        category = RiskCategory::design;
        severity = 4;
        occurrence = 2;
        detection = 3;
    }
}

part power : PowerSupply {
    @Fmea {
        failureMode = "Solar cell delamination";
        cause = "Thermal cycling stress";
        effect = "Power output degradation";
        severity = 3;
        occurrence = 3;
        detection = 4;
    }
}
```

### 8.2 Render the FMEA worksheet and risk matrix

```sh
$ sysml view FmeaWorksheet -I libraries model.sysml
element    failureMode                    cause                            category              severity  occurrence  detection  rpn
--------------------------------------------------------------------------------------------------------------------------------------
power      Solar cell delamination        Thermal cycling stress           RiskCategory::design  3         3           4          36
enclosure  Moisture ingress past IP seal  Seal degradation from UV expos.  RiskCategory::design  4         2           3          24
2 row(s).

$ sysml view RiskMatrix -I libraries model.sysml
severity\occurrence  2  3
-------------------------
4                    1
3                       1
2 row(s).
```

RPN is a derived column (`rpn = severity*occurrence*detection` in the view's `@TableRendering` spec) — computed at render time, never stored in the model. Export with `-f csv` or `-f md`.

Hazard analysis (MIL-STD-882E / ISO 14971, RAAML-aligned hazard chains) lives in `libraries/HazardAnalysis.sysml` with a `HazardLog` view; the sysml-domain-libraries examples show complete FMEA + hazard-chain models.

## Part 9: Mass and Cost Rollups

Any numeric attribute rolls up over the part hierarchy — the same mechanism serves mass budgets, cost estimates, and power budgets. Multiplicity is the quantity: `part wheels : Wheel[4]` counts four times.

### 9.1 Add the attributes

```sysml
part def TemperatureSensor :> Sensor {
    attribute mass = 0.150 [SI::kg];
    attribute cost = 45.00;
}
```

Values may carry unit brackets; mixed units (kg/g, m/mm) convert automatically into the root's unit.

### 9.2 Compute rollups

```sh
$ sysml rollup compute model.sysml --root WeatherStationUnit --attr mass
Rollup: mass (sum) for WeatherStationUnit
  WeatherStationUnit                        total: 4.9500 [kg]
    tempSensor : TemperatureSensor     0.1500 =>   0.1500 (3.0%)
    ...
    enclosure : Enclosure              2.5000 =>   2.5000 (50.5%)

$ sysml rollup compute model.sysml --root WeatherStationUnit --attr cost
$ sysml rollup budget model.sysml --root WeatherStationUnit --attr mass --limit 5
$ sysml rollup sensitivity model.sysml --root WeatherStationUnit --attr cost
```

`rollup budget` exits non-zero when the limit is exceeded — a ready-made CI gate. To list every element carrying a given attribute, use `sysml list -k attributes -n cost`.

## Part 10: Editing

### 10.1 Add new elements

```sh
sysml add model.sysml part-def RainGauge --extends Sensor --doc "Measures rainfall"
sysml add model.sysml part rainGauge -t RainGauge --inside WeatherStationUnit
```

### 10.2 Preview and multiplicity

```sh
sysml add model.sysml part-def Vehicle \
    -m "part wheels:Wheel[4],attribute doors:Door[2..5]" --dry-run
```

### 10.3 Remove and rename

```sh
sysml remove model.sysml RainGauge --dry-run    # Preview
sysml remove model.sysml RainGauge              # Apply
sysml rename model.sysml WindSensor Anemometer
```

### 10.4 Learn SysML syntax

```sh
$ sysml add --stdout --teach part-def Motor
// A "part def" defines a reusable component type in SysML v2.
// Parts are physical or logical components that make up a system.
// Other definitions can specialize this with `:>` (e.g., ElectricMotor :> Motor).
part def Motor {
    // Add attributes with: attribute name : Type;
    // Add ports with:      port name : PortType;
    // Add nested parts:    part name : PartType;
}
```

## Part 11: Export and Reports

### 11.1 Export

```sh
sysml export modelica model.sysml --part Controller      # Modelica stub
sysml export ssp model.sysml -o system.ssd               # SSP XML
```

### 11.2 Reports

Project status comes from the analysis commands and model-defined views — each exportable as CSV/Markdown/JSON:

```sh
sysml trace model.sysml requirements.sysml verification.sysml   # requirement coverage
sysml coverage model.sysml                                       # quality score
sysml view FmeaWorksheet -I libraries -f md model.sysml          # FMEA worksheet
sysml view ModelStats -I libraries model.sysml                   # element counts
sysml doc model.sysml                                            # Markdown documentation
```

## Part 12: Formatting and CI

```sh
$ sysml fmt --diff model.sysml
--- model.sysml
+++ model.sysml (formatted)
@@ -31,7 +31,7 @@
-    attribute status:SensorStatus;
+    attribute status : SensorStatus;

$ sysml fmt model.sysml                   # Format in place
$ sysml fmt --check model.sysml           # CI mode (exit 1 if unformatted)
```

CI is a plain sequence of commands — every gate exits non-zero on failure:

```sh
sysml check --severity warning *.sysml \
  && sysml fmt --check *.sysml \
  && sysml trace --check --min-coverage 80 *.sysml \
  && sysml coverage --check --min-score 60 *.sysml
```

JSON output for editor integration:

```sh
sysml check -f json model.sysml         # Diagnostics as JSON array
sysml list -f json model.sysml          # Element list as JSON
```

## Quick Reference

| Task | Interactive | Flags |
|------|------------|-------|
| Create any element | `sysml add` | `sysml add <file> <kind> <name>` |
| Add with model context | `sysml add <file>` | `sysml add <file> <kind> <name> --inside Parent` |
| Enum with members | wizard prompts | `sysml add <file> enum-def Color -m red,green,blue` |
| State machine | wizard prompts | `sysml add <file> state-def S -m "state idle,transition first idle accept go then running"` |
| Connection | wizard prompts | `sysml add <file> connection c --connect "a.x to b.y" --inside Assy` |
| Satisfy requirement | wizard prompts | `sysml add <file> satisfy Req --by Element` |
| Verify requirement | wizard prompts | `sysml add <file> verify Req --by TestCase` |
| Verification case | wizard prompts | `sysml add <file> verification-def Test --doc "..." -m "subject s"` |
| Import | wizard prompts | `sysml add <file> import "Pkg::*"` |
| Remove element | — | `sysml remove <file> Name` |
| Rename element | — | `sysml rename <file> Old New` |
| Generate to stdout | — | `sysml add --stdout <kind> <name>` |
| FMEA worksheet | — | `sysml view FmeaWorksheet -I libraries <files>` |
| Requirement coverage | — | `sysml trace --check --min-coverage 80 <files>` |
| Mass/cost rollup | — | `sysml rollup compute <files> --root Def --attr mass` |
