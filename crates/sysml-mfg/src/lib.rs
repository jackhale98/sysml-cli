//! Manufacturing execution domain for SysML v2 models.
//!
//! Provides types and functions for mapping SysML action definitions to
//! manufacturing routings, tracking production lots through process steps,
//! recording parameter readings, and computing SPC statistics.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use sysml_core::model::{DefKind, Model};
use sysml_core::record::{
    generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue,
};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Classification of manufacturing process types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessType {
    Machining,
    Welding,
    Brazing,
    Soldering,
    AdhesiveBonding,
    Molding,
    Casting,
    Forging,
    Stamping,
    SheetMetal,
    HeatTreat,
    SurfaceTreatment,
    Coating,
    Assembly,
    TestAndInspection,
    Packaging,
    Cleaning,
    Printing3d,
    Programming,
    Calibration,
}

impl ProcessType {
    /// Return a human-readable label for display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Machining => "Machining",
            Self::Welding => "Welding",
            Self::Brazing => "Brazing",
            Self::Soldering => "Soldering",
            Self::AdhesiveBonding => "Adhesive Bonding",
            Self::Molding => "Molding",
            Self::Casting => "Casting",
            Self::Forging => "Forging",
            Self::Stamping => "Stamping",
            Self::SheetMetal => "Sheet Metal",
            Self::HeatTreat => "Heat Treat",
            Self::SurfaceTreatment => "Surface Treatment",
            Self::Coating => "Coating",
            Self::Assembly => "Assembly",
            Self::TestAndInspection => "Test & Inspection",
            Self::Packaging => "Packaging",
            Self::Cleaning => "Cleaning",
            Self::Printing3d => "3D Printing",
            Self::Programming => "Programming",
            Self::Calibration => "Calibration",
        }
    }

    /// Attempt to infer a process type from a step name using keyword matching.
    fn infer_from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("machin") || lower.contains("mill") || lower.contains("drill") || lower.contains("turn") || lower.contains("cnc") {
            Self::Machining
        } else if lower.contains("weld") {
            Self::Welding
        } else if lower.contains("braz") {
            Self::Brazing
        } else if lower.contains("solder") {
            Self::Soldering
        } else if lower.contains("adhesive") || lower.contains("bond") || lower.contains("glue") {
            Self::AdhesiveBonding
        } else if lower.contains("mold") || lower.contains("inject") {
            Self::Molding
        } else if lower.contains("cast") {
            Self::Casting
        } else if lower.contains("forg") {
            Self::Forging
        } else if lower.contains("stamp") || lower.contains("press") {
            Self::Stamping
        } else if lower.contains("sheet") || lower.contains("bend") || lower.contains("shear") {
            Self::SheetMetal
        } else if lower.contains("heat") || lower.contains("anneal") || lower.contains("temper") || lower.contains("quench") {
            Self::HeatTreat
        } else if lower.contains("coat") || lower.contains("paint") || lower.contains("plate") || lower.contains("anodize") {
            Self::Coating
        } else if lower.contains("surface") || lower.contains("finish") || lower.contains("polish") || lower.contains("grind") {
            Self::SurfaceTreatment
        } else if lower.contains("assembl") || lower.contains("install") || lower.contains("mount") {
            Self::Assembly
        } else if lower.contains("test") || lower.contains("inspect") || lower.contains("measure") || lower.contains("check") || lower.contains("verify") {
            Self::TestAndInspection
        } else if lower.contains("packag") || lower.contains("pack") || lower.contains("crate") || lower.contains("ship") {
            Self::Packaging
        } else if lower.contains("clean") || lower.contains("wash") || lower.contains("degrease") {
            Self::Cleaning
        } else if lower.contains("print") || lower.contains("3d") || lower.contains("additive") {
            Self::Printing3d
        } else if lower.contains("program") || lower.contains("flash") || lower.contains("firmware") {
            Self::Programming
        } else if lower.contains("calibrat") {
            Self::Calibration
        } else {
            Self::Assembly
        }
    }
}

impl fmt::Display for ProcessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Classification of a production lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LotType {
    Production,
    Prototype,
    FirstArticle,
}

impl fmt::Display for LotType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Production => f.write_str("Production"),
            Self::Prototype => f.write_str("Prototype"),
            Self::FirstArticle => f.write_str("First Article"),
        }
    }
}

/// Status of a production lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LotStatus {
    Created,
    InProgress,
    OnHold,
    Completed,
    Scrapped,
}

impl fmt::Display for LotStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => f.write_str("Created"),
            Self::InProgress => f.write_str("In Progress"),
            Self::OnHold => f.write_str("On Hold"),
            Self::Completed => f.write_str("Completed"),
            Self::Scrapped => f.write_str("Scrapped"),
        }
    }
}

