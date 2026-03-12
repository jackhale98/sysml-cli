# Tutorial: Building and Managing a SysML v2 Model

Build a complete systems engineering model from scratch using the `sysml` interactive wizard. You'll model a **weather station** — an embedded system with sensors, a controller, and a display — then validate, diagram, simulate, and run lifecycle management on it.

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

Explore built-in help:

```sh
sysml guide                    # list topics
sysml guide getting-started
```

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
$ sysml lint model.sysml
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

```sh
$ sysml stats model.sysml
Model Statistics
========================================
Definitions: 16
Usages:      22

Definitions by kind:
  part def             10
  port def              3
  enum def              2
  action def            1
  constraint def        2
  calc def              1
  state def             1
  connection def        0
  package               1

Relationships:
  Connections:    5
  Flows:          0
  Satisfactions:  0
  Verifications:  0

Packages:         1
Abstract defs:    1
Max nesting:      1

Documentation:    10/15 (67%)
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

A verification case defines *what* to verify and the *procedure* to follow. Sub-usages inside the verification def become procedure steps — `sysml verify run` walks through them interactively. Steps with names containing "measure" or "reading" (or typed as attributes) prompt for numeric values; other steps prompt for pass/fail confirmation.

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

```sh
$ sysml verify list verification.sysml
Verification Cases:
  TestTemperatureAccuracy    verifies: TemperatureAccuracy    steps: 3
  TestOperatingRange         verifies: OperatingRange         steps: 3
  TestBatteryLife            verifies: BatteryLife            steps: 3

$ sysml verify status verification.sysml requirements.sysml
Requirement              Status       Verified By
---------------------------------------------------
TemperatureAccuracy      unverified   TestTemperatureAccuracy
OperatingRange           unverified   TestOperatingRange
BatteryLife              unverified   TestBatteryLife
UpdateRate               no case      -
IPRating                 no case      -

Coverage: 3/5 have verification cases (60%)
```

### 4.4 Execute a verification case

`verify run` reads the procedure steps from the verification def and walks through them interactively:

```sh
$ sysml verify run verification.sysml --case TestTemperatureAccuracy --author "Jane Smith"
Verification Case: TestTemperatureAccuracy
  Verifies: TemperatureAccuracy
  Steps: 3

? Step 1 - setup: Completed? [Y/n] Y
? Step 2 - measureAccuracy: Enter measured value: 0.3
? Unit for 'measureAccuracy': C
? Is the measurement for 'measureAccuracy' within specification? [Y/n] Y
? Step 3 - evaluate: Completed? [Y/n] Y
? Any observations or notes?
? Overall verification result: > Pass

Result: PASS
Measurements:
  measureAccuracy = 0.3 C [OK]

Record written: .sysml/records/verify-execution-20260310-JaneSmith-ab12.toml
```

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

Other output formats:

```sh
sysml diagram -t bdd -o plantuml model.sysml
sysml diagram -t bdd -o dot model.sysml
sysml diagram -t bdd -o d2 model.sysml
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

```sh
$ sysml interfaces --unconnected model.sysml
Unconnected Ports:
  Name            Owner           Type            Direction
  -------------------------------------------------------
  power           Controller      PowerPort       -
  power           Display         PowerPort       -
2 port(s) found.
```

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

## Part 8: Risk Management

Risk management follows FMEA (AIAG/VDA, SAE J1739) and hazard analysis (MIL-STD-882E, ISO 14971) methodology. Risks are nested inside the part, action, or use case they apply to — this assignment is automatically tracked in reports.

### 8.1 Define risks in the model

Risks use numeric 1–5 scores for Severity (S), Occurrence (O), and Detection (D):

| Score | Severity     | Occurrence   | Detection          |
|-------|-------------|-------------|--------------------|
| 1     | Negligible  | Improbable  | Almost Certain     |
| 2     | Marginal    | Remote      | High               |
| 3     | Moderate    | Occasional  | Moderate           |
| 4     | Critical    | Probable    | Low                |
| 5     | Catastrophic| Frequent    | Almost Impossible  |

### 8.2 Create risks interactively

The wizard prompts for FMEA fields (failure mode, effect, cause) and numeric scores:

```sh
$ sysml risk add --file model.sysml --inside Enclosure
? Failure mode: Moisture ingress past IP seal
? Failure effect: Corrosion of internal electronics
? Failure cause: Seal degradation from UV exposure
? Severity (1-5): 4
? Occurrence (1-5): 2
? Detection (1-5): 3
? Recommended action: Add redundant seal + UV-resistant gasket

Preview:
  part riskMoistureIngress : RiskDef {
      doc /* Moisture ingress past IP seal */
      attribute severity = 4;
      attribute occurrence = 2;
      attribute detection = 3;
      attribute failureEffect = "Corrosion of internal electronics";
      attribute failureCause = "Seal degradation from UV exposure";
      attribute recommendedAction = "Add redundant seal + UV-resistant gasket";
  }
  RPN: 24 (4 × 2 × 3)  Risk: Undesirable

? Select target file > model.sysml
? Insert inside which definition? > Enclosure
Wrote riskMoistureIngress to model.sysml
```

