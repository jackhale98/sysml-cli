# SV2 Tools: Architecture and Guidance

**A SysML v2-Native Product Lifecycle Toolchain**

---

## Preface

This document defines the architecture for an open-source toolchain that treats SysML v2 textual notation as source code for systems engineering. this is a unified CLI tool which provides the lifecycle management, analysis, execution, and reporting capabilities that the modeling language alone cannot deliver.

The fundamental premise is that SysML v2's textual notation is a first-class authoring format, not an interchange serialization. The OMG designed it so engineers could model systems the way developers write code: in plain text files, under version control, with diffs and pull requests and code review. This toolchain makes that workflow complete.

Everything that can be expressed as a definition, relationship, or constraint belongs in SysML v2 files. Everything that involves execution, temporal state, computation, or workflow belongs in the CLI tool. The model is the source of truth. The tool is its companion.

---

## Table of Contents

1. [Core Principles](#1-core-principles)
2. [SysML v2 Domain Libraries](#2-sysml-v2-domain-libraries)
   - 2.1 [Library Architecture and Conventions](#21-library-architecture-and-conventions)
   - 2.2 [Risk Management Library](#22-risk-management-library)
   - 2.3 [Tolerance and GD&T Library](#23-tolerance-and-gdt-library)
   - 2.4 [BOM and Sourcing Library](#24-bom-and-sourcing-library)
   - 2.5 [Manufacturing Process Library](#25-manufacturing-process-library)
   - 2.6 [Quality and Inspection Library](#26-quality-and-inspection-library)
   - 2.7 [CAPA Library](#27-capa-library)
   - 2.8 [Verification Extensions Library](#28-verification-extensions-library)
   - 2.9 [Project Management Library](#29-project-management-library)
3. [CLI Tool Architecture](#3-cli-tool-architecture)
   - 3.1 [Binary Structure and Subcommands](#31-binary-structure-and-subcommands)
   - 3.2 [Configuration and Project Discovery](#32-configuration-and-project-discovery)
   - 3.3 [Model Indexing and Cache](#33-model-indexing-and-cache)
   - 3.4 [Record Storage and File Conventions](#34-record-storage-and-file-conventions)
   - 3.5 [Output Flexibility](#35-output-flexibility)
4. [Subcommand Specification](#4-subcommand-specification)
   - 4.1 [sv2 init and sv2 index](#41-sv2-init-and-sv2-index)
   - 4.2 [sv2 check](#42-sv2-check)
   - 4.3 [sv2 verify](#43-sv2-verify)
   - 4.4 [sv2 risk](#44-sv2-risk)
   - 4.5 [sv2 tol](#45-sv2-tol)
   - 4.6 [sv2 bom](#46-sv2-bom)
   - 4.7 [sv2 source](#47-sv2-source)
   - 4.8 [sv2 mfg](#48-sv2-mfg)
   - 4.9 [sv2 qc](#49-sv2-qc)
   - 4.10 [sv2 capa](#410-sv2-capa)
   - 4.11 [sv2 report](#411-sv2-report)
   - 4.12 [sv2 pipeline](#412-sv2-pipeline)
5. [Guided User Experience](#5-guided-user-experience)
   - 5.1 [Progressive Disclosure](#51-progressive-disclosure)
   - 5.2 [Scaffolding and Templates](#52-scaffolding-and-templates)
   - 5.3 [Interactive Wizards](#53-interactive-wizards)
   - 5.4 [Contextual Help and Validation](#54-contextual-help-and-validation)
   - 5.5 [Example-Driven Onboarding](#55-example-driven-onboarding)
6. [Integration Patterns](#6-integration-patterns)
   - 6.1 [SysML v2 Model as Source of Truth](#61-sysml-v2-model-as-source-of-truth)
   - 6.2 [Qualified Name Linking](#62-qualified-name-linking)
   - 6.3 [Cross-Domain Traceability](#63-cross-domain-traceability)
   - 6.4 [Pipeline Workflows](#64-pipeline-workflows)
7. [Data Architecture](#7-data-architecture)
   - 7.1 [SQLite Cache Schema](#71-sqlite-cache-schema)
   - 7.2 [TOML Record Conventions](#72-toml-record-conventions)
   - 7.3 [Git Collaboration Design](#73-git-collaboration-design)
   - 7.4 [Reference Integrity](#74-reference-integrity)
8. [Rust Workspace Structure](#8-rust-workspace-structure)
9. [Testing and Quality Strategy](#9-testing-and-quality-strategy)
10. [Development Roadmap](#10-development-roadmap)
11. [Appendix: Complete SysML v2 Library Source Sketches](#11-appendix-complete-sysml-v2-library-source-sketches)

---

## 1. Core Principles

**SysML v2 is code.** Users author models in text editors. The toolchain respects this by never requiring a graphical interface, never abstracting the notation away, and never treating the textual format as a second-class citizen behind some internal representation.

**The model defines; the tool executes.** Definitions, types, relationships, constraints, and traceability live in `.sysml` files. Execution records, temporal state, computed results, evidence, and workflow status live in TOML files managed by the CLI. There is exactly one source of truth for each category of information.

**No mandatory project structure.** The tool works with a single `.sysml` file and a single command. It also scales to hundreds of model files across packages with thousands of operational records. Users opt into structure as they need it. `sv2 init` is a convenience, not a prerequisite.

**Lazy infrastructure.** The SQLite cache is built automatically the first time a command needs it. Configuration files are optional and provide defaults that commands would otherwise require as flags. Nothing blocks the user from doing useful work immediately.

**Guided, not gatekept.** SysML v2 and MBSE are complex. The tool helps users learn by doing: scaffolding commands generate valid SysML v2 with comments explaining the constructs, interactive wizards walk through complex workflows step by step, and validation messages explain not just what is wrong but why it matters and how to fix it. The tool meets the user where they are.

**Single binary, many capabilities.** One `sv2` binary with subcommands. No separate installations, no dependency coordination, no version matrix. Install one thing, everything works.

**Git-native collaboration.** Every artifact the tool produces is a plain text file that diffs cleanly, merges predictably, and can be reviewed in a pull request. The tool actively avoids producing files that cause unnecessary merge conflicts.

---

## 2. SysML v2 Domain Libraries

### 2.1 Library Architecture and Conventions

All domain libraries follow consistent conventions to feel like natural extensions of SysML v2 rather than bolted-on additions.

**Package structure:** Each domain library is a single `.sysml` file defining a top-level package. Libraries import from the SysML v2 standard library and from each other where relationships exist. The dependency graph is:

```
SysML Standard Library (ISQ, SI, etc.)
    ├── sv2-risk
    ├── sv2-tolerance (imports ISQ for units)
    ├── sv2-bom (imports sv2-tolerance for part attributes)
    ├── sv2-sourcing (imports sv2-bom for SupplierDef)
    ├── sv2-manufacturing (imports sv2-tolerance, sv2-bom)
    ├── sv2-quality (imports sv2-tolerance, sv2-manufacturing)
    ├── sv2-capa (imports sv2-quality, sv2-sourcing)
    ├── sv2-verification-ext (imports sv2-risk, sv2-tolerance)
    └── sv2-project (imports sv2-risk)
```

**Naming conventions:** All definition names use PascalCase. Attribute names use camelCase. Enumeration values use camelCase. Package names use lowercase with hyphens. This aligns with the SysML v2 standard library conventions.

**Documentation:** Every definition includes a `doc` comment explaining its purpose, when to use it, and what CLI tool features consume it. This makes the libraries self-documenting and teaches users SysML v2 and MBSE simultaneously.

**Extensibility:** Libraries define abstract or general types that users specialize for their domain. A `RiskDef` provides the common structure; a user creates `AvionicsRiskDef :> RiskDef` with domain-specific severity scales. The CLI tools work with the base types, so specializations are automatically supported.

**Versioning:** Libraries carry a version attribute in their package-level metadata comment. The CLI tool checks library versions against its own compatibility matrix and warns if a library is newer than the tool supports.

### 2.2 Risk Management Library

**Package:** `SV2Risk`

**Purpose:** Provides type definitions for risk identification, categorization, assessment, and mitigation planning. Supports both simple qualitative scales (high/medium/low) and quantitative FMEA-style ratings (1–10 scales with RPN computation).

**Definitions provided:**

`RiskDef` — The core risk definition. A specialized `part def` carrying:
- `id` — unique identifier string attribute
- `title` — human-readable name
- `description` — detailed risk statement (as `doc`)
- `category` — enumeration: technical, schedule, cost, safety, regulatory, supply chain, environmental
- `status` — enumeration: identified, analyzing, mitigating, monitoring, closed, accepted
- `severity` — SeverityLevel enumeration or numeric 1–10
- `likelihood` — LikelihoodLevel enumeration or numeric 1–10
- `detectability` — DetectabilityLevel enumeration or numeric 1–10 (for FMEA)
- `riskPriorityNumber` — derived attribute, constrained as `severity * likelihood * detectability`
- `identifiedDate` — timestamp attribute
- `owner` — string attribute for responsible party

`SeverityLevel` — Enumeration: negligible, marginal, moderate, critical, catastrophic. Each value carries a `numericValue` attribute (1–5) for computation.

`LikelihoodLevel` — Enumeration: improbable, remote, occasional, probable, frequent. Same numeric mapping.

`DetectabilityLevel` — Enumeration: almostCertain, high, moderate, low, almostImpossible (inverted scale per FMEA convention where low detectability means high risk).

`RiskCategory` — Enumeration as listed above.

`RiskStatus` — Enumeration as listed above.

`MitigationDef` — A specialized `action def` representing a planned or active mitigation:
- `strategy` — enumeration: avoid, transfer, reduce, accept, contingency
- `effectivenessTarget` — percentage attribute representing expected risk reduction
- `owner` — responsible party
- `dueDate` — target completion date
- `status` — enumeration: planned, inProgress, implemented, verified, ineffective

`mitigates` — A custom `connection def` linking a `MitigationDef` to one or more `RiskDef` instances. Carries an `expectedReduction` attribute.

`affects` — A custom `connection def` linking a `RiskDef` to any part, requirement, interface, or action in the model. Enables impact analysis when risks materialize or when design changes affect risk exposure.

`RiskRegisterDef` — A container `part def` that aggregates `RiskDef` instances for a given scope (system, subsystem, project). Includes constraint expressions for register-level metrics: total risk count by category, count of risks above threshold severity, average RPN.

`RiskMatrixDef` — A metadata `part def` that defines the dimensions and thresholds of the risk matrix visualization. Carries severity axis labels, likelihood axis labels, and cell classification (acceptable, tolerable, unacceptable) as a nested enumeration matrix. The CLI tool reads this to render risk matrices that match the project's specific risk framework.

**What the CLI does with this:** `sv2 risk` reads `RiskDef` instances from the model, tracks assessment changes over time in TOML records, computes RPN trends, generates risk matrices using `RiskMatrixDef` configuration, manages mitigation status tracking, and performs Monte Carlo simulation using the numeric severity/likelihood values as probability distributions.

### 2.3 Tolerance and GD&T Library

**Package:** `SV2Tolerance`

**Purpose:** Provides type definitions for dimensional tolerances, geometric dimensioning and tolerancing, tolerance stack-up chain definitions, fit classifications, and process capability metadata. Imports from the SysML v2 ISQ/SI libraries for units.

**Definitions provided:**

`ToleranceDef` — A specialized `attribute def` carrying:
- `nominal` — nominal dimension value with units (using ISQ length quantities)
- `upperLimit` — upper tolerance bound
- `lowerLimit` — lower tolerance bound
- `distributionType` — enumeration: normal, uniform, triangular, skewedLeft, skewedRight, beta
- `processCapability` — optional Cp/Cpk target values
- `measurementMethod` — string reference to measurement technique
- `isCritical` — boolean flag for critical dimensions requiring 100% inspection

`BilateralToleranceDef` — Specialization of `ToleranceDef` where upper and lower limits are symmetric: `nominal ± tolerance`.

`UnilateralToleranceDef` — Specialization where tolerance applies in only one direction.

`LimitsToleranceDef` — Specialization defined by absolute max and min values rather than nominal ± deviation.

`DimensionChainDef` — A specialized `part def` representing an ordered stack-up:
- `contributors` — ordered collection of `ToleranceDef` references
- `closingDimension` — the resultant dimension being analyzed
- `stackDirection` — enumeration: linear, radial, angular
- `analysisMethod` — enumeration: worstCase, rss, monteCarlo, modifiedRSS
- `targetCapability` — Cp/Cpk requirement for the closing dimension

`StackUpConstraint` — A `constraint def` expressing the mathematical relationship between contributors. For linear stacks this is a simple summation. For more complex geometric relationships, the constraint captures the transfer function.

`FitDef` — A `part def` representing a shaft-hole fit relationship:
- `holeToleranceGrade` — IT grade enumeration (IT01 through IT18 per ISO 286)
- `shaftToleranceGrade` — IT grade enumeration
- `fitType` — enumeration: clearance, transition, interference
- `fitClass` — string attribute for standard designations (e.g., H7/g6)

`DatumDef` — Represents a GD&T datum reference:
- `label` — datum letter designation (A, B, C, etc.)
- `feature` — reference to the part feature establishing the datum
- `materialCondition` — enumeration: regardlessOfFeatureSize, maximumMaterial, leastMaterial

`FeatureControlFrameDef` — Represents a GD&T callout:
- `characteristic` — enumeration covering all 14 geometric characteristics: straightness, flatness, circularity, cylindricity, lineProfile, surfaceProfile, angularity, perpendicularity, parallelism, position, concentricity, symmetry, circularRunout, totalRunout
- `toleranceZone` — tolerance value with optional diameter symbol flag
- `materialCondition` — same enumeration as DatumDef
- `datumReferences` — ordered collection of up to three DatumDef references with material condition modifiers
- `compositeFrames` — optional collection for composite feature control frames

`SurfaceFinishDef` — Surface roughness specification:
- `ra` — arithmetic average roughness
- `rz` — mean roughness depth
- `method` — enumeration: machined, ground, lapped, polished, asForged, asCast

**What the CLI does with this:** `sv2 tol` reads `DimensionChainDef` and its linked `ToleranceDef` nodes, performs worst-case, RSS, and Monte Carlo stack-up analysis, generates distribution histograms, computes sensitivity rankings, and imports CMM measurement data to calculate actual Cp/Cpk against the defined targets.

### 2.4 BOM and Sourcing Library

**Package:** `SV2BOM`

**Purpose:** Extends SysML v2 part definitions with manufacturing and procurement metadata. SysML v2 already handles structural composition and multiplicity — this library adds the attributes that make parts procurable, trackable, and costed.

**Definitions provided:**

`PartIdentity` — An `attribute def` bundle applied to any part:
- `partNumber` — string, unique within the project
- `revision` — string, revision level
- `description` — human-readable part name
- `category` — enumeration: assembly, subassembly, component, rawMaterial, fastener, consumable, software, document
- `lifecycleState` — enumeration: concept, development, prototype, production, obsolete, discontinued
- `makeOrBuy` — enumeration: make, buy, makeAndBuy, tbd

`MaterialDef` — Material specification:
- `materialName` — string
- `specification` — string (e.g., "ASTM A36", "AMS 4911")
- `grade` — string
- `condition` — string (e.g., "T6", "annealed")
- `density` — mass per volume using ISQ units
- `recyclable` — boolean

`MassProperty` — An `attribute def`:
- `mass` — using ISQ mass quantities
- `massUnit` — enumeration: actual, estimated, calculated, allocated
- `massMargin` — percentage margin applied to estimated masses

`CostProperty` — An `attribute def`:
- `unitCost` — decimal with currency indication
- `toolingCost` — non-recurring cost
- `costBasis` — enumeration: quoted, estimated, historical, target
- `effectiveDate` — timestamp

`SupplierDef` — A `part def` representing a vendor:
- `companyName` — string
- `supplierCode` — unique identifier
- `qualificationStatus` — enumeration: pending, conditional, approved, preferred, probation, disqualified
- `contactInfo` — structured attribute with address, phone, email
- `certifications` — collection of certification strings (ISO 9001, AS9100, ISO 13485, etc.)

`SourceDef` — A `connection def` linking a part to a supplier:
- `supplierPartNumber` — the vendor's part number
- `leadTimeDays` — standard lead time
- `minimumOrderQuantity` — MOQ
- `packageQuantity` — standard pack size
- `sourceType` — enumeration: sole, single, dual, multi
- `isPreferred` — boolean

`ApprovedSourceList` — A container `part def` aggregating `SourceDef` connections for controlled parts.

**What the CLI does with this:** `sv2 bom` walks the composition hierarchy, multiplies quantities through assembly levels, rolls up mass and cost, generates indented BOMs in various formats, performs where-used analysis, and compares BOMs across revisions. `sv2 source` manages quoting workflows, tracks supplier performance, and generates RFQ packages from part specifications.

### 2.5 Manufacturing Process Library

**Package:** `SV2Manufacturing`

**Purpose:** Defines manufacturing process structures, routings, process parameters with control limits, work instructions, and in-process inspection points. Uses SysML v2 `action def` as the base for process steps since manufacturing processes are fundamentally sequences of actions.

**Definitions provided:**

`ProcessDef` — A specialized `action def`:
- `processType` — enumeration: machining, welding, brazing, soldering, adhesiveBonding, molding, casting, forging, stamping, sheetMetal, heatTreat, surfaceTreatment, coating, assembly, testAndInspection, packaging, cleaning, printing3d, programming, calibration
- `workCenter` — string identifying the workstation or area
- `setupTimeMinutes` — setup/changeover time
- `cycleTimeMinutes` — per-unit processing time
- `requiredTooling` — collection of string references to tooling IDs
- `requiredFixtures` — collection of fixture references
- `safetyRequirements` — collection of safety callout strings (PPE, lockout/tagout, ventilation, etc.)
- `environmentalControls` — collection of environmental requirements (temperature, humidity, cleanliness)

`ProcessParameterDef` — A specialized `attribute def` for controlled variables:
- `parameterName` — descriptive name
- `nominal` — target value with units
- `upperControlLimit` — UCL for SPC
- `lowerControlLimit` — LCL for SPC
- `upperSpecLimit` — USL (specification, wider than control)
- `lowerSpecLimit` — LSL
- `monitoringMethod` — enumeration: continuous, periodic, perUnit, perLot, perSetup
- `spcRule` — enumeration: westernElectric, nelsonRules, customRule

`WorkInstructionDef` — Operator-facing step definition:
- `stepNumber` — integer sequence
- `instruction` — human-readable text (as `doc`)
- `safetyWarning` — optional hazard callout
- `qualityCheckpoint` — optional boolean flag indicating operator self-inspection
- `requiredSkillLevel` — enumeration: entry, intermediate, certified, specialist
- `visualAids` — collection of reference strings to diagrams or photos
- `estimatedTimeMinutes` — expected time for this step

`InspectionPointDef` — Embedded in-process inspection:
- `inspectionType` — enumeration: dimensional, visual, functional, destructive, nonDestructive
- `verificationCase` — reference to a SysML v2 `verification case def`
- `samplingRate` — enumeration: everyUnit, perLot, firstArticle, periodic
- `gateType` — enumeration: mandatory (stops process), advisory (flags but continues)

`RoutingDef` — An ordered collection of `ProcessDef` references constituting the full manufacturing sequence for a part:
- `steps` — ordered succession of `ProcessDef` usages
- `alternateRouting` — optional reference to an alternate routing for capacity flexibility
- `revision` — routing revision level
- `effectiveDate` — date this routing becomes active

`ProcessDeviationDef` — Type definition for process excursions:
- `deviationType` — enumeration: parameterExcursion, toolingSubstitution, sequenceChange, materialSubstitution, operatorOverride
- `dispositionRequired` — boolean flag
- `requiresEngApproval` — boolean flag for engineering sign-off

**What the CLI does with this:** `sv2 mfg` creates lot travelers from `RoutingDef`, guides operators through each `WorkInstructionDef` interactively, records actual `ProcessParameterDef` values, flags excursions against control and specification limits, manages process deviations, tracks WIP across lots, and generates SPC charts from collected parameter data.

### 2.6 Quality and Inspection Library

**Package:** `SV2Quality`

**Purpose:** Defines inspection plans, measurement specifications, acceptance criteria, and quality classification types. Works in conjunction with the tolerance library for dimensional acceptance and the manufacturing library for in-process inspection.

**Definitions provided:**

`InspectionPlanDef` — Top-level inspection specification for a part:
- `planType` — enumeration: incoming, inProcess, final, firstArticle, periodicRequalification
- `applicablePart` — reference to the part being inspected
- `samplingStandard` — enumeration: ansiZ14, iso2859, c0Sampling, hundredPercent, custom
- `aqlLevel` — acceptable quality level (0.065, 0.10, 0.15, 0.25, 0.40, 0.65, 1.0, 1.5, 2.5, 4.0, 6.5)
- `inspectionLevel` — enumeration: reducedI, normalII, tightenedIII, specialS1, specialS2, specialS3, specialS4
- `characteristics` — collection of `QualityCharacteristicDef` usages

`QualityCharacteristicDef` — A single measurable or observable feature:
- `characteristicName` — descriptive name
- `classification` — enumeration: critical, major, minor, informational
- `measurementType` — enumeration: variable (continuous), attribute (go/noGo)
- `tolerance` — reference to a `ToleranceDef` from the tolerance library (for variable characteristics)
- `acceptanceCriteria` — `constraint def` expressing pass/fail logic
- `measurementMethod` — string describing technique
- `requiredInstrument` — string identifying measurement equipment
- `instrumentAccuracy` — required instrument accuracy/resolution

`MeasurementDef` — Specification for a single measurement:
- `measuredCharacteristic` — reference to `QualityCharacteristicDef`
- `instrumentType` — enumeration: caliper, micrometer, cmm, opticalComparator, surfaceProfiler, gaugePin, threadGauge, hardnessTester, tensionTester, visual, custom
- `uncertaintyBudget` — measurement uncertainty value
- `calibrationRequirement` — boolean flag

`GaugeRRDef` — Gauge repeatability and reproducibility study specification:
- `studyType` — enumeration: crossed, nested, expandedMultiFactor
- `numberOfOperators` — integer
- `numberOfParts` — integer
- `numberOfTrials` — integer
- `acceptablePTV` — acceptable percent tolerance consumed by measurement variation (typically 10% or 30%)

`CertificateOfConformanceDef` — Template definition for CoC documents:
- `requiredSections` — collection of enumeration values: materialCerts, dimensionalResults, functionalTestResults, visualInspection, processTraceability, specialProcessCerts, regulatoryCompliance

**What the CLI does with this:** `sv2 qc` executes inspections guided by `InspectionPlanDef`, applies sampling plan mathematics to determine sample sizes and accept/reject numbers, collects measurements interactively, imports CMM and instrument data files, runs Gauge R&R analysis, computes statistical metrics (Cp, Cpk, Pp, Ppk), generates control charts, and produces certificates of conformance.

### 2.7 CAPA Library

**Package:** `SV2CAPA`

**Purpose:** Defines the controlled vocabulary for nonconformance and corrective/preventive action management. This is the thinnest library because CAPA is almost entirely an operational workflow. The library exists to standardize the enumerations that the CLI enforces.

**Definitions provided:**

`NonconformanceCategoryDef` — Enumeration: dimensional, material, cosmetic, functional, workmanship, documentation, labeling, packaging, contamination, software.

`SeverityClassDef` — Enumeration: critical (safety/regulatory impact), major (functional impact), minor (cosmetic or documentation), observation (potential issue, no nonconformance yet).

`DispositionDef` — Enumeration: useAsIs, rework, repair, scrap, returnToVendor, sortAndScreen, deviate.

`CorrectiveActionTypeDef` — Enumeration: designChange, processChange, supplierChange, toolingChange, trainingRetraining, procedureUpdate, inspectionEnhancement, containment, noActionRequired.

`RootCauseMethodDef` — Enumeration: fiveWhy, fishbone (ishikawa), faultTreeAnalysis, eightD, kepnerTregoe, is_isNot, paretoAnalysis.

`CapaStatusDef` — Enumeration: initiated, investigating, rootCauseIdentified, actionPlanned, actionImplemented, effectivenessVerified, closed, closedIneffective, reopened.

`EscalationTriggerDef` — Constraint definitions that specify when a nonconformance requires escalation: repeat occurrences of the same failure mode within a time window, critical severity, regulatory-reportable conditions, customer-affecting escapes.

**What the CLI does with this:** `sv2 capa` manages the entire NCR lifecycle: creation, investigation tracking, root cause documentation, corrective action assignment and tracking, effectiveness verification (linking to `sv2 verify` executions), trend analysis across NCR history, and regulatory export formatting. The tool enforces the vocabulary from this library so that all NCRs use consistent categorization.

### 2.8 Verification Extensions Library

**Package:** `SV2VerificationExt`

**Purpose:** Extends the standard SysML v2 verification constructs with additional metadata that the CLI needs for interactive test execution, evidence management, and coverage tracking. The standard `verification case def` and `verify` relationship are sufficient for defining what needs to be tested and linking tests to requirements. This library adds the operational metadata.

**Definitions provided:**

`VerificationClassificationDef` — Enumeration extending the standard: analysis, demonstration, inspection, test, simulation, certification, similarity.

`VerificationMaturityDef` — Enumeration: planned, procedureInDevelopment, procedureApproved, readyToExecute, executed, reportApproved.

`EquipmentRequirementDef` — An `attribute def` for test equipment specifications:
- `equipmentType` — string
- `minimumAccuracy` — value with units
- `calibrationRequired` — boolean
- `calibrationInterval` — duration

`PersonnelRequirementDef` — An `attribute def`:
- `requiredCertification` — string
- `minimumExperience` — string description
- `supervisorRequired` — boolean

`EnvironmentalConditionDef` — An `attribute def` for test environment:
- `temperatureRange` — min/max with units
- `humidityRange` — min/max percentage
- `cleanlinessClass` — enumeration per ISO 14644

`TestProcedureStepDef` — A specialized `action def` for individual test steps within a verification case:
- `stepNumber` — integer
- `instruction` — text (as `doc`)
- `expectedResult` — text describing expected outcome
- `safetyWarnings` — collection of hazard callouts
- `inputRequired` — boolean indicating operator data entry needed
- `measurementRequired` — boolean indicating quantitative data collection
- `evidenceRequired` — boolean indicating evidence capture (photo, file, etc.)

**What the CLI does with this:** `sv2 verify` uses `TestProcedureStepDef` sequences to build interactive walkthroughs, reads `EquipmentRequirementDef` to generate pre-test checklists, validates that environmental conditions and personnel qualifications are recorded before allowing execution to proceed, and tracks verification maturity lifecycle.

### 2.9 Project Management Library

**Package:** `SV2Project`

**Purpose:** Lightweight project management definitions for milestone tracking, work breakdown, and review gates. Not a full project management system — that is not what SysML is for — but enough structure to link engineering artifacts to project phases and track design review readiness.

**Definitions provided:**

`PhaseDef` — Project phase:
- `phaseType` — enumeration: concept, preliminaryDesign, detailedDesign, prototyping, designVerification, processValidation, production, sustaining

`MilestoneDef` — A specialized `action def` representing a project gate:
- `phase` — reference to `PhaseDef`
- `requiredArtifacts` — collection of references to requirements, verification cases, risk assessments, and other model elements that must be complete/approved to pass the gate
- `reviewType` — enumeration: systemRequirementsReview, preliminaryDesignReview, criticalDesignReview, testReadinessReview, firstArticleReview, productionReadinessReview

`DesignReviewChecklistDef` — Template for review checklists:
- `requiredCoverageThreshold` — minimum verification coverage percentage
- `maxOpenCriticalRisks` — maximum allowed critical risks
- `maxOpenNCRs` — maximum allowed open nonconformances
- `requiredDocuments` — collection of document type enumerations

**What the CLI does with this:** `sv2 check --gate pdr` evaluates a milestone gate by resolving all `requiredArtifacts`, checking verification coverage thresholds, counting open risks and NCRs, and producing a gate readiness report showing which criteria pass and which block the review.

---

## 3. CLI Tool Architecture

### 3.1 Binary Structure and Subcommands

The `sv2` tool is a single Rust binary. Subcommands map to Cargo workspace crates internally, but the user sees one installation, one binary, one help system.

```
sv2 <subcommand> [options] [arguments]

Subcommands:
  init        Initialize a project workspace
  index       Build or rebuild the model cache
  check       Validate references, coverage, and gate readiness
  verify      Verification case execution and coverage
  risk        Risk assessment and tracking
  tol         Tolerance analysis and stack-ups
  bom         BOM rollup, where-used, comparison
  source      Supplier management and quoting
  mfg         Manufacturing lot execution and tracking
  qc          Quality control and inspection
  capa        Nonconformance and corrective action management
  report      Cross-domain report generation
  pipeline    Workflow pipeline execution
  scaffold    Generate SysML v2 templates and examples
  help        Contextual help and MBSE guidance
```

Every subcommand supports common flags:

```
--model <path>      Path to SysML v2 model root (default: auto-discover)
--cache <path>      Path to SQLite cache (default: .sv2/cache.db)
--config <path>     Path to config file (default: .sv2/config.toml)
--output <path>     Output file path (default: stdout or configured default)
--format <format>   Output format: text, json, toml, csv, html (default: text)
--verbose           Increase output detail
--quiet             Suppress non-essential output
--dry-run           Show what would be done without writing files
--no-color          Disable colored output
```

### 3.2 Configuration and Project Discovery

**Project discovery** follows a walk-up strategy from the current working directory:
1. Look for `.sv2/config.toml` in the current directory
2. Walk parent directories until found or filesystem root is reached
3. If not found, operate in standalone mode: commands work but without project-level defaults

**Configuration file** (`.sv2/config.toml`) provides defaults:

```toml
[project]
name = "BrakeSystem"
model_root = "model/"
library_paths = ["libraries/"]

[cache]
path = ".sv2/cache.db"
auto_rebuild = true

[defaults]
author = "jhale"
output_dir = "records/"

[records]
verification_dir = "records/verification/"
risk_dir = "records/risk/"
tolerance_dir = "records/tolerance/"
# ... etc. All optional. If not set, defaults to records/<domain>/

[hooks]
post_record = "sv2 index --incremental"
post_verify_fail = "sv2 capa suggest-ncr --from-execution {record_id}"

[[pipelines]]
name = "first-article"
description = "First article inspection workflow"
steps = [
  "sv2 mfg start-lot --routing {routing_qn} --type first-article",
  "sv2 verify run --campaign first-article --lot {lot_id}",
  "sv2 qc start-final --lot {lot_id}",
  "sv2 qc generate-coc --lot {lot_id}",
]
```

**Environment variables** override config for CI/CD and scripting:
- `SV2_MODEL_ROOT` — model file root
- `SV2_CACHE` — cache database path
- `SV2_OUTPUT` — default output directory
- `SV2_AUTHOR` — default author name
- `SV2_CONFIG` — config file path

**Precedence:** command-line flags > environment variables > config file > built-in defaults.

### 3.3 Model Indexing and Cache

The SQLite cache is the performance layer between SysML v2 model files and CLI tool queries. It is never the source of truth.

**Cache lifecycle:**
1. First command invocation detects no cache exists → automatic full build
2. Subsequent invocations compare cache's stored git HEAD hash against current HEAD → rebuild if stale
3. `sv2 index --incremental` rebuilds only for files changed since last index (using git diff)
4. `sv2 index --full` forces complete rebuild
5. Any tool encountering a cache miss on a qualified name triggers a warning and suggests re-indexing

**Index build process:**
1. Parse all `.sysml` files under `model_root` and `library_paths` using the tree-sitter grammar
2. Extract every named element: package, part def, part usage, action def, attribute def, enum def, requirement def, constraint def, connection def, verification case, and their qualified names
3. Extract relationship edges: composition, specialization, satisfy, verify, connect, typed by
4. Populate the `nodes` table with qualified name, element type, source file, and line number
5. Parse all TOML records under configured record directories
6. Populate the `records` and `ref_edges` tables
7. Store the current git HEAD hash as the cache version marker

**Cache is gitignored.** Always. Each developer rebuilds locally. This is non-negotiable.

### 3.4 Record Storage and File Conventions

Operational records are TOML files. Every record follows a common envelope structure:

```toml
[meta]
id = "exec-20260308-143000-jhale-a1b2"
tool = "verify"
schema_version = "1.0"
created = 2026-03-08T14:30:00Z
modified = 2026-03-08T15:45:00Z
author = "jhale"

[refs]
# All qualified name references live here
# This table is indexed by sv2 index for fast lookups

[data]
# Domain-specific content lives here
# Structure varies by tool and record type
```

**Filename conventions:**

Append-only records (executions, inspections, lot travelers):
```
{domain}-{ISO8601-compact}-{author}-{4char-hash}.toml
verify-20260308T143000-jhale-a1b2.toml
```

Entity records (risks, NCRs, corrective actions):
```
{domain}-{entity-id}.toml
risk-RSK-0042.toml
ncr-NCR-2026-0017.toml
```

**Serialization rules:**
- All maps use `BTreeMap` for deterministic key ordering
- Dates use RFC 3339 format
- Qualified names are always full paths (never abbreviated)
- Numeric values include units in the key name when not obvious: `force_n`, `response_ms`, `temperature_c`
- Arrays of qualified names are always sorted alphabetically

### 3.5 Output Flexibility

Every command that produces output supports multiple destinations and formats:

**Default behavior:** Output goes to stdout as human-readable text. This means commands compose with standard Unix tools: `sv2 risk list | grep critical`, `sv2 bom rollup Vehicle | wc -l`, `sv2 verify coverage Vehicle::BrakeSystem | less`.

**File output:** `--output path/to/file.toml` writes to a specific file. The file extension determines format if `--format` is not specified.

**Configured defaults:** If `defaults.output_dir` is set in config, record-producing commands write files there automatically. The user is told the file path.

**Formats:**
- `text` — human-readable terminal output with optional color
- `json` — machine-parseable, suitable for piping to `jq`
- `toml` — for records that will be committed to git
- `csv` — tabular data for spreadsheet import
- `html` — single-file reports with embedded charts
- `sysml` — when the output is a model fragment (scaffolding commands)

---

## 4. Subcommand Specification

### 4.1 sv2 init and sv2 index

```
sv2 init [--name <project-name>] [--model-root <path>]
```

Creates `.sv2/` directory, generates a starter `config.toml`, adds `.sv2/cache.db` to `.gitignore`, and optionally creates the model root directory. If the current directory already contains `.sysml` files, it detects them and configures the model root automatically.

Interactive mode (default): asks the user what domains they plan to use and generates a config file with appropriate record directories and library imports.

```
sv2 index [--full | --incremental] [--stats]
```

Builds or rebuilds the SQLite cache. `--stats` reports counts of nodes, records, and reference edges. Exit code 0 means success; exit code 1 means parse errors were encountered (reported as warnings with file:line locations).

### 4.2 sv2 check

```
sv2 check [--scope <qualified-name>] [--gate <gate-name>] [--fix]
```

The health check and gate readiness command. Without arguments, it reports:

- **Broken references:** TOML records pointing to qualified names not in the model. Reports the record file, the reference field, and the missing qualified name.
- **Orphaned records:** TOML records with no valid model references at all.
- **Coverage gaps:** Model nodes of types that typically should have associated records (requirements without verification cases, parts without tolerance specifications) but don't. These are informational, not errors.
- **Stale records:** Execution records older than a configurable threshold.

With `--gate <gate-name>`, evaluates a `MilestoneDef` by checking all `requiredArtifacts`, coverage thresholds, open risk and NCR counts. Produces a pass/fail gate readiness report.

With `--fix`, offers to interactively resolve broken references by searching for likely renamed qualified names (using edit distance matching against the current model index).

### 4.3 sv2 verify

```
sv2 verify run <verification-case-qn> [--lot <lot-id>] [--campaign <name>]
sv2 verify coverage [<scope-qn>] [--format text|json|html|csv]
sv2 verify status <execution-id>
sv2 verify list [--status pass|fail|pending] [--scope <qn>]
sv2 verify history <verification-case-qn>
```

**`sv2 verify run`** is the interactive test execution engine. It:
1. Resolves the verification case from the model
2. Displays pre-test information: purpose, requirements being verified, equipment needed, environmental conditions, personnel requirements (from the extensions library)
3. Prompts for pre-test confirmations: equipment calibrated, environment within spec, operator qualified
4. Steps through each `TestProcedureStepDef` in sequence, displaying instructions, safety warnings, and expected results
5. At each step, prompts for required inputs (measurements, observations, pass/fail decisions)
6. Validates numeric inputs against acceptance criteria defined in the model's constraint expressions
7. Collects evidence references (file paths) when `evidenceRequired` is true, computes and stores SHA-256 hashes
8. Supports branching: conditional steps based on previous results
9. On completion, writes a TOML execution record with all inputs, results, and evidence references
10. Reports overall pass/fail and which requirements were satisfied or not
11. If the result is a failure and hooks are configured, triggers the post-verify-fail hook

**`sv2 verify coverage`** generates a traceability matrix. For every requirement within the scope, it shows: the requirement ID and title, linked verification cases, execution count and most recent result, and a coverage status (verified, executed-but-failed, case-defined-but-not-executed, no-verification-case). Output includes summary statistics: total requirements, percent covered, percent verified (passed).

### 4.4 sv2 risk

```
sv2 risk add <title> [--category <cat>] [--severity <level>] [--likelihood <level>]
sv2 risk assess <risk-qn-or-id> [--severity <level>] [--likelihood <level>] [--detectability <level>]
sv2 risk mitigate <risk-qn-or-id> --action <description> [--owner <name>] [--due <date>]
sv2 risk matrix [--scope <qn>] [--format text|html]
sv2 risk list [--status <status>] [--category <cat>] [--above-rpn <threshold>]
sv2 risk trend <risk-qn-or-id>
sv2 risk fmea [--scope <qn>] [--format csv|html]
sv2 risk impact <qn>
```

**`sv2 risk add`** is notable because it creates both a SysML v2 model element and a TOML assessment record. Interactive mode (default when flags are omitted) walks the user through risk identification: what could go wrong, what is the consequence, what is the cause, how likely is it. This is the tool teaching MBSE through practice — the wizard explains each field's purpose as it asks for input.

**`sv2 risk matrix`** reads `RiskMatrixDef` from the model to determine axis labels and cell classifications, then populates the matrix from current assessment data. Terminal output uses color-coded cells. HTML output generates an interactive matrix.

**`sv2 risk impact`** takes any model node (part, requirement, interface) and finds all risks that `affect` it through the connection relationships, plus any risks that affect parent or child nodes in the composition tree. This is the answer to "what happens if this part fails?"

**`sv2 risk fmea`** generates an FMEA worksheet. In interactive mode, it walks through each part in scope and for each potential failure mode prompts for severity, occurrence, detection ratings, and recommended actions. The output is a structured TOML record and optionally a formatted HTML or CSV report.

### 4.5 sv2 tol

```
sv2 tol analyze <dimension-chain-qn> [--method worst-case|rss|monte-carlo] [--iterations <n>]
sv2 tol sensitivity <dimension-chain-qn>
sv2 tol whatif <dimension-chain-qn> --set <tolerance-qn>=<new-value>
sv2 tol capability <tolerance-qn> --data <measurements-file>
sv2 tol import-measurements <file> --map <mapping-file>
sv2 tol report <dimension-chain-qn> [--format html]
```

**`sv2 tol analyze`** reads a `DimensionChainDef`, resolves all `ToleranceDef` contributors, and performs the requested analysis. Monte Carlo uses the `distributionType` from each `ToleranceDef` to sample appropriately. Output includes: closing dimension nominal, worst-case min/max, RSS min/max (if applicable), Monte Carlo distribution percentiles (if applicable), estimated Cp/Cpk of the closing dimension, and percent out-of-specification.

**`sv2 tol sensitivity`** ranks contributors by their contribution to total variation. For RSS analysis, this is the partial derivative (sensitivity coefficient) squared times the variance. For Monte Carlo, it's computed by correlation analysis on the simulation data. Output tells the engineer which tolerance to tighten for the greatest improvement.

**`sv2 tol whatif`** re-runs the analysis with one or more tolerances modified, showing before/after comparison. This enables rapid iteration without editing the model.

**`sv2 tol capability`** imports actual measurement data and computes Cp, Cpk, Pp, Ppk against the tolerance specification from the model. Flags processes that are below the `processCapability` target defined in the `ToleranceDef`.

### 4.6 sv2 bom

```
sv2 bom rollup <part-qn> [--format tree|flat|csv|xlsx] [--include-mass] [--include-cost]
sv2 bom where-used <part-qn>
sv2 bom compare <part-qn> --rev-a <git-ref> --rev-b <git-ref>
sv2 bom mass-budget <part-qn> [--format text|html]
sv2 bom export <part-qn> --format <csv|xlsx>
sv2 bom find <query> [--category <cat>] [--material <material>]
```

**`sv2 bom rollup`** recursively walks the part composition tree from the SysML v2 model. At each level, it multiplies the part's quantity (from SysML v2 multiplicity) by the parent quantity to compute extended quantities. With `--include-mass`, it reads `MassProperty` attributes and sums through the tree. With `--include-cost`, it reads `CostProperty` and computes extended costs.

Tree format output shows indented assembly structure:
```
Vehicle (1)
├── BrakeSystem (1) — 4.2 kg — $342.00
│   ├── BrakePedal (1) — 0.8 kg — $28.00
│   ├── MasterCylinder (1) — 1.2 kg — $85.00
│   ├── BrakeCaliper (4) — 0.4 kg ea — $45.00 ea
│   └── BrakeRotor (4) — 0.2 kg ea — $21.00 ea
```

**`sv2 bom compare`** checks out the model at two git refs, indexes both, and compares the BOM trees. Output shows added parts, removed parts, quantity changes, and attribute changes (mass, cost, material). This is essential for design review preparation.

**`sv2 bom mass-budget`** compares actual masses (from `MassProperty` where `massUnit` is "actual") against allocated masses, showing margin at each assembly level. Flags assemblies where margin is negative or below a configurable threshold.

### 4.7 sv2 source

```
sv2 source rfq <part-qn> [--suppliers <supplier-qn,...>] [--quantity <n>]
sv2 source quote add <part-qn> --supplier <supplier-qn> [interactive prompts for pricing]
sv2 source compare <part-qn>
sv2 source scorecard <supplier-qn>
sv2 source asl <part-qn>
```

**`sv2 source rfq`** generates an RFQ package by reading part specifications, tolerances, and material requirements from the model and formatting them into a structured document (TOML + optional HTML) that can be sent to suppliers. If suppliers are not specified, it lists all `SourceDef` connections for the part.

**`sv2 source compare`** presents a side-by-side comparison of all quotes for a part across suppliers, including unit price, tooling, lead time, MOQ, and total cost at various quantities. Highlights the best option for each criterion.

**`sv2 source scorecard`** computes supplier performance metrics from historical data: on-time delivery rate, quality acceptance rate (from `sv2 qc` incoming inspection records), price competitiveness, and responsiveness. Reads `SupplierDef` qualification status from the model.

### 4.8 sv2 mfg

```
sv2 mfg start-lot <routing-qn> --quantity <n> [--type production|prototype|firstArticle]
sv2 mfg step <lot-id> [--step <n>]
sv2 mfg deviate <lot-id> --step <n> --type <deviation-type> --description <text>
sv2 mfg status [<lot-id>] [--scope <part-qn>]
sv2 mfg spc <process-parameter-qn> [--lot <lot-id>] [--format text|html]
sv2 mfg wip [--scope <part-qn>]
sv2 mfg cycle-time <routing-qn> [--lots <n>]
```

**`sv2 mfg start-lot`** creates a lot traveler record from a `RoutingDef`. The traveler contains the full sequence of process steps with their work instructions, parameters to be recorded, and inspection points. Each lot gets a unique ID.

**`sv2 mfg step`** is the interactive lot execution command, analogous to `sv2 verify run` but for manufacturing. It guides the operator through the current process step's `WorkInstructionDef` instructions, prompts for process parameter values, validates against control and specification limits in real time, collects operator sign-off, and advances to the next step. If an `InspectionPointDef` is embedded in the step, it invokes the inspection workflow (potentially delegating to `sv2 qc` via a hook).

If a recorded parameter exceeds control limits but is within specification limits, the tool flags a process excursion and prompts for deviation documentation. If the parameter exceeds specification limits, the step is failed and the lot is held pending disposition.

**`sv2 mfg spc`** generates statistical process control charts (X-bar/R, X-bar/S, individuals/moving range) from collected parameter data across lots. Applies the selected SPC rules from `ProcessParameterDef.spcRule` and highlights out-of-control conditions.

### 4.9 sv2 qc

```
sv2 qc inspect <inspection-plan-qn> --lot <lot-id> [--type incoming|inProcess|final]
sv2 qc sample-size <lot-size> [--aql <level>] [--inspection-level <level>]
sv2 qc gauge-rr <gauge-rr-qn> [interactive study execution]
sv2 qc capability <characteristic-qn> --data <file>
sv2 qc coc <lot-id> [--format html|pdf]
sv2 qc trend [--characteristic <qn>] [--part <qn>] [--period <days>]
sv2 qc import <file> [--instrument <type>] [--map <mapping-file>]
```

**`sv2 qc inspect`** executes an inspection guided by `InspectionPlanDef`. For each `QualityCharacteristicDef`, it:
1. Determines the sample size from the sampling standard, AQL, inspection level, and lot size
2. Prompts for measurements (variable) or pass/fail counts (attribute)
3. Applies accept/reject criteria from the sampling plan
4. For variable data, validates against `ToleranceDef` limits
5. Reports characteristic-level and lot-level accept/reject decisions
6. Writes an inspection record to TOML

**`sv2 qc coc`** generates a certificate of conformance by aggregating: part identification, lot traceability (from `sv2 mfg`), inspection results (from this tool), material certifications (referenced from the model), special process certifications, and requirement compliance status (from `sv2 verify`). The sections included are driven by `CertificateOfConformanceDef`.

### 4.10 sv2 capa

```
sv2 capa ncr create --part <part-qn> [--lot <lot-id>] [--supplier <supplier-qn>]
sv2 capa ncr list [--status <status>] [--severity <class>] [--part <qn>]
sv2 capa ncr disposition <ncr-id> --disposition <disposition> [--justification <text>]
sv2 capa rca <ncr-id> --method <method>
sv2 capa action add <ncr-id> --type <action-type> --description <text> --owner <name> --due <date>
sv2 capa action close <action-id> [--effective | --ineffective]
sv2 capa verify-effectiveness <action-id> --execution <verify-execution-id>
sv2 capa trend [--period <days>] [--group-by part|supplier|category|process]
sv2 capa escalation-check
```

**`sv2 capa ncr create`** in interactive mode walks the user through nonconformance documentation: what was found, where it was found (incoming/in-process/final/field), which part and lot are affected, severity classification (using `SeverityClassDef` vocabulary), nonconformance category, and preliminary containment actions. This is where the tool's guidance is most valuable — quality engineers filling out NCRs under time pressure benefit from structured prompts that ensure nothing is missed.

**`sv2 capa rca`** guides root cause analysis using the selected method. For 5-Why, it iteratively prompts "Why?" and records each level. For fishbone/Ishikawa, it prompts for causes in each standard category (man, machine, method, material, measurement, environment). The structured output becomes part of the NCR record.

**`sv2 capa verify-effectiveness`** links a corrective action to a `sv2 verify` execution record that demonstrates the fix works. This closes the loop: nonconformance → root cause → corrective action → verification → effectiveness confirmed.

**`sv2 capa trend`** analyzes NCR history to identify systemic issues: most common failure modes, worst-performing suppliers, process steps with highest defect rates, and trend lines over time. This is the data that drives continuous improvement.

**`sv2 capa escalation-check`** evaluates `EscalationTriggerDef` constraints against current NCR data. Alerts when escalation criteria are met (e.g., third occurrence of the same failure mode within 90 days on the same part).

### 4.11 sv2 report

```
sv2 report dashboard [--scope <qn>] [--format html]
sv2 report traceability <requirement-qn> [--depth full]
sv2 report gate <gate-name> [--format html]
sv2 report design-history <part-qn>
sv2 report supplier <supplier-qn> [--format html]
```

**`sv2 report dashboard`** generates a cross-domain project health summary: requirement count and verification coverage percentage, open risk count by severity, NCR count by status, active lot count and on-time percentage, BOM mass budget status. This is the view a program manager or chief engineer uses.

**`sv2 report traceability`** follows a requirement through its full lifecycle chain: requirement → verification case → execution results → affected parts → tolerance specifications → manufacturing processes → inspection results → any NCRs. This is the end-to-end thread that auditors and regulatory bodies ask for.

**`sv2 report design-history`** generates a design history file for a part: all requirements it satisfies, all verification evidence, all risk assessments that affect it, its tolerance specifications and capability data, its manufacturing routing, its quality history, and any NCRs. This is essential for regulated industries (FDA DHF, aerospace certification packages).

### 4.12 sv2 pipeline

```
sv2 pipeline run <pipeline-name> [--param key=value ...]
sv2 pipeline list
sv2 pipeline dry-run <pipeline-name> [--param key=value ...]
sv2 pipeline create <pipeline-name> [interactive]
```

**`sv2 pipeline run`** executes a sequence of `sv2` subcommands defined in the config file. Parameters are substituted using `{param_name}` syntax. Each step's output is displayed, and the pipeline pauses on failure with the option to retry, skip, or abort.

**`sv2 pipeline create`** interactively builds a pipeline definition by prompting for steps, parameters, and descriptions. Writes the result to the config file. This is how the tool teaches workflow design — the user constructs their standard operating procedures as composable pipeline definitions.

---

## 5. Guided User Experience

### 5.1 Progressive Disclosure

The tool has a layered complexity model. A new user encounters only what they need:

**Level 0 — Zero config.** `sv2 scaffold example brake-system` generates a complete example project with model files, libraries, and sample records. The user explores a working system before building their own.

**Level 1 — Single domain.** A user can use `sv2 verify` without ever touching risk, tolerance, or manufacturing. Each subcommand works independently. Cross-domain references are optional enrichments, not requirements.

**Level 2 — Configured project.** `sv2 init` sets up a project with selected domains. Config file provides defaults. Record directories are created. Libraries are copied into the project.

**Level 3 — Integrated workflows.** Pipelines, hooks, and cross-domain reports tie everything together. Gate checks enforce completeness across domains.

The tool never requires a higher level than the task demands.

### 5.2 Scaffolding and Templates

```
sv2 scaffold <type> [options]

Types:
  example <name>          Complete example project (brake-system, medical-device, consumer-electronics)
  library <domain>        Generate a domain library with comments explaining every construct
  requirement             Scaffold a requirement def with doc comments
  verification-case       Scaffold a verification case with step template
  risk                    Scaffold a risk def with assessment attributes
  process                 Scaffold a manufacturing process with parameters
  inspection-plan         Scaffold an inspection plan with characteristics
  routing                 Scaffold a manufacturing routing
  tolerance-chain         Scaffold a dimension chain with contributors
  part-with-bom           Scaffold a part with BOM attributes
```

Every scaffold command generates valid SysML v2 with extensive `doc` comments explaining what each construct means, why it matters, and how the CLI tools consume it. The scaffolded code is meant to be edited — it is a starting point with explanations, not a black box.

Example output from `sv2 scaffold risk`:

```sysml
package RiskRegister {
    /* Import the SV2 risk management library.
     * This provides RiskDef, MitigationDef, and the
     * severity/likelihood enumerations your risk
     * assessments will use.
     *
     * The sv2 risk commands read these definitions
     * to validate your assessments and generate
     * risk matrices.
     */
    import SV2Risk::*;

    /* Define a risk by specializing RiskDef.
     * Each risk should describe:
     *   - What could go wrong (the title and description)
     *   - How bad it would be (severity)
     *   - How likely it is (likelihood)
     *   - What it affects in the system (using 'affects' connections)
     *
     * Run 'sv2 risk add' to create risks interactively,
     * or write them directly in SysML like this.
     */
    part riskExample : RiskDef {
        /* The 'doc' comment becomes the risk description.
         * Be specific: describe the failure mode, its cause,
         * and its consequence.
         */
        doc /* Brake fluid leak at master cylinder seal
             * causes loss of braking pressure, resulting
             * in extended stopping distance. */

        attribute redefines category = RiskCategory::safety;
        attribute redefines severity = SeverityLevel::critical;
        attribute redefines likelihood = LikelihoodLevel::remote;
        attribute redefines status = RiskStatus::mitigating;
    }

    /* Connect risks to the parts they affect.
     * This enables 'sv2 risk impact <part>' to find
     * all risks relevant to a component.
     */
    connection riskToComponent : affects
        connect riskExample to BrakeSystem::masterCylinder;
}
```

### 5.3 Interactive Wizards

Commands that create complex artifacts run in interactive mode by default when required options are missing. The wizard:

1. Explains what is being created and why it matters
2. Prompts for each field with explanation and examples
3. Validates inputs and shows immediate feedback
4. Offers to show related model elements (e.g., when creating a verification case, list requirements that don't yet have verification)
5. Previews the output before writing
6. Suggests next steps after creation

Interactive mode can be disabled with `--no-interactive` for scripting and CI.

Wizard prompts are contextual. When `sv2 capa ncr create` asks for severity, it explains:

```
Severity classification determines how the NCR is handled:

  critical — Safety or regulatory impact. Requires immediate containment
             and engineering disposition. May trigger regulatory reporting.
  major    — Functional impact. Part does not meet specifications and
             cannot perform its intended function.
  minor    — Cosmetic or documentation issue. Part is functional but
             does not meet workmanship or documentation standards.
  observation — Potential issue identified. No nonconformance exists yet
                but conditions suggest a risk of future occurrence.

Select severity [critical/major/minor/observation]:
```

### 5.4 Contextual Help and Validation

```
sv2 help <topic>

Topics:
  mbse              What is MBSE and how SysML v2 supports it
  sysml-basics      Core SysML v2 constructs (parts, requirements, actions)
  requirements      How to write and manage requirements in SysML v2
  verification      Verification strategy and the V-model
  risk-management   Risk assessment methodologies (qualitative, FMEA)
  tolerance         Tolerance analysis fundamentals (worst-case, RSS, Monte Carlo)
  bom               BOM structure and configuration management
  manufacturing     Manufacturing process definition and control
  quality           Quality management system fundamentals (sampling, SPC, capability)
  capa              Nonconformance management and corrective action
  traceability      Building end-to-end traceability in your model
  git-workflow      Git collaboration patterns for model-based engineering
  getting-started   Step-by-step tutorial for your first project
  library-<name>    Detailed documentation for a specific SV2 library
```

These are not man pages. They are explanatory articles that teach systems engineering concepts in the context of the tool. A quality engineer who types `sv2 help quality` learns about sampling plans, SPC, and process capability — and how each concept maps to SysML v2 constructs and CLI commands.

**Validation messages** follow the same philosophy. Instead of:

```
Error: Reference 'Vehicle::BrakeSystem::StoppingDistance' not found
```

The tool says:

```
Warning: Reference not found
  Record:    verification/verify-20260308T143000-jhale-a1b2.toml
  Field:     refs.satisfies[0]
  Reference: Vehicle::BrakeSystem::StoppingDistance

  This qualified name does not exist in the current model index.
  Possible causes:
    - The element was renamed or moved to a different package
    - The model file containing this element was not included in model_root
    - The element has not been defined yet

  Similar names in the model:
    - Vehicle::BrakeSystem::Requirements::StoppingDistance  (requirement def)
    - Vehicle::BrakeSystem::Constraints::MaxStoppingDistance  (constraint def)

  To fix: update the reference in the TOML file, or run 'sv2 check --fix'
  to interactively resolve broken references.
```

### 5.5 Example-Driven Onboarding

The `sv2 scaffold example` command generates complete, realistic example projects that demonstrate integrated use across all domains. Each example is chosen to be representative of a real engineering context:

**`brake-system`** — Automotive subsystem. Demonstrates mechanical part hierarchies, tolerance stack-ups, FMEA-style risk assessment, verification test procedures with equipment requirements, and incoming inspection of purchased components. Moderate complexity. Good starting point.

**`medical-device`** — A regulated product (Class II medical device). Demonstrates stringent traceability requirements, design history file generation, risk management per ISO 14971, process validation per FDA guidance, and CAPA workflows. Shows how the tool supports regulatory compliance.

**`consumer-electronics`** — A multi-component electronic product. Demonstrates BOM management with multiple suppliers, prototype-to-production transition, tolerance analysis for enclosure fits, and manufacturing process control for assembly operations. Shows supply chain integration.

Each example includes:
- Complete SysML v2 model with the relevant domain libraries imported
- Populated operational records showing what tool output looks like
- A README.md explaining the project, the key workflows, and the commands to explore it
- Pipeline definitions for the project's standard workflows

---

## 6. Integration Patterns

### 6.1 SysML v2 Model as Source of Truth

The model owns all definitions. No TOML record ever duplicates information available in the model. When a record needs to reference a requirement's title, it stores the qualified name and resolves the title from the model at display time. When a risk's severity scale changes, the library is edited and all tools automatically use the new scale.

This means model changes can break records. This is intentional and correct. If a requirement is deleted, verification records that reference it should be flagged. The tool handles this gracefully through warnings, not errors, and provides repair tooling.

The one exception where the CLI writes `.sysml` files: `sv2 risk add` and `sv2 scaffold` commands generate model elements. These always write to clearly identified locations and are expected to be reviewed and edited by the user. The tool never silently modifies existing model files.

### 6.2 Qualified Name Linking

Every reference from a TOML record to a model element uses the full qualified name: `Package::SubPackage::ElementName`. Never relative names, never IDs, never file paths.

Qualified names are human-readable, stable across file reorganization (as long as the package structure is preserved), and meaningful in git diffs and code review. When a reviewer sees `verifies = "Vehicle::BrakeSystem::Requirements::MaxResponseTime"` in a TOML record, they know exactly what is being referenced without looking anything up.

The `sv2-core` crate provides the `QualifiedName` type:

```rust
pub struct QualifiedName {
    segments: Vec<String>,
}

impl QualifiedName {
    pub fn parse(s: &str) -> Result<Self>;
    pub fn is_child_of(&self, parent: &QualifiedName) -> bool;
    pub fn parent(&self) -> Option<QualifiedName>;
    pub fn leaf(&self) -> &str;
    pub fn to_path_safe(&self) -> String; // Replace :: with __ for filenames
}
```

### 6.3 Cross-Domain Traceability

The SQLite `ref_edges` table enables cross-domain queries without any tool knowing about any other tool's data structures. Every tool writes records with qualified name references. The index normalizes these into a uniform edge table. Cross-domain queries are just joins:

"Show me every requirement that has a verified test, a risk assessment, and an inspection plan":

```sql
SELECT DISTINCT r.qn
FROM ref_edges r
WHERE r.ref_type = 'satisfies' AND r.qn IN (
    SELECT qn FROM ref_edges WHERE ref_type = 'risk_affects'
) AND r.qn IN (
    SELECT qn FROM ref_edges WHERE ref_type = 'inspects'
)
```

The tool exposes these queries through `sv2 report traceability` and `sv2 check --gate`, but power users can also query the cache directly with any SQLite client.

### 6.4 Pipeline Workflows

Pipelines compose subcommands into repeatable workflows. They are defined in config, not in code, so they can be customized per project without rebuilding the tool.

Pipeline parameters use `{name}` substitution. A step's output can feed the next step's input through environment variables: each step can set `SV2_LAST_RECORD_ID`, `SV2_LAST_LOT_ID`, etc., which subsequent steps consume.

Pipeline execution is transparent: every command executed is displayed, every output is shown, every failure pauses for user decision. Pipelines are guidance that the user controls, not automation that takes over.

---

## 7. Data Architecture

### 7.1 SQLite Cache Schema

```sql
-- Model elements indexed from .sysml files
CREATE TABLE nodes (
    qn TEXT PRIMARY KEY,
    element_type TEXT NOT NULL,
    file TEXT NOT NULL,
    line INTEGER NOT NULL,
    parent_qn TEXT,
    attributes TEXT,             -- JSON: key attributes extracted at index time
    FOREIGN KEY (parent_qn) REFERENCES nodes(qn)
);

-- Model relationships indexed from .sysml files
CREATE TABLE edges (
    source_qn TEXT NOT NULL,
    target_qn TEXT NOT NULL,
    edge_type TEXT NOT NULL,     -- specialization, composition, satisfy, verify, connect, etc.
    file TEXT NOT NULL,
    line INTEGER NOT NULL,
    PRIMARY KEY (source_qn, target_qn, edge_type),
    FOREIGN KEY (source_qn) REFERENCES nodes(qn),
    FOREIGN KEY (target_qn) REFERENCES nodes(qn)
);

-- Operational records indexed from .toml files
CREATE TABLE records (
    id TEXT PRIMARY KEY,
    tool TEXT NOT NULL,
    record_type TEXT NOT NULL,   -- execution, assessment, analysis, entity, etc.
    file TEXT NOT NULL,
    author TEXT,
    created TEXT,
    modified TEXT,
    status TEXT
);

-- References from records to model nodes
CREATE TABLE ref_edges (
    record_id TEXT NOT NULL,
    qn TEXT NOT NULL,
    ref_type TEXT NOT NULL,      -- verifies, satisfies, affects, inspects, etc.
    PRIMARY KEY (record_id, qn, ref_type),
    FOREIGN KEY (record_id) REFERENCES records(id)
);

-- Cache metadata
CREATE TABLE cache_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: git_head, build_timestamp, sv2_version, schema_version

-- Performance indexes
CREATE INDEX idx_nodes_parent ON nodes(parent_qn);
CREATE INDEX idx_nodes_type ON nodes(element_type);
CREATE INDEX idx_edges_target ON edges(target_qn);
CREATE INDEX idx_edges_type ON edges(edge_type);
CREATE INDEX idx_ref_edges_qn ON ref_edges(qn);
CREATE INDEX idx_ref_edges_type ON ref_edges(ref_type);
CREATE INDEX idx_records_tool ON records(tool);
CREATE INDEX idx_records_status ON records(status);
```

### 7.2 TOML Record Conventions

**Common envelope** (all records):

```toml
[meta]
id = "verify-20260308T143000-jhale-a1b2"
tool = "verify"
record_type = "execution"
schema_version = "1.0"
created = 2026-03-08T14:30:00Z
modified = 2026-03-08T15:45:00Z
author = "jhale"

[refs]
# All model references here — this section is indexed
```

**Serialization rules enforced by sv2-core:**
- Maps: `BTreeMap` always (deterministic ordering)
- Dates: RFC 3339
- Qualified names: full paths, always
- Enumerations: lowercase with underscores, matching SysML library enum values
- Units in key names: `_mm`, `_n`, `_ms`, `_c`, `_pct` suffixes when not obvious
- Arrays: sorted alphabetically when order is not semantically meaningful
- Booleans: never use strings "true"/"false", always native TOML booleans
- Nulls: omit the key entirely rather than using empty strings

### 7.3 Git Collaboration Design

**Append-only records** (executions, inspections, lot steps):
- One file per event
- Filename encodes timestamp + author + hash: never collides
- Git sees: file additions only, never modifications
- Merge conflicts: impossible

**Entity records** (risks, NCRs, corrective actions):
- One file per entity
- Filename encodes entity ID: `risk-RSK-0042.toml`
- Git sees: occasional modifications to existing files
- Merge conflicts: rare because TOML's flat structure means changes to different fields appear on different lines
- Mitigation: `BTreeMap` ordering prevents key-reorder diffs

**Model files** (`.sysml`):
- Standard code review practices apply
- The tree-sitter grammar enables structural diffing in the future (not just textual)
- Library files should rarely change; model files change with the design

**Gitignore entries** (created by `sv2 init`):

```gitignore
.sv2/cache.db
.sv2/cache.db-wal
.sv2/cache.db-shm
```

### 7.4 Reference Integrity

References break when the model changes. The tool's integrity model:

**Soft references:** All qualified name references from TOML records to model nodes are soft. A broken reference is a warning, not an error. The record remains valid and readable; it just has an unresolved link.

**Resolution assistance:** `sv2 check --fix` uses Levenshtein distance and prefix matching against the current model index to suggest likely matches for broken references. The user confirms each fix.

**Bulk rename support:** `sv2 check --rename <old-qn> <new-qn>` updates all TOML records that reference the old qualified name. This is run after refactoring the SysML model to keep records in sync.

**Staleness detection:** Cache stores git HEAD hash. Any command detects staleness and rebuilds transparently. No user action needed.

---

## 8. Rust Workspace Structure

```
sv2/
├── Cargo.toml                        # workspace definition
├── crates/
│   ├── sv2-core/                     # shared foundation
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── qualified_name.rs     # QualifiedName type and parsing
│   │   │   ├── cache.rs              # SQLite cache interface
│   │   │   ├── index.rs              # model indexer (tree-sitter integration)
│   │   │   ├── record.rs             # TOML record envelope types
│   │   │   ├── reference.rs          # Reference<T> lazy resolution
│   │   │   ├── config.rs             # config file parsing
│   │   │   ├── project.rs            # project discovery
│   │   │   ├── output.rs             # output format handling
│   │   │   ├── interactive.rs        # interactive prompt utilities
│   │   │   ├── cli.rs                # shared CLI argument definitions
│   │   │   └── report.rs             # shared report generation
│   │   └── Cargo.toml
│   ├── sv2-verify/                   # verification subcommand
│   ├── sv2-risk/                     # risk subcommand
│   ├── sv2-tol/                      # tolerance subcommand
│   ├── sv2-bom/                      # bom subcommand
│   ├── sv2-source/                   # sourcing subcommand
│   ├── sv2-mfg/                      # manufacturing subcommand
│   ├── sv2-qc/                       # quality control subcommand
│   ├── sv2-capa/                     # capa subcommand
│   ├── sv2-report/                   # cross-domain reporting
│   ├── sv2-pipeline/                 # pipeline execution
│   └── sv2-scaffold/                 # scaffolding and templates
├── src/
│   └── main.rs                       # top-level binary: subcommand dispatch
├── libraries/                        # SysML v2 domain library source files
│   ├── sv2-risk.sysml
│   ├── sv2-tolerance.sysml
│   ├── sv2-bom.sysml
│   ├── sv2-sourcing.sysml
│   ├── sv2-manufacturing.sysml
│   ├── sv2-quality.sysml
│   ├── sv2-capa.sysml
│   ├── sv2-verification-ext.sysml
│   └── sv2-project.sysml
├── examples/                         # example projects
│   ├── brake-system/
│   ├── medical-device/
│   └── consumer-electronics/
├── tests/
│   ├── integration/                  # cross-tool integration tests
│   ├── fixtures/                     # test model and record files
│   └── git-simulation/              # merge conflict testing
└── docs/
    ├── help/                         # sv2 help topic content
    └── library-docs/                 # library documentation
```

**Dependency rule:** Every subcommand crate depends on `sv2-core`. No subcommand crate depends on any other subcommand crate. This is enforced by CI — a build step verifies the dependency graph.

**Shared infrastructure in sv2-core:**
- `QualifiedName` — parsing, comparison, prefix matching, path-safe conversion
- `Cache` — SQLite connection management, query builders, staleness detection, rebuild orchestration
- `RecordEnvelope` — common `[meta]` and `[refs]` serialization/deserialization
- `Reference<T>` — lazy-resolving wrapper around a qualified name string
- `InteractivePrompt` — colored prompts, input validation, wizard step sequencing
- `OutputFormatter` — text, JSON, TOML, CSV, HTML rendering with consistent styling
- `ProjectConfig` — config file parsing with environment variable override
- `ProjectDiscovery` — walk-up `.sv2/` detection
- `CliArgs` — clap argument definitions shared across all subcommands

---

## 9. Testing and Quality Strategy

**Unit tests** (per crate): Every type, parser, formatter, and computation has unit tests. The `sv2-core` crate has the heaviest unit test load: qualified name parsing edge cases, cache query correctness, TOML serialization round-trips, reference resolution behavior with missing nodes.

**Integration tests** (per subcommand): Each subcommand crate has integration tests that operate against fixture `.sysml` model files and pre-built TOML records in the `tests/fixtures/` directory. Tests invoke the subcommand's entry point function (not the binary) with constructed arguments and verify the output and any written files. These tests run fast and in isolation — no other subcommand needs to be present.

**Cross-tool integration tests** (workspace level): A test suite in `tests/integration/` exercises multi-tool workflows by invoking the actual compiled binary as a subprocess. Test scenarios:
- Scaffold a project, index it, run `sv2 check`, verify clean status
- Execute a verification run, verify the record is written and indexed correctly
- Create an NCR referencing a verification failure, verify cross-domain link resolves
- Rename a model element, run `sv2 check`, verify broken references are reported
- Run `sv2 check --fix`, verify references are repaired
- Execute a pipeline, verify all steps complete and records are created

**Git collaboration tests** (workspace level): A test harness in `tests/git-simulation/` creates a temporary git repository, makes parallel changes on branches (simulating two engineers creating records simultaneously), merges, and verifies:
- No merge conflicts in any TOML files
- Cache rebuilds correctly from merged state
- All records are intact and all references resolve

**Library validation tests**: Parse each `.sysml` library with `sv2 index` and verify that every expected definition exists with the correct element type and attributes. This catches regressions when libraries are updated for SysML v2 spec changes.

**Property-based tests**: Using `proptest` or `quickcheck`, generate random model mutations (rename, move, delete elements) and verify that `sv2 check` correctly identifies every broken reference without panicking. Generate random TOML records with valid and invalid qualified names and verify that the indexer handles all cases.

**Continuous integration**: Every PR runs unit tests, per-subcommand integration tests, and cross-tool integration tests. Library validation and git simulation tests run nightly. The dependency graph check runs on every PR.

---

## 10. Development Roadmap

**Phase 1 — Foundation and Verification (Months 1–3)**

Deliver: `sv2-core`, `sv2 init`, `sv2 index`, `sv2 check`, `sv2 scaffold`, `sv2 verify`, `sv2 help`, and the verification extensions library.

This phase validates the entire architecture: model parsing, cache management, qualified name resolution, TOML record handling, interactive execution, and coverage analysis. Verification is the highest-value subcommand and the most direct port from Tessera's existing domain knowledge.

Milestone: A user can scaffold an example project, write SysML v2 requirements and verification cases, execute tests interactively, and see coverage reports.

**Phase 2 — Risk and Tolerance (Months 3–5)**

Deliver: `sv2 risk`, `sv2 tol`, risk library, tolerance library.

These two domains have the richest library content. Building them validates that the library design works when tools consume complex type hierarchies. Tolerance analysis exercises numerical computation and data import. Risk exercises the pattern where the tool writes both model elements and records.

Milestone: A user can define risks in SysML v2, assess them through the CLI, generate FMEA worksheets, define tolerance chains, and run Monte Carlo analysis.

**Phase 3 — BOM and Supply Chain (Months 5–7)**

Deliver: `sv2 bom`, `sv2 source`, BOM library, sourcing library.

BOM rollup exercises recursive model traversal with multiplicity handling, which is architecturally distinct from anything in earlier phases. Sourcing is the first purely operational subcommand (minimal library content, heavy record management).

Milestone: A user can generate indented BOMs, compare BOMs across git revisions, track suppliers and quotes, and generate RFQ packages.

**Phase 4 — Manufacturing and Quality (Months 7–10)**

Deliver: `sv2 mfg`, `sv2 qc`, manufacturing library, quality library.

These are delivered together because lot execution and inspection are tightly coupled. Manufacturing exercises the guided walkthrough pattern (similar to verification but with process parameter recording and SPC). Quality exercises sampling plan mathematics and data import.

Milestone: A user can define manufacturing routings in SysML v2, execute lot travelers interactively, record process parameters, perform inspections with sampling plans, and generate SPC charts.

**Phase 5 — CAPA and Reporting (Months 10–12)**

Deliver: `sv2 capa`, `sv2 report`, `sv2 pipeline`, CAPA library, project library.

CAPA is last because it references records from every other domain. Reporting and pipelines tie everything together. The project library and gate checking validate end-to-end integration.

Milestone: A user can manage NCRs and corrective actions with full cross-domain traceability, generate design history files and certificates of conformance, define and execute workflow pipelines, and run design review gate checks.

---

## 11. Appendix: Complete SysML v2 Library Source Sketches

The following are structural sketches showing the shape of each library. These are not syntactically complete SysML v2 — they show the definitions, attributes, and relationships that each library must provide. Implementation requires alignment with the final SysML v2 grammar as supported by the tree-sitter parser.

### Risk Library Sketch

```sysml
package SV2Risk {
    import ISQ::*;
    import SI::*;

    enum def SeverityLevel {
        negligible;    // numericValue = 1
        marginal;      // numericValue = 2
        moderate;      // numericValue = 3
        critical;      // numericValue = 4
        catastrophic;  // numericValue = 5
    }

    enum def LikelihoodLevel {
        improbable;    // numericValue = 1
        remote;        // numericValue = 2
        occasional;    // numericValue = 3
        probable;      // numericValue = 4
        frequent;      // numericValue = 5
    }

    enum def DetectabilityLevel {
        almostCertain;     // numericValue = 1 (low risk)
        high;              // numericValue = 2
        moderate;          // numericValue = 3
        low;               // numericValue = 4
        almostImpossible;  // numericValue = 5 (high risk)
    }

    enum def RiskCategory {
        technical; schedule; cost; safety;
        regulatory; supplyChain; environmental;
    }

    enum def RiskStatus {
        identified; analyzing; mitigating;
        monitoring; closed; accepted;
    }

    enum def MitigationStrategy {
        avoid; transfer; reduce; accept; contingency;
    }

    enum def MitigationStatus {
        planned; inProgress; implemented; verified; ineffective;
    }

    part def RiskDef {
        attribute id : String;
        attribute title : String;
        attribute category : RiskCategory;
        attribute status : RiskStatus;
        attribute severity : SeverityLevel;
        attribute likelihood : LikelihoodLevel;
        attribute detectability : DetectabilityLevel;
        attribute riskPriorityNumber : Integer;
        attribute identifiedDate : String; // ISO 8601
        attribute owner : String;

        constraint rpnCalculation {
            riskPriorityNumber == severity.numericValue
                * likelihood.numericValue
                * detectability.numericValue
        }
    }

    action def MitigationDef {
        attribute strategy : MitigationStrategy;
        attribute effectivenessTarget : Real; // percentage
        attribute owner : String;
        attribute dueDate : String;
        attribute status : MitigationStatus;
    }

    connection def mitigates {
        end risk : RiskDef;
        end mitigation : MitigationDef;
        attribute expectedReduction : Real;
    }

    connection def affects {
        end risk : RiskDef;
        end element : Anything;
    }

    part def RiskMatrixDef {
        attribute severityLabels : String[*]; // ordered axis labels
        attribute likelihoodLabels : String[*];
        // Cell classifications defined as nested structure
    }
}
```

### Tolerance Library Sketch

```sysml
package SV2Tolerance {
    import ISQ::*;
    import SI::*;
    import NumericalFunctions::*;

    enum def DistributionType {
        normal; uniform; triangular;
        skewedLeft; skewedRight; beta;
    }

    enum def StackDirection {
        linear; radial; angular;
    }

    enum def AnalysisMethod {
        worstCase; rss; monteCarlo; modifiedRSS;
    }

    enum def GeometricCharacteristic {
        straightness; flatness; circularity; cylindricity;
        lineProfile; surfaceProfile;
        angularity; perpendicularity; parallelism;
        position; concentricity; symmetry;
        circularRunout; totalRunout;
    }

    enum def MaterialCondition {
        regardlessOfFeatureSize; maximumMaterial; leastMaterial;
    }

    attribute def ToleranceDef {
        attribute nominal : ISQ::LengthValue;
        attribute upperLimit : ISQ::LengthValue;
        attribute lowerLimit : ISQ::LengthValue;
        attribute distributionType : DistributionType;
        attribute processCapabilityCp : Real;
        attribute processCapabilityCpk : Real;
        attribute isCritical : Boolean;
    }

    part def DimensionChainDef {
        attribute closingDimension : String;
        attribute stackDirection : StackDirection;
        attribute analysisMethod : AnalysisMethod;
        attribute targetCpk : Real;
        ref contributors : ToleranceDef[1..*] ordered;
    }

    attribute def DatumDef {
        attribute label : String;
        attribute materialCondition : MaterialCondition;
    }

    attribute def FeatureControlFrameDef {
        attribute characteristic : GeometricCharacteristic;
        attribute toleranceZone : ISQ::LengthValue;
        attribute materialCondition : MaterialCondition;
        ref datumReferences : DatumDef[0..3] ordered;
    }
}
```

### Manufacturing Library Sketch

```sysml
package SV2Manufacturing {
    import ISQ::*;
    import SI::*;

    enum def ProcessType {
        machining; welding; brazing; soldering;
        adhesiveBonding; molding; casting; forging;
        stamping; sheetMetal; heatTreat; surfaceTreatment;
        coating; assembly; testAndInspection; packaging;
        cleaning; printing3d; programming; calibration;
    }

    enum def MonitoringMethod {
        continuous; periodic; perUnit; perLot; perSetup;
    }

    enum def SPCRule {
        westernElectric; nelsonRules; customRule;
    }

    enum def InspectionType {
        dimensional; visual; functional;
        destructive; nonDestructive;
    }

    enum def SamplingRate {
        everyUnit; perLot; firstArticle; periodic;
    }

    enum def GateType {
        mandatory; advisory;
    }

    enum def DeviationType {
        parameterExcursion; toolingSubstitution;
        sequenceChange; materialSubstitution; operatorOverride;
    }

    action def ProcessDef {
        attribute processType : ProcessType;
        attribute workCenter : String;
        attribute setupTimeMinutes : Real;
        attribute cycleTimeMinutes : Real;
        attribute requiredTooling : String[*];
        attribute requiredFixtures : String[*];
        attribute safetyRequirements : String[*];
    }

    attribute def ProcessParameterDef {
        attribute parameterName : String;
        attribute nominal : Real;
        attribute upperControlLimit : Real;
        attribute lowerControlLimit : Real;
        attribute upperSpecLimit : Real;
        attribute lowerSpecLimit : Real;
        attribute units : String;
        attribute monitoringMethod : MonitoringMethod;
        attribute spcRule : SPCRule;
    }

    action def WorkInstructionDef {
        attribute stepNumber : Integer;
        // doc comment carries the instruction text
        attribute safetyWarning : String[0..1];
        attribute qualityCheckpoint : Boolean;
        attribute estimatedTimeMinutes : Real;
    }

    part def InspectionPointDef {
        attribute inspectionType : InspectionType;
        attribute samplingRate : SamplingRate;
        attribute gateType : GateType;
    }

    action def RoutingDef {
        // Ordered succession of ProcessDef usages
        attribute revision : String;
        attribute effectiveDate : String;
    }
}
```

---

*This document is a living architecture specification. As the SysML v2 specification evolves and user feedback shapes priorities, the library definitions, CLI interfaces, and integration patterns will be refined. The core principles — SysML v2 as code, model as source of truth, guided user experience, git-native collaboration — are stable foundations that the implementation builds on.*