/// Status of an individual process step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    Passed,
    Failed,
    Skipped,
    Deviated,
}

impl fmt::Display for StepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("Pending"),
            Self::InProgress => f.write_str("In Progress"),
            Self::Passed => f.write_str("Passed"),
            Self::Failed => f.write_str("Failed"),
            Self::Skipped => f.write_str("Skipped"),
            Self::Deviated => f.write_str("Deviated"),
        }
    }
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A measurable parameter within a process step with control and spec limits.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessParameter {
    pub name: String,
    /// Nominal (target) value.
    pub nominal: f64,
    /// Upper control limit.
    pub ucl: f64,
    /// Lower control limit.
    pub lcl: f64,
    /// Upper specification limit.
    pub usl: f64,
    /// Lower specification limit.
    pub lsl: f64,
    /// Engineering unit (e.g. "mm", "degC", "N").
    pub unit: String,
}

/// A single step in a manufacturing routing.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessStep {
    /// 1-based step number.
    pub number: usize,
    pub name: String,
    pub process_type: ProcessType,
    pub description: String,
    pub parameters: Vec<ProcessParameter>,
    pub inspection_required: bool,
    pub status: StepStatus,
}

/// A production lot tracking a batch of parts through a routing.
#[derive(Debug, Clone, Serialize)]
pub struct Lot {
    pub id: String,
    pub routing_name: String,
    pub quantity: u32,
    pub lot_type: LotType,
    pub status: LotStatus,
    pub steps: Vec<ProcessStep>,
    /// Index into `steps` for the current active step (0-based).
    pub current_step: usize,
}

/// A single parameter measurement taken during production.
#[derive(Debug, Clone, Serialize)]
pub struct ParameterReading {
    pub parameter_name: String,
    pub value: f64,
    pub within_control: bool,
    pub within_spec: bool,
    pub timestamp: String,
}

/// Statistical process control data for a parameter.
#[derive(Debug, Clone, Serialize)]
pub struct SpcData {
    pub parameter_name: String,
    pub readings: Vec<f64>,
    pub mean: f64,
    pub sigma: f64,
    pub ucl: f64,
    pub lcl: f64,
    pub usl: f64,
    pub lsl: f64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during manufacturing operations.
#[derive(Debug, Clone, PartialEq)]
pub enum MfgError {
    /// A parameter reading is outside its specification limits.
    ParameterOutOfSpec(String),
    /// The lot has already completed all steps.
    NoMoreSteps,
    /// The lot is on hold and cannot advance.
    LotOnHold,
}

impl fmt::Display for MfgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParameterOutOfSpec(msg) => write!(f, "parameter out of spec: {msg}"),
            Self::NoMoreSteps => f.write_str("no more steps in routing"),
            Self::LotOnHold => f.write_str("lot is on hold"),
        }
    }
}

impl std::error::Error for MfgError {}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Extract a manufacturing routing from a SysML model by finding the action
/// definition matching `name` and converting its sub-actions to process steps.
///
/// Each action usage whose `parent_def` matches `name` becomes a step, ordered
/// by its appearance in the model (preserving the definition order). The
/// process type is inferred from the step name via keyword matching.
pub fn extract_routing(model: &Model, name: &str) -> Option<Vec<ProcessStep>> {
    // Verify that the named action definition exists.
    let def = model.definitions.iter().find(|d| {
        d.name == name && d.kind == DefKind::Action
    })?;

    // Collect child action usages belonging to this definition.
    let children: Vec<_> = model
        .usages
        .iter()
        .filter(|u| u.parent_def.as_deref() == Some(&def.name) && u.kind == "action")
        .collect();

    if children.is_empty() {
        return None;
    }

    let steps = children
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let process_type = ProcessType::infer_from_name(&u.name);
            let description = u
                .type_ref
                .as_deref()
                .unwrap_or("")
                .to_string();
            let inspection_required = matches!(
                process_type,
                ProcessType::TestAndInspection | ProcessType::Calibration
            );
            ProcessStep {
                number: i + 1,
                name: u.name.clone(),
                process_type,
                description,
                parameters: Vec::new(),
                inspection_required,
                status: StepStatus::Pending,
            }
        })
        .collect();

    Some(steps)
}

/// Create a new production lot with a generated ID.
pub fn create_lot(
    routing_name: &str,
    steps: Vec<ProcessStep>,
    quantity: u32,
    lot_type: LotType,
) -> Lot {
    let id = generate_record_id("mfg", "lot", routing_name);
    Lot {
        id,
        routing_name: routing_name.to_string(),
        quantity,
        lot_type,
        status: LotStatus::Created,
        steps,
        current_step: 0,
    }
}