Add a second risk to a different component:

```sh
$ sysml risk add --file model.sysml --inside SolarPanel
? Failure mode: Solar cell delamination
? Failure effect: Power output degradation
? Failure cause: Thermal cycling stress
? Severity (1-5): 3
? Occurrence (1-5): 3
? Detection (1-5): 4
? Recommended action:
```

### 8.3 Analyze risks

Risk reports show entity assignments and acceptance levels:

```sh
$ sysml risk list model.sysml
Risks (2):
  Moisture ingress past IP seal [S:4 O:2 RPN:24 UNDESIRABLE] → Enclosure
  Solar cell delamination [S:3 O:3 RPN:36 UNDESIRABLE] → SolarPanel

$ sysml risk matrix model.sysml
                  | Improb. | Remote  | Occasnl | Probabl | Frequnt |
------------------+---------+---------+---------+---------+---------+
Catastroph        |    -    |    -    |  !! -   |  !! -   |  !! -   |
Critical          |    -    | ! risk..|  !! -   |  !! -   |  !! -   |
Moderate          |    -    |    -    | ! risk..|    -    |    -    |
Marginal          |    -    |    -    |    -    |    -    |    -    |
Negligible        |    -    |    -    |    -    |    -    |    -    |

Zones: !! = Unacceptable  ! = Undesirable  ? = Review    (blank) = Acceptable
```

FMEA worksheet with full standard columns:

```sh
$ sysml risk fmea model.sysml
Item                 Failure Mode         Effect          Cause            S   O   D   RPN Risk Level     Rec. Action          Status     Assigned To
---------------------------------------------------------------------
Enclosure            Moisture ingress...  Corrosion...    Seal degrada...  4   2   3    24 UNDESIRABLE    Add redundant...     Identified Enclosure
SolarPanel           Solar cell delam...  Power output..  Thermal cycl...  3   3   4    36 UNDESIRABLE    -                    Identified SolarPanel
```

### 8.4 Risk coverage

Check which parts and actions have risks identified — useful as a CI gate:

```sh
$ sysml risk coverage model.sysml
Risk Coverage
  Elements (parts/actions/use cases): 5
  With risks:    2 (40.0%)
  Without risks: 3

Uncovered elements:
  WeatherStation (part)
  MainBoard (part)
  DataProcessor (action)
```

## Part 9: Manufacturing and Quality

### 9.1 SPC analysis

```sh
$ sysml mfg spc --parameter SensorCalibration \
    --values 0.48,0.52,0.50,0.49,0.51,0.50,0.53,0.47
  Mean: 0.500  Std: 0.019  UCL: 0.557  LCL: 0.443
  All points within control limits
```

### 9.2 Define a manufacturing routing

A manufacturing routing is an `action def` whose child `action` usages become process steps. `sysml mfg` extracts these steps and infers the process type from each step name (e.g., "machineHousing" → Machining, "inspectDimensions" → Test & Inspection, "assembleUnit" → Assembly).

```sh
sysml add model.sysml action-def AssemblyProcess \
    --doc "Enclosure production routing" \
    -m "action cutSheet" \
    -m "action machineHousing" \
    -m "action heatTreatFrame" \
    -m "action coatSurface" \
    -m "action assembleUnit" \
    -m "action inspectDimensions" \
    -m "action packageShip"
```

This creates a routing with 7 steps. `mfg list` shows the extracted routing:

```sh
$ sysml mfg list model.sysml
Manufacturing Routings (1):
  AssemblyProcess (7 steps)
```

> **Step naming convention:** The process type is inferred from keywords in the step name — `cut`/`shear` → Sheet Metal, `machine`/`mill`/`drill` → Machining, `weld` → Welding, `heat`/`anneal` → Heat Treat, `coat`/`paint` → Coating, `assembl`/`install` → Assembly, `test`/`inspect` → Test & Inspection, `package`/`ship` → Packaging. Steps typed as Test & Inspection or Calibration automatically require inspection sign-off.

### 9.3 Run a production lot

