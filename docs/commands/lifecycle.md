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

### verify add

Add a verification case to a model file using an interactive wizard.

```sh
sysml verify add --file model.sysml
sysml verify add                        # prints to stdout
```

### verify run

Execute a verification case interactively, recording results step-by-step. Writes a TOML execution record to `.sysml/records/`.

```sh
sysml verify run model.sysml
sysml verify run model.sysml --case TestBrakes
sysml verify run model.sysml --author "Jane Smith"
```

| Option | Description |
|--------|-------------|
| `--case <NAME>` | Verification case to run (prompted if omitted) |
| `--author <NAME>` | Author name for the execution record (default: `engineer`) |

## risk

Hazard analysis (MIL-STD-882E / ISO 14971) and FMEA (AIAG/VDA, SAE J1739).

Risks are part definitions specializing `RiskDef`, nested inside the part, action, or use case they apply to. Scores use 1–5 integer scales for Severity, Occurrence, and Detection.

### risk list

List risks with S/O/D scores, RPN, acceptance level, and entity assignment.

```sh
sysml risk list model.sysml
sysml risk list -f json model.sysml
```

### risk matrix

Generate a 5×5 risk matrix (severity vs. occurrence) with acceptance zones.

```sh
sysml risk matrix model.sysml
```

Zones: `!!` = Unacceptable, `!` = Undesirable, `?` = Review, blank = Acceptable.

### risk fmea

Generate an FMEA worksheet with standard columns: Item, Failure Mode, Effect, Cause, S, O, D, RPN, Risk Level, Recommended Action, Status, Assigned To.

```sh
sysml risk fmea model.sysml
sysml risk fmea -f json model.sysml
```

### risk coverage

Show which parts, actions, and use cases have risks assigned vs. uncovered.

```sh
sysml risk coverage model.sysml
```

### risk add

Add a risk using an interactive FMEA wizard (failure mode, effect, cause, S/O/D scores). Prompts for target file and parent definition if not specified.

```sh
sysml risk add
sysml risk add --file model.sysml --inside Enclosure
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

### tol add

Add a tolerance dimension chain to a model file using an interactive wizard.

```sh
sysml tol add --file model.sysml
sysml tol add                           # prints to stdout
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

### bom cost

Show a costed BOM by resolving supplier quotes at a given production quantity. Reads quote records from `.sysml/records/` and matches them to parts in the BOM tree using quantity-based price breaks.

```sh
sysml bom cost model.sysml --root Vehicle --quantity 100
sysml bom cost model.sysml --root Vehicle --quantity 500 --apply
sysml bom cost -f json model.sysml --root Vehicle --quantity 100
```

| Option | Description |
|--------|-------------|
| `--root <DEF>` | Root part definition name (required) |
| `--quantity <N>` | Production order quantity (required) |
| `--apply` | Write resolved `unitCost` attributes back into part definitions |

The `--apply` flag inserts `attribute unitCost = <price>;` into each part definition that has a matching quote. This lets you snapshot pricing into the model for a specific production quantity.

## source

Supplier management: approved source lists, quotes, and RFQ generation.

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

### source quote

Record a supplier quote with quantity-based price breaks using an interactive wizard. Writes a TOML record to `.sysml/records/`.

```sh
sysml source quote
sysml source quote --author "Jane Smith"
```

| Option | Description |
|--------|-------------|
| `--author <NAME>` | Author name for the record (default: `engineer`) |

The wizard prompts for part name, supplier, currency, lead time, MOQ, and one or more price breaks (minimum quantity + unit price at that tier). Quote records are used by `sysml bom cost` to compute costed BOMs.

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

### mfg start-lot

Start a new production lot for a manufacturing routing. Creates a lot record with a unique ID and writes it to `.sysml/records/`.

```sh
sysml mfg start-lot model.sysml
sysml mfg start-lot model.sysml --routing AssembleWidget --quantity 100
sysml mfg start-lot model.sysml --lot-type prototype --quantity 5
```

| Option | Description |
|--------|-------------|
| `--routing <NAME>` | Routing name (prompted if omitted) |
| `--quantity <N>` | Lot quantity (default: 1) |
| `--lot-type <TYPE>` | Lot type: `production`, `prototype`, `first-article` (default: `production`) |
| `--author <NAME>` | Author name for the record (default: `engineer`) |

### mfg step

Execute the next step of an active lot interactively. Prompts for parameter readings, validates against control limits, and advances the lot.

```sh
sysml mfg step LOT-ID
sysml mfg step mfg-lot-abc123
```

| Option | Description |
|--------|-------------|
| `--author <NAME>` | Author name for the record (default: `engineer`) |

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

### quality create

Create a quality item (NCR, CAPA, or Process Deviation) using an interactive wizard. Writes a TOML record to `.sysml/records/`.

```sh
sysml quality create                    # wizard asks which type
sysml quality create --type ncr         # skip type selection
sysml quality create --type capa
sysml quality create --type deviation
```

| Option | Description |
|--------|-------------|
| `--type <TYPE>` | Quality item type: `ncr`, `capa`, `deviation` |

### quality rca

Perform root cause analysis on an NCR or CAPA using a structured interactive methodology. Supports 5 Why (5 successive "why" questions) and Fishbone/Ishikawa (6 cause categories). Writes a TOML record to `.sysml/records/`.

```sh
sysml quality rca                                          # prompted for all inputs
sysml quality rca --source NCR-001 --method five-why       # skip prompts
sysml quality rca --source CAPA-003 --method fishbone
```

| Option | Description |
|--------|-------------|
| `--source <ID>` | Source item ID (NCR or CAPA) to analyze |
| `--method <METHOD>` | RCA method: `five-why`, `fishbone` |

### quality action

Add a corrective or preventive action to an existing CAPA using an interactive wizard. Records the action type, description, owner, due date, and optional verification reference.

```sh
sysml quality action                           # prompted for CAPA ID
sysml quality action --capa CAPA-001           # skip CAPA ID prompt
```

| Option | Description |
|--------|-------------|
| `--capa <ID>` | CAPA ID to add the action to |