/// Create a record envelope for a lot suitable for persistence.
pub fn create_lot_record(lot: &Lot, author: &str) -> RecordEnvelope {
    let mut refs = BTreeMap::new();
    refs.insert(
        "routing".into(),
        vec![lot.routing_name.clone()],
    );

    let step_names: Vec<String> = lot.steps.iter().map(|s| s.name.clone()).collect();
    refs.insert("steps".into(), step_names);

    let mut data = BTreeMap::new();
    data.insert(
        "lot_id".into(),
        RecordValue::String(lot.id.clone()),
    );
    data.insert(
        "quantity".into(),
        RecordValue::Integer(lot.quantity as i64),
    );
    data.insert(
        "lot_type".into(),
        RecordValue::String(format!("{}", lot.lot_type)),
    );
    data.insert(
        "status".into(),
        RecordValue::String(format!("{}", lot.status)),
    );
    data.insert(
        "current_step".into(),
        RecordValue::Integer(lot.current_step as i64),
    );
    data.insert(
        "total_steps".into(),
        RecordValue::Integer(lot.steps.len() as i64),
    );

    // Embed per-step status as a sub-table.
    let mut step_statuses = BTreeMap::new();
    for step in &lot.steps {
        step_statuses.insert(
            format!("step_{}", step.number),
            RecordValue::String(format!("{}", step.status)),
        );
    }
    data.insert("step_statuses".into(), RecordValue::Table(step_statuses));

    RecordEnvelope {
        meta: RecordMeta {
            id: generate_record_id("mfg", "lot", author),
            tool: "mfg".into(),
            record_type: "lot".into(),
            created: now_iso8601(),
            author: author.to_string(),
        },
        refs,
        data,
    }
}

/// Advance the lot to the next step, validating parameter readings against
/// control and specification limits.
///
/// Returns the resulting status of the step that was just completed:
/// - `Passed` if all readings are within spec limits.
/// - `Deviated` if all readings are within spec but some are outside control limits.
/// - Returns `Err(ParameterOutOfSpec)` if any reading is outside spec limits.
/// - Returns `Err(NoMoreSteps)` if the lot has already completed all steps.
/// - Returns `Err(LotOnHold)` if the lot is on hold.
pub fn advance_step(
    lot: &mut Lot,
    readings: Vec<ParameterReading>,
) -> Result<StepStatus, MfgError> {
    if lot.status == LotStatus::OnHold {
        return Err(MfgError::LotOnHold);
    }
    if lot.current_step >= lot.steps.len() {
        return Err(MfgError::NoMoreSteps);
    }

    // Transition lot status on first advance.
    if lot.status == LotStatus::Created {
        lot.status = LotStatus::InProgress;
    }

    // Mark current step as in-progress.
    lot.steps[lot.current_step].status = StepStatus::InProgress;

    // Validate readings against spec limits.
    let mut any_out_of_control = false;
    for reading in &readings {
        if !reading.within_spec {
            lot.steps[lot.current_step].status = StepStatus::Failed;
            return Err(MfgError::ParameterOutOfSpec(format!(
                "{}: value {:.4} is outside specification limits",
                reading.parameter_name, reading.value
            )));
        }
        if !reading.within_control {
            any_out_of_control = true;
        }
    }

    // Determine step outcome.
    let step_status = if any_out_of_control {
        StepStatus::Deviated
    } else {
        StepStatus::Passed
    };

    lot.steps[lot.current_step].status = step_status;
    lot.current_step += 1;

    // Check if all steps are complete.
    if lot.current_step >= lot.steps.len() {
        lot.status = LotStatus::Completed;
    }

    Ok(step_status)
}

/// Compute SPC statistics (mean, standard deviation) for a set of readings.
pub fn compute_spc(
    readings: &[f64],
    ucl: f64,
    lcl: f64,
    usl: f64,
    lsl: f64,
) -> SpcData {
    let n = readings.len() as f64;
    let mean = if n > 0.0 {
        readings.iter().sum::<f64>() / n
    } else {
        0.0
    };

    let sigma = if n > 1.0 {
        let variance = readings.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        variance.sqrt()
    } else {
        0.0
    };

    SpcData {
        parameter_name: String::new(),
        readings: readings.to_vec(),
        mean,
        sigma,
        ucl,
        lcl,
        usl,
        lsl,
    }
}