```sh
$ sysml mfg start-lot model.sysml --routing AssemblyProcess --quantity 100
Lot: mfg-lot-20260310-engineer-5a7b
  Routing:  AssemblyProcess
  Quantity: 100
  Progress: 0/7 steps
Record written: .sysml/records/mfg-lot-20260310-engineer-5a7b.toml

$ sysml mfg step
Lot: mfg-lot-20260310-engineer-5a7b — Step 1/7: cutSheet [SheetMetal]
? Step 1: cutSheet [SheetMetal] — Ready to begin? [Y/n] Y
Step status: Passed
```

### 9.4 Quality management

```sh
# Sampling plans
$ sysml qc sample-size --lot-size 500
  AQL: 1.0  Level: Normal  Sample: 50  Accept: 5  Reject: 6

# Process capability
$ sysml qc capability --usl 10.05 --lsl 9.95 \
    --values 10.01,9.99,10.02,9.98,10.00,10.01,9.99,10.00
  Cp: 1.67  Cpk: 1.33  Process is capable

# NCR → RCA → CAPA workflow
$ sysml quality create --type ncr
? Affected part: Enclosure
? Category: > Dimensional
? Severity: > Major
? Description: IP seal gap exceeds tolerance
NCR Created: quality-ncr-20260310-engineer-7f3a
Record: .sysml/records/quality-ncr-20260310-engineer-7f3a.toml

$ sysml quality rca --source quality-ncr-20260310-engineer-7f3a --method five-why
? Why 1: IP seal gap exceeds tolerance → Tool wear on seal groove
? Why 2: Tool wear on seal groove → No scheduled tool replacement
? Why 3: No scheduled tool replacement → Missing PM schedule
? Why 4: Missing PM schedule → New tooling, no PM baseline
? Why 5: No PM baseline → Commissioning checklist incomplete
Root Cause: Commissioning checklist incomplete
Record: .sysml/records/quality-rca-20260310-engineer-2e4f.toml

$ sysml quality create --type capa
? Title: Add PM schedule for seal groove tooling
? Source: > Ncr
? CAPA type: > Corrective
CAPA Created: quality-capa-20260310-engineer-4b2c

$ sysml quality action --capa quality-capa-20260310-engineer-4b2c
? Action type: > Procedure Update
? Description: Create PM schedule with tool replacement intervals
? Owner: alice
? Due date: 2026-03-20
Action added to CAPA

$ sysml quality trend --group-by category
Category      Open  In Progress  Closed  Total
Dimensional      1            0       0      1
```

## Part 10: Bill of Materials

### 10.1 Add BOM attributes

Add identity and mass properties to your parts using the BOM library types:

```sh
sysml add model.sysml import "SysMLBOM::*"

# Add identity and mass to existing parts
sysml add model.sysml attribute partNumber -t ScalarValues::String --inside TemperatureSensor
sysml add model.sysml attribute mass_kg -t ScalarValues::Real --inside TemperatureSensor
```

### 10.2 Record supplier quotes

Cost data lives in quote records (TOML files in `.sysml/records/`), not in the SysML model. This separates volatile commercial data from the engineering model.

```sh
$ sysml source quote
? Part name: TemperatureSensor
? Supplier name: SensorCorp
? Currency code: USD
? Lead time (days): 14
? Minimum order quantity: 10
? Quote valid until (YYYY-MM-DD): 2026-12-31

Add price breaks (at least one required):
? Minimum quantity for this tier: 1
? Unit price at this tier: 45.00
  Added: qty >= 1 → 45.0000 USD
? Add another price break? Yes
? Minimum quantity for this tier: 100
? Unit price at this tier: 38.00
  Added: qty >= 100 → 38.0000 USD
? Add another price break? No

Quote: SensorCorp → TemperatureSensor (USD)
  qty >=      1 → 45.0000 USD
  qty >=    100 → 38.0000 USD
  Lead time: 14 days, MOQ: 10

Wrote quote record: .sysml/records/source-quote-20260311T...toml
```

### 10.3 Costed BOM

Once you have quotes recorded, run a costed BOM analysis at any production quantity:

```sh
$ sysml bom cost model.sysml --root WeatherStationUnit --quantity 100

Costed BOM — WeatherStationUnit × 100 (order qty 100)

Level  Name                 Definition           BOM Qty  Total Qty   Unit Price       Extended Supplier
-------------------------------------------------------------------------------------------------------------
0      WeatherStationUnit   WeatherStationUnit         1        100            -              - -
1        tempSensor         TemperatureSensor           1        100      38.0000        3800.00 SensorCorp
1        controller         Controller                  1        100     120.0000       12000.00 ChipCo
...
-------------------------------------------------------------------------------------------------------------
                                                                          TOTAL:       56800.00
```

Use `--apply` to write the resolved unit prices back into the model as `attribute unitCost = <price>;`:

```sh
$ sysml bom cost model.sysml --root WeatherStationUnit --quantity 100 --apply
# ... costed BOM report ...
Applied unitCost to 8 definitions.
```

