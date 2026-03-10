# Lifecycle Commands

Domain commands for verification, risk, tolerance, BOM, supply chain, manufacturing, quality control, and quality management (NCR/CAPA/deviation). These commands operate on SysML v2 models using the [domain libraries](../domain-libraries.md).

## verify

Verification case management and coverage tracking.

### verify list

List verification cases found in model files.

```sh
sysml verify list model.sysml
```

### verify coverage

Show verification coverage — which requirements have verification cases and execution results.

```sh
sysml verify coverage model.sysml
sysml verify coverage -f json model.sysml
```

### verify status

Show verification status for all requirements.

```sh
sysml verify status model.sysml
```

## risk

Risk management: identification, assessment, matrix visualization, and FMEA.

### risk list

List risks found in model files (parts specializing `RiskDef`).

```sh
sysml risk list model.sysml
sysml risk list -f json model.sysml
```

### risk matrix

Generate a 5x5 risk matrix (severity vs. likelihood) from risks in the model.

```sh
sysml risk matrix model.sysml
sysml risk matrix -f json model.sysml
```

### risk fmea

Generate an FMEA (Failure Mode and Effects Analysis) worksheet.

```sh
sysml risk fmea model.sysml
```

## tol

Tolerance stack-up analysis: worst-case, RSS, and Monte Carlo methods.

### tol analyze

Run tolerance stack-up analysis on dimension chains in the model.

```sh
sysml tol analyze model.sysml                           # Worst-case (default)
sysml tol analyze model.sysml --method rss              # Root sum of squares
sysml tol analyze model.sysml --method monte-carlo      # Monte Carlo
sysml tol analyze model.sysml --method mc --iterations 50000
```

| Option | Description |
|--------|-------------|
| `--method <METHOD>` | Analysis method: `worst-case`/`wc`, `rss`, `monte-carlo`/`mc` (default: `worst-case`) |
| `--iterations <N>` | Monte Carlo iterations (default: 10000) |

### tol sensitivity

Rank tolerance contributors by their impact on overall variation.

```sh
sysml tol sensitivity model.sysml
```

## bom

Bill of materials: hierarchical rollup, where-used analysis, and export.

### bom rollup

Build a hierarchical BOM tree from the model's composition hierarchy. Optionally includes mass and cost rollup through assembly levels.

```sh
sysml bom rollup model.sysml --root Vehicle
sysml bom rollup model.sysml --root Vehicle --include-mass --include-cost
sysml bom rollup -f json model.sysml --root Vehicle
```

| Option | Description |
|--------|-------------|
| `--root <DEF>` | Root part definition name (required) |
| `--include-mass` | Include mass rollup in output |
| `--include-cost` | Include cost rollup in output |

SysML v2 multiplicity is the BOM quantity — `part brakePad : BrakePadDef[4]` means 4 units.

### bom where-used

Reverse lookup: find all assemblies that contain a given part.

```sh
sysml bom where-used model.sysml --part BrakePad
```

| Option | Description |
|--------|-------------|
| `--part <PART>` | Part definition name to search (required) |

### bom export

Export a flattened BOM as CSV for ERP/MRP import.

```sh
sysml bom export model.sysml --root Vehicle
sysml bom export model.sysml --root Vehicle --format csv
```

| Option | Description |
|--------|-------------|
| `--root <DEF>` | Root part definition name (required) |
| `--format <FORMAT>` | Output format (default: `csv`) |

## source

Supplier management: approved source lists, RFQ generation.

### source list

List suppliers found in model files (parts specializing `SupplierDef`).

```sh
sysml source list model.sysml
```

### source asl

Show the approved source list — suppliers with `approved` or `preferred` qualification status.

```sh
sysml source asl model.sysml
```

### source rfq

Generate a request-for-quotation document for a part.

```sh
sysml source rfq --part BrakePad --quantity 1000 --description "Ceramic brake pad"
```

| Option | Description |
|--------|-------------|
| `--part <PART>` | Part name (required) |
| `--quantity <N>` | Required quantity (default: 1) |
| `--description <TEXT>` | Part description |

## mfg

Manufacturing: routing discovery and statistical process control.

### mfg list

List manufacturing routings (action definitions) found in model files.

```sh
sysml mfg list model.sysml
```

### mfg spc

Compute SPC (Statistical Process Control) statistics for a process parameter.

```sh
sysml mfg spc --parameter Temperature --values "150.1,149.8,150.3,150.0,149.9"
```

| Option | Description |
|--------|-------------|
| `--parameter <NAME>` | Parameter name (required) |
| `--values <VALUES>` | Comma-separated measurement values (required) |

Output includes mean, standard deviation, UCL/LCL (control limits at 3 sigma), and out-of-control flags.

## qc

Quality control: sampling plans and process capability.

### qc sample-size

Look up ANSI Z1.4 / ISO 2859 sample size for a given lot size and AQL.

```sh
sysml qc sample-size --lot-size 500
sysml qc sample-size --lot-size 500 --aql 0.65 --level tightened
```

| Option | Description |
|--------|-------------|
| `--lot-size <N>` | Lot size (required) |
| `--aql <PCT>` | Acceptable quality level (default: 1.0) |
| `--level <LEVEL>` | Inspection level: `reduced`, `normal`, `tightened` (default: `normal`) |

### qc capability

Compute process capability indices (Cp, Cpk) from measurement data.

```sh
sysml qc capability --usl 10.05 --lsl 9.95 --values "10.01,9.99,10.02,9.98,10.00"
```

| Option | Description |
|--------|-------------|
| `--usl <VAL>` | Upper specification limit (required) |
| `--lsl <VAL>` | Lower specification limit (required) |
| `--values <VALUES>` | Comma-separated measurement values (required) |

Output includes Cp, Cpk, mean, standard deviation, and a capability assessment.

## quality

Quality management: NCR, CAPA, and Process Deviation — three distinct quality item types.

- **NCR** (Nonconformance Report): documents observed nonconformances
- **CAPA** (Corrective/Preventive Action): formal action programs to address root causes
- **Process Deviation**: planned, approved departures from standard processes

### quality list

Show quality item types, their lifecycles, and workflow guidance.

```sh
sysml quality list
```

### quality trend

Analyze NCR trends grouped by category, severity, part, or supplier.

```sh
sysml quality trend --group-by category
sysml quality trend --group-by severity model.sysml
```

| Option | Description |
|--------|-------------|
| `--group-by <DIM>` | Grouping dimension: `category`, `severity`, `part`, `supplier` (default: `category`) |