/// Format an ASCII SPC chart showing readings relative to control and spec
/// limits.
///
/// The chart uses a fixed-width layout with `|` markers for UCL/LCL,
/// `!` markers for USL/LSL, and `*` for each reading.
pub fn format_spc_text(spc: &SpcData) -> String {
    let mut out = String::new();

    out.push_str(&format!("SPC Chart: {}\n", spc.parameter_name));
    out.push_str(&format!(
        "  Mean: {:.4}  Sigma: {:.4}  N: {}\n",
        spc.mean,
        spc.sigma,
        spc.readings.len()
    ));
    out.push_str(&format!(
        "  UCL: {:.4}  LCL: {:.4}  USL: {:.4}  LSL: {:.4}\n",
        spc.ucl, spc.lcl, spc.usl, spc.lsl
    ));

    if spc.readings.is_empty() {
        out.push_str("  (no readings)\n");
        return out;
    }

    // Determine chart range: use the widest of USL/LSL with some margin.
    let all_values: Vec<f64> = spc
        .readings
        .iter()
        .copied()
        .chain([spc.ucl, spc.lcl, spc.usl, spc.lsl, spc.mean])
        .collect();
    let min_val = all_values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = all_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max_val - min_val;
    let margin = if range > 0.0 { range * 0.1 } else { 1.0 };
    let chart_min = min_val - margin;
    let chart_max = max_val + margin;
    let chart_range = chart_max - chart_min;

    let width: usize = 60;

    // Helper to map a value to a column position.
    let to_col = |v: f64| -> usize {
        let frac = (v - chart_min) / chart_range;
        let col = (frac * (width as f64 - 1.0)).round() as i64;
        col.clamp(0, width as i64 - 1) as usize
    };

    let ucl_col = to_col(spc.ucl);
    let lcl_col = to_col(spc.lcl);
    let usl_col = to_col(spc.usl);
    let lsl_col = to_col(spc.lsl);
    let mean_col = to_col(spc.mean);

    // Header line with limit labels.
    out.push_str("  ");
    let mut header = vec![' '; width];
    if lsl_col < width {
        header[lsl_col] = 'L';
    }
    if lcl_col < width {
        header[lcl_col] = 'l';
    }
    if mean_col < width {
        header[mean_col] = 'M';
    }
    if ucl_col < width {
        header[ucl_col] = 'u';
    }
    if usl_col < width {
        header[usl_col] = 'U';
    }
    out.extend(header.iter());
    out.push('\n');

    // Limit reference line.
    out.push_str("  ");
    let mut limit_line = vec!['-'; width];
    if lsl_col < width {
        limit_line[lsl_col] = '!';
    }
    if lcl_col < width {
        limit_line[lcl_col] = '|';
    }
    if mean_col < width {
        limit_line[mean_col] = '+';
    }
    if ucl_col < width {
        limit_line[ucl_col] = '|';
    }
    if usl_col < width {
        limit_line[usl_col] = '!';
    }
    out.extend(limit_line.iter());
    out.push('\n');

    // One row per reading.
    for (i, &val) in spc.readings.iter().enumerate() {
        out.push_str(&format!("{:>3} ", i + 1));
        let col = to_col(val);
        let mut row = vec![' '; width];
        // Draw faint reference markers.
        if lcl_col < width {
            row[lcl_col] = '.';
        }
        if ucl_col < width {
            row[ucl_col] = '.';
        }
        if mean_col < width {
            row[mean_col] = ':';
        }
        // Plot the reading.
        if col < width {
            row[col] = '*';
        }
        out.extend(row.iter());
        out.push_str(&format!(" {:.4}", val));
        out.push('\n');
    }

    out
}

