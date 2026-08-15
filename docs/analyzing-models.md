# Analyzing a model with the sysml CLI

You write SysML v2 in your text editor; the CLI is the analysis engine
for the models you write. This guide runs a real model — the pressure
relief valve from
[sysml-domain-libraries](https://github.com/jackhale98/sysml-domain-libraries)
(`examples/ReliefValve.sysml`), where a tolerance stackup is the
evidence behind a safety risk control — through the analysis commands,
showing what each one is for and what it prints. Every output below is
real.

Throughout, `-I libraries` puts the domain libraries on the import path;
inside a project with a `.sysml/config.toml` (created by `sysml init`),
the include path is picked up automatically and the file arguments can
be omitted.

## 1. Is the model well-formed? — `check`

Always first. Cross-file name resolution, requirement coverage,
multiplicity, and the constraints the model itself declares (W017
evaluates the libraries' `assert constraint`s against your values —
an `@Fmea` line with `severity = 12` fails `FmeaRating`'s 1–10 range):

```
$ sysml check -I libraries examples/ReliefValve.sysml
No issues found.
```

In CI, `check` exits non-zero on errors or warnings. Notes (unused
defs, missing docs) don't fail the build.

## 2. What's in it? — `list`, `show`, `deps`

Orientation commands for a model you didn't write:

```
$ sysml list -k requirements -I libraries examples/ReliefValve.sysml
  requirement def OverpressureProtectionReq (in ReliefValveExample)
  requirement def MinTravelReq : RiskControl (in ReliefValveExample)
  requirement    overpressureProtection : OverpressureProtectionReq (in ReliefValveExample)
  requirement    minTravel : MinTravelReq (in overpressureProtection)

$ sysml show examples/ReliefValve.sysml travelGap     # full detail on one element
$ sysml deps examples/ReliefValve.sysml Piston        # who references it, what it needs
```

## 3. The model's own reports — `view`

Views are defined *in the model* (a `view def` with a `@TableRendering`
annotation — the libraries ship a set, projects add their own). The CLI
just renders them; with no name it lists what's available:

```
$ sysml view -I libraries examples/ReliefValve.sysml
PortTable                      libraries/Reporting.sysml
AllocationMatrix               libraries/Reporting.sysml
RequirementsTraceMatrix        libraries/Reporting.sysml
ModelStats                     libraries/Reporting.sysml
ConnectionTable                libraries/Reporting.sysml
FmeaWorksheet                  libraries/RiskAnalysis.sysml
RiskMatrix                     libraries/RiskAnalysis.sysml
HazardLog                      libraries/HazardAnalysis.sysml
StackupSummary                 libraries/Tolerancing.sysml
FitTable                       libraries/Tolerancing.sysml

$ sysml view FmeaWorksheet -I libraries examples/ReliefValve.sysml
element  failureMode             cause                                                category              severity  likelihood  detection  rpn
------------------------------------------------------------------------------------------------------------------------------------------------
piston   Piston seizure in bore  Insufficient travel clearance at tolerance extremes  RiskCategory::design  8         3           6          144
```

The `rpn` column is computed from the row (`severity*likelihood*detection`
in the view's column spec) — derived values are never stored in the
model. `StackupSummary` runs every uncertainty analysis it lists:

```
$ sysml view StackupSummary -I libraries examples/ReliefValve.sysml
case       critical  nominal  lower   upper   wcMin   wcMax   margin  cp  cpk  result
---------------------------------------------------------------------------------------
travelGap  true      0.5000   0.2000  0.8000  0.2500  0.7500  0.0500  2   2    MARGINAL
```

`-f csv` or `-f md` exports any view for a spreadsheet or a report;
`-f json` for scripting.

## 4. Deep-dive one analysis — `analyze run`

`view StackupSummary` surveys every stackup; `analyze run` interrogates
one. Cases typed by `Uncertainty::UncertaintyAnalysis` (tolerance
stackups, uncertain budgets) get worst-case interval arithmetic, RSS
variance propagation, and seeded Monte Carlo:

```
$ sysml analyze run -I libraries examples/ReliefValve.sysml -n travelGap --method all \
      --iterations 20000 --seed 42
Uncertainty analysis: travelGap (ToleranceStackup)  [critical]
  Target: 0.5000 in [0.2000, 0.8000]
  Contributions:
    + seatDepth                 30.0000 +0.1000/-0.1000  normal    DWG-RV-001
    - pistonLength              28.0000 +0.0500/-0.0500  normal    DWG-RV-002
    - springSolid                1.5000 +0.1000/-0.1000  uniform   VS-SPR-9

  Worst-case:
    Range: 0.2500 .. 0.7500   margin: 0.0500   result: MARGINAL

  RSS:
    Mean: 0.5000   3σ: 0.1500
    Cp: 2.00   Cpk: 2.00   Yield: 100.00%
    Sensitivity: seatDepth 44.4% | pistonLength 11.1% | springSolid 44.4%
    Result: PASS

  Monte Carlo (20000 iterations, seed 42):
    Mean: 0.5003   StdDev: 0.0689
    Range: 0.2804 .. 0.7079   95% CI: [0.3716, 0.6276]
    Pp: 1.45   Ppk: 1.45   Yield: 100.00%
    Distribution:
         0.2804 |                                          10
         0.3008 |                                          20
         0.3211 | #                                        67
         0.3415 | ####                                     220
         0.3618 | ##########                               496
         0.3822 | ################                         806
         0.4026 | #########################                1233
         0.4229 | ###################################      1738
         0.4433 | #####################################    1834
         0.4636 | ######################################   1931
         0.4840 | #######################################  1981
         0.5043 | ######################################## 2009
         0.5247 | ######################################## 1987
         0.5450 | ####################################     1807
         0.5654 | ###############################          1550
         0.5857 | #######################                  1134
         0.6061 | #############                            646
         0.6265 | #######                                  358
         0.6468 | ###                                      127
         0.6672 | #                                        37
         0.6875 |                                          9
    Result: PASS
```

How to read this: worst-case is MARGINAL (0.05 mm of margin, under 10%
of the band) while RSS and Monte Carlo PASS comfortably — the classic
signal that the chain is statistically fine but a 100%-interchangeable
guarantee is tight. Sensitivity says seatDepth and springSolid dominate;
tightening pistonLength buys nothing. The histogram shows the shape:
the uniform spring contribution visibly widens the flanks. Bins holding
a spec limit are marked `< LSL` / `< USL`. The seed is recorded, so the
run — histogram included — reproduces bit-for-bit.

Contributions are feature-chain references into the parts
(`:>> dim = body.seatDepth;`), so retolerancing a drawing dimension
re-runs everywhere it participates.

## 5. Does the safety case close? — `trace` and `coverage`

```
$ sysml trace -I libraries examples/ReliefValve.sysml
Requirement             Satisfied By          Verified By
------------------------------------------------------------------
overpressureProtection  relief                PopTest
minTravel               relief                TravelClearanceTest
RiskControl             (via specialization)  (via specialization)

Coverage: 3/3 satisfied (100%), 3/3 verified (100%)
```

Trace understands specialization: `RiskControl` counts as satisfied
because the requirement specializing it is. `coverage` scores overall
model completeness — with the weighting itself defined in the model
(a `QualityScore` calc):

```
$ sysml coverage -I libraries examples/ReliefValve.sysml
Summary:
  Documentation:       90%
  Typed usages:        64%
  Populated defs:      96%
  Req satisfaction:    100%
  Req verification:    100%
  Overall score:       89%  (model:QualityScore)
```

The CI gates are model content too: `--check` evaluates constraint
usages typed `QualityGate` / `TraceGate` (from `ModelQuality.sysml`),
so the shipping threshold is a reviewed model change, not a CI knob:

```sysml
constraint qualityGate : QualityGate { :>> minScore = 80.0; }
constraint traceGate : TraceGate;   // defaults: everything satisfied AND verified
```

```
$ sysml coverage --check -I libraries examples/ReliefValve.sysml; echo $status
89 >= 80: gate passes
0
```

With no gate declared, `--check` is strict (perfect score; every
requirement satisfied and verified).

## 6. Budgets across the hierarchy — `rollup`

Any numeric attribute rolls up the part tree — mass, cost, power —
with units converted and variant configurations applied:

```sh
sysml rollup compute model.sysml --root Vehicle --attr mass
sysml rollup budget model.sysml --root Vehicle --attr mass --limit 1000
sysml rollup compute model.sysml --root Drone --attr mass --variant battery=powerBattery
```

## 7. Diagrams, diffs, and everything else

```sh
sysml diagram -t iv --scope ReliefValveAsm examples/ReliefValve.sysml   # interconnection view
sysml diff old.sysml new.sysml                                          # semantic model diff
sysml simulate state-machine model.sysml -n DroneStates -e TurnOn       # state machines
sysml repl -I libraries examples/ReliefValve.sysml                      # all of the above, interactively
```

The REPL dispatches `check`, `view`, and `analyze` into the same
implementations as the batch commands, so interactive results never
disagree with CI.

## Command reference

Full per-command documentation: [Analysis](commands/analysis.md) ·
[Views](commands/views.md) · [Diagrams](commands/diagrams.md) ·
[Editing](commands/editing.md) · [Simulation](commands/simulation.md) ·
[Project](commands/project.md).
