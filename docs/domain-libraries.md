# Domain Libraries

The tool ships with SysML v2 domain libraries in the `libraries/` directory. These provide abstract base types that users specialize in their models. The tool operates on the base types, so all specializations are recognized automatically.

## How Libraries Work

```sysml
// Domain library (shipped with tool)
package SysMLRisk {
    part def RiskDef {
        attribute severity : Integer;
        attribute occurrence : Integer;
        attribute detection : Integer;
    }
}

// Your model — nest risks inside the parts they apply to
package MyProject {
    import SysMLRisk::*;

    part def Enclosure {
        part riskMoistureIngress : RiskDef {
            doc /* Moisture ingress past IP seal */
            attribute severity = 4;
            attribute occurrence = 2;
            attribute detection = 3;
        }
    }
}
```

### Setup

Run `sysml init` in a project with a `libraries/` directory — it automatically configures `library_paths`:

```sh
cp -r /path/to/sysml-cli/libraries .
sysml init
# library_path = "libraries/"
```

All commands then resolve imports automatically. No `-I` flag needed:

```sh
sysml lint model.sysml
sysml risk matrix model.sysml
```

You can also set library paths manually in `.sysml/config.toml`:

```toml
[project]
library_paths = ["libraries/", "vendor/models/"]
```

Or use `-I` for ad-hoc includes: `sysml lint model.sysml -I /other/libs/`

## Library Reference

### SysMLVerification (`sysml-verification-ext.sysml`)

Types for verification case management and execution tracking.

| Type | Kind | Description |
|------|------|-------------|
| `VerificationStatus` | enum def | `planned`, `inProgress`, `passed`, `failed`, `blocked`, `skipped` |
| `VerificationMethod` | enum def | `test`, `analysis`, `inspection`, `demonstration`, `simulation`, `review` |
| `AcceptanceCriteriaDef` | requirement def | Base type for acceptance criteria |
| `EquipmentDef` | part def | Test equipment with calibration tracking |
| `VerificationProcedureDef` | action def | Ordered test procedure steps |

### SysMLRisk (`sysml-risk.sysml`)

FMEA (AIAG/VDA, SAE J1739) and hazard analysis (MIL-STD-882E, ISO 14971) types.

| Type | Kind | Description |
|------|------|-------------|
| `RiskDef` | part def | FMEA risk item: failureMode, failureEffect, failureCause, severity(1-5), occurrence(1-5), detection(1-5) |
| `RiskCategory` | enum def | `technical`, `schedule`, `cost`, `safety`, `regulatory`, `supplyChain`, `environmental` |
| `RiskStatus` | enum def | `identified`, `analyzing`, `mitigating`, `monitoring`, `closed`, `accepted` |
| `MitigationDef` | action def | Planned mitigation with strategy, owner, due date |
| `MitigationStrategy` | enum def | `avoid`, `transfer`, `reduce`, `accept`, `contingency` |

**Scoring** — all three dimensions use 1–5 integer scales:

| Score | Severity (S) | Occurrence (O) | Detection (D) |
|-------|-------------|----------------|---------------|
| 1 | Negligible | Improbable | Almost Certain |
| 2 | Marginal | Remote | High |
| 3 | Moderate | Occasional | Moderate |
| 4 | Critical | Probable | Low |
| 5 | Catastrophic | Frequent | Almost Impossible |

**RPN** = S × O × D (range 1–125). Risk acceptance zones: Unacceptable / Undesirable / Review / Acceptable.

### SysMLTolerance (`sysml-tolerance.sysml`)

Types for dimensional tolerance analysis and GD&T.

| Type | Kind | Description |
|------|------|-------------|
| `DistributionType` | enum def | `normal`, `uniform`, `triangular`, `skewedLeft`, `skewedRight`, `beta` |
| `ToleranceDef` | attribute def | Nominal, upper/lower limits, distribution, Cp/Cpk targets |
| `BilateralToleranceDef` | attribute def | Symmetric +/- tolerance |
| `DimensionChainDef` | part def | Ordered collection of tolerance contributors with closing dimension |
| `StackDirection` | enum def | `linear`, `radial`, `angular` |
| `GeometricCharacteristic` | enum def | All 14 per ASME Y14.5 (straightness through totalRunout) |
| `FeatureControlFrameDef` | part def | GD&T callout with datum references |
| `MaterialCondition` | enum def | `RFS`, `MMC`, `LMC` |

### SysMLBOM (`sysml-bom.sysml`)

Types for bill of materials, mass properties, and supplier management. Cost data is managed through quote records (see `source quote` and `bom cost` commands), not embedded in the SysML model.