/// Return a text summary of a lot's current status.
pub fn lot_summary(lot: &Lot) -> String {
    let mut out = String::new();

    out.push_str(&format!("Lot: {}\n", lot.id));
    out.push_str(&format!("  Routing:  {}\n", lot.routing_name));
    out.push_str(&format!("  Type:     {}\n", lot.lot_type));
    out.push_str(&format!("  Quantity: {}\n", lot.quantity));
    out.push_str(&format!("  Status:   {}\n", lot.status));
    out.push_str(&format!(
        "  Progress: {}/{} steps\n",
        lot.current_step,
        lot.steps.len()
    ));

    if !lot.steps.is_empty() {
        out.push_str("  Steps:\n");
        for step in &lot.steps {
            let marker = match step.status {
                StepStatus::Passed => "[PASS]",
                StepStatus::Failed => "[FAIL]",
                StepStatus::Deviated => "[DEV] ",
                StepStatus::Skipped => "[SKIP]",
                StepStatus::InProgress => "[... ]",
                StepStatus::Pending => "[    ]",
            };
            let inspect = if step.inspection_required {
                " (inspection)"
            } else {
                ""
            };
            out.push_str(&format!(
                "    {} {:>2}. {} [{}]{}\n",
                marker,
                step.number,
                step.name,
                step.process_type,
                inspect,
            ));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::model::{Definition, Model, Span, Usage};

    /// Build a minimal model with an action definition and child action usages.
    fn sample_model() -> Model {
        let mut model = Model::new("test.sysml".into());
        model.definitions.push(Definition {
            kind: DefKind::Action,
            name: "ManufactureWidget".into(),
            super_type: None,
            span: Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: None,
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        let step_names = [
            "CutBlanks",
            "MachineHousing",
            "WeldAssembly",
            "HeatTreatPart",
            "CoatSurface",
            "InspectDimensions",
            "PackageForShipment",
        ];

        for name in &step_names {
            model.usages.push(Usage {
                kind: "action".into(),
                name: name.to_string(),
                type_ref: None,
                span: Span::default(),
                direction: None,
                is_conjugated: false,
                parent_def: Some("ManufactureWidget".into()),
                multiplicity: None,
                value_expr: None,
                short_name: None,
                redefinition: None,
                subsets: None,
                qualified_name: None,
            });
        }

        model
    }

    fn sample_steps() -> Vec<ProcessStep> {
        vec![
            ProcessStep {
                number: 1,
                name: "CutBlanks".into(),
                process_type: ProcessType::SheetMetal,
                description: String::new(),
                parameters: vec![ProcessParameter {
                    name: "length".into(),
                    nominal: 100.0,
                    ucl: 100.5,
                    lcl: 99.5,
                    usl: 101.0,
                    lsl: 99.0,
                    unit: "mm".into(),
                }],
                inspection_required: false,
                status: StepStatus::Pending,
            },
            ProcessStep {
                number: 2,
                name: "MachineHousing".into(),
                process_type: ProcessType::Machining,
                description: String::new(),
                parameters: Vec::new(),
                inspection_required: false,
                status: StepStatus::Pending,
            },
            ProcessStep {
                number: 3,
                name: "InspectDimensions".into(),
                process_type: ProcessType::TestAndInspection,
                description: String::new(),
                parameters: Vec::new(),
                inspection_required: true,
                status: StepStatus::Pending,
            },
        ]
    }

    fn make_reading(name: &str, value: f64, within_control: bool, within_spec: bool) -> ParameterReading {
        ParameterReading {
            parameter_name: name.into(),
            value,
            within_control,
            within_spec,
            timestamp: "2026-03-09T10:00:00Z".into(),
        }
    }

    // -----------------------------------------------------------------------
    // ProcessType tests
    // -----------------------------------------------------------------------

    #[test]
    fn process_type_labels_are_nonempty() {
        let types = [
            ProcessType::Machining,
            ProcessType::Welding,
            ProcessType::Brazing,
            ProcessType::Soldering,
            ProcessType::AdhesiveBonding,
            ProcessType::Molding,
            ProcessType::Casting,
            ProcessType::Forging,
            ProcessType::Stamping,
            ProcessType::SheetMetal,
            ProcessType::HeatTreat,
            ProcessType::SurfaceTreatment,
            ProcessType::Coating,
            ProcessType::Assembly,
            ProcessType::TestAndInspection,
            ProcessType::Packaging,
            ProcessType::Cleaning,
            ProcessType::Printing3d,
            ProcessType::Programming,
            ProcessType::Calibration,
        ];
        for pt in &types {
            assert!(!pt.label().is_empty());
            assert!(!pt.to_string().is_empty());
        }
    }

    #[test]
    fn process_type_infer_keywords() {
        assert_eq!(ProcessType::infer_from_name("CNC_Machining"), ProcessType::Machining);
        assert_eq!(ProcessType::infer_from_name("WeldJoint"), ProcessType::Welding);
        assert_eq!(ProcessType::infer_from_name("BrazeFitting"), ProcessType::Brazing);
        assert_eq!(ProcessType::infer_from_name("SolderPCB"), ProcessType::Soldering);
        assert_eq!(ProcessType::infer_from_name("AdhesiveBond"), ProcessType::AdhesiveBonding);
        assert_eq!(ProcessType::infer_from_name("InjectionMold"), ProcessType::Molding);
        assert_eq!(ProcessType::infer_from_name("SandCasting"), ProcessType::Casting);
        assert_eq!(ProcessType::infer_from_name("ForgeBillet"), ProcessType::Forging);
        assert_eq!(ProcessType::infer_from_name("StampPanel"), ProcessType::Stamping);
        assert_eq!(ProcessType::infer_from_name("SheetBend"), ProcessType::SheetMetal);
        assert_eq!(ProcessType::infer_from_name("HeatTreat"), ProcessType::HeatTreat);
        assert_eq!(ProcessType::infer_from_name("SurfaceFinish"), ProcessType::SurfaceTreatment);
        assert_eq!(ProcessType::infer_from_name("PaintCoat"), ProcessType::Coating);
        assert_eq!(ProcessType::infer_from_name("FinalAssembly"), ProcessType::Assembly);
        assert_eq!(ProcessType::infer_from_name("InspectDim"), ProcessType::TestAndInspection);
        assert_eq!(ProcessType::infer_from_name("PackageShip"), ProcessType::Packaging);
        assert_eq!(ProcessType::infer_from_name("CleanPart"), ProcessType::Cleaning);
        assert_eq!(ProcessType::infer_from_name("3DPrint"), ProcessType::Printing3d);
        assert_eq!(ProcessType::infer_from_name("ProgramFirmware"), ProcessType::Programming);
        assert_eq!(ProcessType::infer_from_name("CalibrateGauge"), ProcessType::Calibration);
    }

    #[test]
    fn process_type_infer_defaults_to_assembly() {
        assert_eq!(ProcessType::infer_from_name("DoSomething"), ProcessType::Assembly);
    }

    // -----------------------------------------------------------------------
    // Enum Display tests
    // -----------------------------------------------------------------------

    #[test]
    fn lot_type_display() {
        assert_eq!(LotType::Production.to_string(), "Production");
        assert_eq!(LotType::Prototype.to_string(), "Prototype");
        assert_eq!(LotType::FirstArticle.to_string(), "First Article");
    }

    #[test]
    fn lot_status_display() {
        assert_eq!(LotStatus::Created.to_string(), "Created");
        assert_eq!(LotStatus::InProgress.to_string(), "In Progress");
        assert_eq!(LotStatus::OnHold.to_string(), "On Hold");
        assert_eq!(LotStatus::Completed.to_string(), "Completed");
        assert_eq!(LotStatus::Scrapped.to_string(), "Scrapped");
    }

    #[test]
    fn step_status_display() {
        assert_eq!(StepStatus::Pending.to_string(), "Pending");
        assert_eq!(StepStatus::InProgress.to_string(), "In Progress");
        assert_eq!(StepStatus::Passed.to_string(), "Passed");
        assert_eq!(StepStatus::Failed.to_string(), "Failed");
        assert_eq!(StepStatus::Skipped.to_string(), "Skipped");
        assert_eq!(StepStatus::Deviated.to_string(), "Deviated");
    }

    // -----------------------------------------------------------------------
    // MfgError tests
    // -----------------------------------------------------------------------

    #[test]
    fn mfg_error_display() {
        let e = MfgError::ParameterOutOfSpec("length too long".into());
        assert_eq!(e.to_string(), "parameter out of spec: length too long");

        let e = MfgError::NoMoreSteps;
        assert_eq!(e.to_string(), "no more steps in routing");

        let e = MfgError::LotOnHold;
        assert_eq!(e.to_string(), "lot is on hold");
    }

    #[test]
    fn mfg_error_is_std_error() {
        let e: Box<dyn std::error::Error> =
            Box::new(MfgError::ParameterOutOfSpec("test".into()));
        assert!(e.to_string().contains("test"));
    }

    // -----------------------------------------------------------------------
    // extract_routing tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_routing_finds_steps() {
        let model = sample_model();
        let steps = extract_routing(&model, "ManufactureWidget").unwrap();
        assert_eq!(steps.len(), 7);
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].name, "CutBlanks");
        assert_eq!(steps[6].name, "PackageForShipment");
    }

    #[test]
    fn extract_routing_infers_process_types() {
        let model = sample_model();
        let steps = extract_routing(&model, "ManufactureWidget").unwrap();
        // "CutBlanks" does not match any keyword strongly, but let's check
        // a few that should match clearly:
        assert_eq!(steps[1].process_type, ProcessType::Machining); // MachineHousing
        assert_eq!(steps[2].process_type, ProcessType::Welding); // WeldAssembly
        assert_eq!(steps[3].process_type, ProcessType::HeatTreat); // HeatTreatPart
        assert_eq!(steps[4].process_type, ProcessType::Coating); // CoatSurface
        assert_eq!(steps[5].process_type, ProcessType::TestAndInspection); // InspectDimensions
        assert_eq!(steps[6].process_type, ProcessType::Packaging); // PackageForShipment
    }

    #[test]
    fn extract_routing_sets_inspection_flag() {
        let model = sample_model();
        let steps = extract_routing(&model, "ManufactureWidget").unwrap();
        // InspectDimensions should have inspection_required = true.
        let inspect_step = steps.iter().find(|s| s.name == "InspectDimensions").unwrap();
        assert!(inspect_step.inspection_required);
        // MachineHousing should not.
        let machine_step = steps.iter().find(|s| s.name == "MachineHousing").unwrap();
        assert!(!machine_step.inspection_required);
    }

    #[test]
    fn extract_routing_returns_none_for_missing_def() {
        let model = sample_model();
        assert!(extract_routing(&model, "NonExistent").is_none());
    }

    #[test]
    fn extract_routing_returns_none_for_empty_children() {
        let mut model = Model::new("test.sysml".into());
        model.definitions.push(Definition {
            kind: DefKind::Action,
            name: "EmptyAction".into(),
            super_type: None,
            span: Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: None,
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });
        assert!(extract_routing(&model, "EmptyAction").is_none());
    }

    // -----------------------------------------------------------------------
    // create_lot tests
    // -----------------------------------------------------------------------

    #[test]
    fn create_lot_initializes_correctly() {
        let steps = sample_steps();
        let lot = create_lot("TestRouting", steps, 50, LotType::Production);
        assert_eq!(lot.routing_name, "TestRouting");
        assert_eq!(lot.quantity, 50);
        assert_eq!(lot.lot_type, LotType::Production);
        assert_eq!(lot.status, LotStatus::Created);
        assert_eq!(lot.current_step, 0);
        assert_eq!(lot.steps.len(), 3);
        assert!(lot.id.starts_with("mfg-lot-"));
    }

    #[test]
    fn create_lot_with_prototype_type() {
        let steps = sample_steps();
        let lot = create_lot("Proto", steps, 5, LotType::Prototype);
        assert_eq!(lot.lot_type, LotType::Prototype);
        assert_eq!(lot.quantity, 5);
    }

    // -----------------------------------------------------------------------
    // create_lot_record tests
    // -----------------------------------------------------------------------

    #[test]
    fn create_lot_record_has_correct_structure() {
        let steps = sample_steps();
        let lot = create_lot("WidgetRouting", steps, 100, LotType::Production);
        let record = create_lot_record(&lot, "operator1");

        assert_eq!(record.meta.tool, "mfg");
        assert_eq!(record.meta.record_type, "lot");
        assert_eq!(record.meta.author, "operator1");
        assert!(record.refs.contains_key("routing"));
        assert!(record.refs.contains_key("steps"));
        assert_eq!(
            record.data.get("lot_id"),
            Some(&RecordValue::String(lot.id.clone()))
        );
        assert_eq!(
            record.data.get("quantity"),
            Some(&RecordValue::Integer(100))
        );
    }

    #[test]
    fn create_lot_record_round_trips_via_toml() {
        let steps = sample_steps();
        let lot = create_lot("Routing", steps, 10, LotType::FirstArticle);
        let record = create_lot_record(&lot, "qa");
        let toml = record.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.tool, "mfg");
        assert_eq!(parsed.meta.author, "qa");
        assert_eq!(parsed.data.get("quantity"), Some(&RecordValue::Integer(10)));
    }

    // -----------------------------------------------------------------------
    // advance_step tests
    // -----------------------------------------------------------------------

    #[test]
    fn advance_step_passes_with_good_readings() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        let readings = vec![make_reading("length", 100.1, true, true)];
        let result = advance_step(&mut lot, readings).unwrap();
        assert_eq!(result, StepStatus::Passed);
        assert_eq!(lot.steps[0].status, StepStatus::Passed);
        assert_eq!(lot.current_step, 1);
        assert_eq!(lot.status, LotStatus::InProgress);
    }

    #[test]
    fn advance_step_deviated_outside_control() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        let readings = vec![make_reading("length", 100.8, false, true)];
        let result = advance_step(&mut lot, readings).unwrap();
        assert_eq!(result, StepStatus::Deviated);
        assert_eq!(lot.steps[0].status, StepStatus::Deviated);
    }

    #[test]
    fn advance_step_fails_out_of_spec() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        let readings = vec![make_reading("length", 105.0, false, false)];
        let err = advance_step(&mut lot, readings).unwrap_err();
        assert!(matches!(err, MfgError::ParameterOutOfSpec(_)));
        assert_eq!(lot.steps[0].status, StepStatus::Failed);
    }

    #[test]
    fn advance_step_completes_lot() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);

        // Advance through all 3 steps.
        for _ in 0..3 {
            advance_step(&mut lot, vec![]).unwrap();
        }
        assert_eq!(lot.status, LotStatus::Completed);
        assert_eq!(lot.current_step, 3);
    }

    #[test]
    fn advance_step_error_no_more_steps() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        for _ in 0..3 {
            advance_step(&mut lot, vec![]).unwrap();
        }
        let err = advance_step(&mut lot, vec![]).unwrap_err();
        assert_eq!(err, MfgError::NoMoreSteps);
    }

    #[test]
    fn advance_step_error_lot_on_hold() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        lot.status = LotStatus::OnHold;
        let err = advance_step(&mut lot, vec![]).unwrap_err();
        assert_eq!(err, MfgError::LotOnHold);
    }

    // -----------------------------------------------------------------------
    // compute_spc tests
    // -----------------------------------------------------------------------

    #[test]
    fn compute_spc_basic_stats() {
        let readings = vec![10.0, 10.1, 9.9, 10.05, 9.95];
        let spc = compute_spc(&readings, 10.3, 9.7, 10.5, 9.5);
        assert!((spc.mean - 10.0).abs() < 0.001);
        assert!(spc.sigma > 0.0);
        assert_eq!(spc.readings.len(), 5);
        assert_eq!(spc.ucl, 10.3);
        assert_eq!(spc.lcl, 9.7);
        assert_eq!(spc.usl, 10.5);
        assert_eq!(spc.lsl, 9.5);
    }

    #[test]
    fn compute_spc_single_reading() {
        let readings = vec![5.0];
        let spc = compute_spc(&readings, 6.0, 4.0, 7.0, 3.0);
        assert_eq!(spc.mean, 5.0);
        assert_eq!(spc.sigma, 0.0);
    }

    #[test]
    fn compute_spc_empty_readings() {
        let spc = compute_spc(&[], 10.0, 0.0, 12.0, -2.0);
        assert_eq!(spc.mean, 0.0);
        assert_eq!(spc.sigma, 0.0);
        assert!(spc.readings.is_empty());
    }

    #[test]
    fn compute_spc_known_sigma() {
        // For [2, 4, 4, 4, 5, 5, 7, 9], mean=5.
        // Sum of squared deviations = 32, sample variance = 32/7, sigma = sqrt(32/7).
        let readings = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let spc = compute_spc(&readings, 8.0, 2.0, 10.0, 0.0);
        let expected_sigma = (32.0_f64 / 7.0).sqrt(); // ~2.1381
        assert!((spc.mean - 5.0).abs() < 0.001);
        assert!((spc.sigma - expected_sigma).abs() < 0.001);
    }

    // -----------------------------------------------------------------------
    // format_spc_text tests
    // -----------------------------------------------------------------------

    #[test]
    fn format_spc_text_contains_header() {
        let readings = vec![10.0, 10.1, 9.9];
        let mut spc = compute_spc(&readings, 10.3, 9.7, 10.5, 9.5);
        spc.parameter_name = "Diameter".into();
        let text = format_spc_text(&spc);
        assert!(text.contains("SPC Chart: Diameter"));
        assert!(text.contains("Mean:"));
        assert!(text.contains("Sigma:"));
        assert!(text.contains("UCL:"));
        assert!(text.contains("LCL:"));
    }

    #[test]
    fn format_spc_text_has_readings_rows() {
        let readings = vec![10.0, 10.1, 9.9, 10.2, 9.8];
        let mut spc = compute_spc(&readings, 10.3, 9.7, 10.5, 9.5);
        spc.parameter_name = "Width".into();
        let text = format_spc_text(&spc);
        // Should have one row per reading.
        let data_lines: Vec<_> = text.lines().filter(|l| l.contains('*')).collect();
        assert_eq!(data_lines.len(), 5);
    }

    #[test]
    fn format_spc_text_empty_readings() {
        let spc = SpcData {
            parameter_name: "Empty".into(),
            readings: vec![],
            mean: 0.0,
            sigma: 0.0,
            ucl: 1.0,
            lcl: -1.0,
            usl: 2.0,
            lsl: -2.0,
        };
        let text = format_spc_text(&spc);
        assert!(text.contains("(no readings)"));
    }

    // -----------------------------------------------------------------------
    // lot_summary tests
    // -----------------------------------------------------------------------

    #[test]
    fn lot_summary_contains_key_info() {
        let steps = sample_steps();
        let lot = create_lot("WidgetRouting", steps, 100, LotType::Production);
        let summary = lot_summary(&lot);
        assert!(summary.contains("WidgetRouting"));
        assert!(summary.contains("Production"));
        assert!(summary.contains("100"));
        assert!(summary.contains("Created"));
        assert!(summary.contains("0/3 steps"));
        assert!(summary.contains("CutBlanks"));
        assert!(summary.contains("MachineHousing"));
        assert!(summary.contains("InspectDimensions"));
    }

    #[test]
    fn lot_summary_shows_step_status_after_advance() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        advance_step(&mut lot, vec![]).unwrap();
        let summary = lot_summary(&lot);
        assert!(summary.contains("[PASS]"));
        assert!(summary.contains("1/3 steps"));
    }

    #[test]
    fn lot_summary_shows_completed_lot() {
        let steps = sample_steps();
        let mut lot = create_lot("R", steps, 10, LotType::Production);
        for _ in 0..3 {
            advance_step(&mut lot, vec![]).unwrap();
        }
        let summary = lot_summary(&lot);
        assert!(summary.contains("Completed"));
        assert!(summary.contains("3/3 steps"));
    }
}