This snapshots pricing into the model for the chosen production volume, making cost data available to `bom rollup --include-cost` and exports.

### 10.4 BOM rollup

```sh
$ sysml bom rollup model.sysml --root WeatherStationUnit --include-mass
WeatherStationUnit : WeatherStationUnit
  tempSensor : TemperatureSensor [TS-100]  mass=0.150kg
  humiditySensor : HumiditySensor [HS-200]  mass=0.120kg
  pressureSensor : PressureSensor [PS-300]  mass=0.180kg
  windSensor : WindSensor [WS-400]  mass=0.250kg
  controller : Controller [CT-500]  mass=0.350kg
  display : Display [DS-600]  mass=0.200kg
  power : PowerSupply [PW-700]  mass=1.200kg
  enclosure : Enclosure [EN-800]  mass=2.500kg
BOM: 9 total parts, 9 unique, depth 2
Total mass: 4.950 kg
```

### 10.5 Where-used and export

```sh
$ sysml bom where-used model.sysml --part TemperatureSensor
Part `TemperatureSensor` is used in:
  WeatherStationUnit

$ sysml bom export model.sysml --root WeatherStationUnit
Level,Name,Definition,Quantity,PartNumber,Revision,Description,Category
0,WeatherStationUnit,WeatherStationUnit,1,,,,assembly
1,tempSensor,TemperatureSensor,1,TS-100,A,,component
1,humiditySensor,HumiditySensor,1,HS-200,A,,component
1,controller,Controller,1,CT-500,A,,component
1,display,Display,1,DS-600,A,,component
1,power,PowerSupply,1,PW-700,A,,component
1,enclosure,Enclosure,1,EN-800,A,,assembly
```

SysML v2 multiplicity is the BOM quantity — `part wheels : Wheel[4]` = 4 units in the rollup.

## Part 11: Editing

### 11.1 Add new elements

```sh
sysml add model.sysml part-def RainGauge --extends Sensor --doc "Measures rainfall"
sysml add model.sysml part rainGauge -t RainGauge --inside WeatherStationUnit
```

### 11.2 Preview and multiplicity

```sh
sysml add model.sysml part-def Vehicle \
    -m "part wheels:Wheel[4],attribute doors:Door[2..5]" --dry-run
```

### 11.3 Remove and rename

```sh
sysml remove model.sysml RainGauge --dry-run    # Preview
sysml remove model.sysml RainGauge              # Apply
sysml rename model.sysml WindSensor Anemometer
```

### 11.4 Learn SysML syntax

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

## Part 12: Export and Reports

### 12.1 Export

```sh
sysml export interfaces model.sysml --part Controller   # FMI 3.0
sysml export modelica model.sysml --part Controller      # Modelica
sysml export ssp model.sysml -o system.ssd               # SSP XML
```

### 12.2 Reports

```sh
$ sysml report dashboard model.sysml requirements.sysml verification.sysml
Project Dashboard
========================================
Requirements:     5  (100% satisfied, 60% verified)
Risks:            2  (1 critical, avg RPN: 48)
Open NCRs:        1  (1 major)
BOM Parts:        10
Documentation:    67%

$ sysml report gate requirements.sysml verification.sysml --gate-name PDR
Gate Readiness: PDR (Preliminary Design Review)
========================================
  Verification coverage:  60%  (threshold: 50%)  PASS
  Open critical risks:    1    (max: 3)           PASS
  Open major NCRs:        1    (max: 5)           PASS

Result: READY
```

## Part 13: Formatting and CI

```sh
$ sysml fmt --diff model.sysml
--- model.sysml
+++ model.sysml (formatted)
@@ -31,7 +31,7 @@
-    attribute status:SensorStatus;
+    attribute status : SensorStatus;

$ sysml fmt model.sysml                   # Format in place
$ sysml fmt --check model.sysml           # CI mode (exit 1 if unformatted)

$ sysml pipeline run ci
[1/4] lint *.sysml ... ok
[2/4] fmt --check *.sysml ... ok
[3/4] trace --check --min-coverage 80 *.sysml ... ok
[4/4] coverage --check --min-score 60 *.sysml ... ok

Pipeline "ci": all 4 steps passed.
```

JSON output for editor integration:

```sh
sysml lint -f json model.sysml          # Diagnostics as JSON array
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
| Risk creation | `sysml risk add` | `sysml risk add --file risks.sysml` |
| Verify execution | `sysml verify run <files>` | `sysml verify run <files> --case Name` |
| NCR creation | `sysml quality create --type ncr` | — |
| Lot tracking | `sysml mfg start-lot` | `sysml mfg start-lot <files> --routing Name` |