| Type | Kind | Description |
|------|------|-------------|
| `PartCategory` | enum def | `assembly`, `subassembly`, `component`, `rawMaterial`, `fastener`, `consumable`, `software`, `document` |
| `LifecycleState` | enum def | `concept`, `development`, `prototype`, `production`, `obsolete` |
| `MakeOrBuy` | enum def | `make`, `buy`, `makeAndBuy`, `tbd` |
| `PartIdentity` | attribute def | Part number, revision, category, lifecycle state |
| `MassProperty` | attribute def | Mass value, type (actual/estimated/calculated/allocated), margin |
| `CostProperty` | attribute def | Unit cost, tooling cost, basis (written by `bom cost --apply`) |
| `SupplierDef` | part def | Company name, code, qualification status, certifications |
| `QualificationStatus` | enum def | `pending`, `conditional`, `approved`, `preferred`, `probation`, `disqualified` |
| `SourceDef` | part def | Links part to supplier with lead time, MOQ, source type |

### SysMLManufacturing (`sysml-manufacturing.sysml`)

Types for manufacturing processes, routings, and work instructions.

| Type | Kind | Description |
|------|------|-------------|
| `ProcessType` | enum def | 20 types: `machining`, `welding`, `molding`, `assembly`, `heatTreat`, etc. |
| `ProcessDef` | action def | Process step with type, work center, setup/cycle time, tooling |
| `ProcessParameterDef` | attribute def | Parameter with nominal, UCL/LCL, USL/LSL, monitoring method |
| `WorkInstructionDef` | action def | Operator instruction with safety warnings and quality checkpoints |
| `InspectionPointDef` | part def | In-process inspection: type, sampling rate, mandatory/advisory gate |
| `RoutingDef` | action def | Ordered sequence of process steps |

### SysMLQuality (`sysml-quality.sysml`)

Types for quality control inspection and measurement.

| Type | Kind | Description |
|------|------|-------------|
| `InspectionType` | enum def | `dimensional`, `visual`, `functional`, `destructive`, `nonDestructive` |
| `CharacteristicClassification` | enum def | `critical`, `major`, `minor`, `informational` |
| `SamplingStandard` | enum def | `ansiZ14`, `iso2859`, `cEqualsZero`, `hundredPercent`, `custom` |
| `InspectionLevel` | enum def | `reduced`, `normal`, `tightened` |
| `InspectionPlanDef` | part def | Plan type, sampling standard, AQL level, characteristics |
| `QualityCharacteristicDef` | part def | Classification, measurement type, tolerance reference |
| `GaugeRRDef` | part def | Study specification: operators, parts, trials, acceptable %PTV |

### SysMLCAPA (`sysml-capa.sysml`)

Quality management types: NCR, CAPA, and Process Deviation — three distinct quality item types, each with its own lifecycle.

| Type | Kind | Description |
|------|------|-------------|
| `NonconformanceCategory` | enum def | `dimensional`, `material`, `cosmetic`, `functional`, `workmanship`, etc. |
| `SeverityClass` | enum def | `critical`, `major`, `minor`, `observation` |
| `Disposition` | enum def | `useAsIs`, `rework`, `repair`, `scrap`, `returnToVendor`, `sortAndScreen`, `deviate` |
| `CorrectiveActionType` | enum def | `designChange`, `processChange`, `supplierChange`, `trainingRetraining`, etc. |
| `NcrStatus` | enum def | NCR lifecycle: `open`, `investigating`, `dispositioned`, `verified`, `closed`, `reopened` |
| `CapaStatus` | enum def | CAPA lifecycle: `initiated` through `closed` (8 states) |
| `CapaSource` | enum def | `ncr`, `auditFinding`, `customerComplaint`, `processImprovement`, etc. |
| `CapaType` | enum def | `corrective`, `preventive` |
| `DeviationStatus` | enum def | Deviation lifecycle: `requested` through `closed` (7 states) |
| `DeviationScope` | enum def | `lot`, `processStep`, `productLine`, `temporary`, `permanent` |

### SysMLProject (`sysml-project.sysml`)

Types for project milestone gates and design reviews.

| Type | Kind | Description |
|------|------|-------------|
| `PhaseDef` | enum def | `concept`, `preliminaryDesign`, `detailedDesign`, `prototyping`, `designVerification`, `processValidation`, `production`, `sustaining` |
| `ReviewType` | enum def | `SRR`, `PDR`, `CDR`, `TRR`, `FAR`, `PRR` |
| `MilestoneDef` | part def | Phase, review type, required coverage threshold, max open risks/NCRs |

## Library Dependency Graph

```
SysMLRisk           (standalone)
SysMLTolerance      (standalone)
SysMLBOM            (standalone)
SysMLManufacturing  (standalone)
SysMLQuality        (standalone)
SysMLCAPA           (standalone)
SysMLVerification   (standalone)
SysMLProject        (standalone)
```

Each library is a single `.sysml` file with no cross-library dependencies. Users import only what they need.
