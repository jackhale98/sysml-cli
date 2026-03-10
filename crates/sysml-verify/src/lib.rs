//! Verification domain for SysML v2 models.
//!
//! This crate provides types and functions for managing the verification
//! lifecycle of SysML v2 models: extracting verification cases from the model,
//! building interactive wizard flows for executing them, recording results as
//! TOML records, and computing verification coverage across requirements.
//!
//! Depends only on `sysml-core`.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use sysml_core::interactive::WizardStep;
use sysml_core::model::{DefKind, Model};
use sysml_core::record::{
    generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue,
};

// ========================================================================
// Verification case types
// ========================================================================

/// A verification case extracted from the model.
///
/// Represents a `verification def` that verifies one or more requirements
/// through a sequence of procedural steps.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationCase {
    /// Simple name of the verification definition.
    pub name: String,
    /// Fully qualified name, if available.
    pub qualified_name: Option<String>,
    /// Names of requirements being verified (from `verify` relationships).
    pub requirements: Vec<String>,
    /// Ordered procedure steps extracted from sub-usages.
    pub steps: Vec<VerificationStep>,
    /// Constraint expressions that define acceptance criteria.
    pub acceptance_criteria: Vec<String>,
}

/// A single step in a verification procedure.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationStep {
    /// Step number (1-based).
    pub number: usize,
    /// Human-readable description of this step.
    pub description: String,
    /// Whether this step captures a measurement value.
    pub is_measurement: bool,
}

// ========================================================================
// Execution result types
// ========================================================================

/// The outcome of executing a verification case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExecutionResult {
    /// All acceptance criteria met.
    Pass,
    /// One or more acceptance criteria not met.
    Fail,
    /// Acceptance criteria met with noted deviations.
    ConditionalPass(String),
    /// Execution could not proceed due to a blocking condition.
    Blocked(String),
}

impl ExecutionResult {
    /// Return a short label for the result.
    pub fn label(&self) -> &str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::ConditionalPass(_) => "conditional_pass",
            Self::Blocked(_) => "blocked",
        }
    }

    /// Whether this result counts as a successful verification.
    pub fn is_passing(&self) -> bool {
        matches!(self, Self::Pass | Self::ConditionalPass(_))
    }
}

/// A measurement captured during verification execution.
#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    /// Name of the measured quantity.
    pub name: String,
    /// Measured value.
    pub value: f64,
    /// Unit of measurement.
    pub unit: String,
    /// Whether the measured value is within specification limits.
    pub within_spec: bool,
}

/// The complete result of running a verification case.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationExecution {
    /// Qualified name of the verification case that was executed.
    pub verification_case: String,
    /// Overall execution result.
    pub result: ExecutionResult,
    /// Requirements verified by this execution.
    pub requirements_verified: Vec<String>,
    /// Measurements captured during execution.
    pub measurements: Vec<Measurement>,
    /// Free-form notes from the executor.
    pub notes: String,
}

// ========================================================================
// Coverage report types
// ========================================================================

/// Per-requirement verification coverage detail.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageItem {
    /// Requirement name.
    pub requirement: String,
    /// Names of verification cases that cover this requirement.
    pub verification_cases: Vec<String>,
    /// Most recent execution result, if any.
    pub last_result: Option<ExecutionResult>,
    /// Whether there is at least one passing execution record.
    pub is_verified: bool,
}

/// Aggregate verification coverage across all requirements.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    /// Total number of requirements in the model.
    pub total_requirements: usize,
    /// Number of requirements covered by at least one verification case.
    pub verified_requirements: usize,
    /// Number of requirements with at least one passing execution.
    pub passing_requirements: usize,
    /// Per-requirement detail.
    pub items: Vec<CoverageItem>,
}

impl CoverageReport {
    /// Fraction of requirements covered by verification cases (0.0..=1.0).
    pub fn coverage_pct(&self) -> f64 {
        if self.total_requirements == 0 {
            return 1.0;
        }
        self.verified_requirements as f64 / self.total_requirements as f64
    }

    /// Fraction of requirements with passing executions (0.0..=1.0).
    pub fn pass_pct(&self) -> f64 {
        if self.total_requirements == 0 {
            return 1.0;
        }
        self.passing_requirements as f64 / self.total_requirements as f64
    }
}

// ========================================================================
// Extraction from model
// ========================================================================

/// Extract verification cases from a parsed SysML model.
///
/// Walks all definitions of kind `Verification`, collects `verify`
/// relationships to determine which requirements each case covers, and
/// extracts sub-usages as procedural steps.
pub fn extract_verification_cases(model: &Model) -> Vec<VerificationCase> {
    let verification_defs: Vec<_> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Verification)
        .collect();

    let mut cases = Vec::new();

    for vdef in &verification_defs {
        // Collect requirements verified by this case.
        let requirements: Vec<String> = model
            .verifications
            .iter()
            .filter(|v| {
                sysml_core::model::simple_name(&v.by) == vdef.name
            })
            .map(|v| v.requirement.clone())
            .collect();

        // Extract sub-usages as steps.  Usages whose parent is this
        // verification def become steps in order of appearance.
        let sub_usages: Vec<_> = model
            .usages
            .iter()
            .filter(|u| u.parent_def.as_deref() == Some(&vdef.name))
            .collect();

        let steps: Vec<VerificationStep> = sub_usages
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let desc = if let Some(tr) = &u.type_ref {
                    format!("{}: {}", u.name, tr)
                } else {
                    u.name.clone()
                };
                let is_measurement = u.kind.contains("attribute")
                    || u.name.to_lowercase().contains("measure")
                    || u.name.to_lowercase().contains("reading");
                VerificationStep {
                    number: i + 1,
                    description: desc,
                    is_measurement,
                }
            })
            .collect();

        // Extract acceptance criteria from constraint usages within this def.
        let acceptance_criteria: Vec<String> = model
            .usages
            .iter()
            .filter(|u| {
                u.parent_def.as_deref() == Some(&vdef.name)
                    && u.kind == "constraint"
            })
            .filter_map(|u| u.value_expr.clone())
            .collect();

        let qualified_name = vdef
            .qualified_name
            .as_ref()
            .map(|qn| qn.to_string());

        cases.push(VerificationCase {
            name: vdef.name.clone(),
            qualified_name,
            requirements,
            steps,
            acceptance_criteria,
        });
    }

    cases
}

// ========================================================================
// Execution record creation
// ========================================================================

/// Create a TOML record envelope from a verification execution result.
///
/// The record captures the execution outcome, measurements, and links
/// back to the verified requirements and verification case in the model.
pub fn create_execution_record(
    execution: &VerificationExecution,
    author: &str,
) -> RecordEnvelope {
    let id = generate_record_id("verify", "execution", author);
    let created = now_iso8601();

    let meta = RecordMeta {
        id,
        tool: "verify".to_string(),
        record_type: "execution".to_string(),
        created,
        author: author.to_string(),
    };

    // Build refs: link to verified requirements and the verification case.
    let mut refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    refs.insert(
        "requirements".to_string(),
        execution.requirements_verified.clone(),
    );
    refs.insert(
        "verification_case".to_string(),
        vec![execution.verification_case.clone()],
    );

    // Build data section.
    let mut data: BTreeMap<String, RecordValue> = BTreeMap::new();
    data.insert(
        "result".to_string(),
        RecordValue::String(execution.result.label().to_string()),
    );

    // Include condition/reason for conditional pass or blocked.
    match &execution.result {
        ExecutionResult::ConditionalPass(condition) => {
            data.insert(
                "condition".to_string(),
                RecordValue::String(condition.clone()),
            );
        }
        ExecutionResult::Blocked(reason) => {
            data.insert(
                "blocked_reason".to_string(),
                RecordValue::String(reason.clone()),
            );
        }
        _ => {}
    }

    // Measurements as an array of inline tables.
    if !execution.measurements.is_empty() {
        let meas_array: Vec<RecordValue> = execution
            .measurements
            .iter()
            .map(|m| {
                let mut tbl = BTreeMap::new();
                tbl.insert("name".to_string(), RecordValue::String(m.name.clone()));
                tbl.insert("value".to_string(), RecordValue::Float(m.value));
                tbl.insert("unit".to_string(), RecordValue::String(m.unit.clone()));
                tbl.insert(
                    "within_spec".to_string(),
                    RecordValue::Bool(m.within_spec),
                );
                RecordValue::Table(tbl)
            })
            .collect();
        data.insert("measurements".to_string(), RecordValue::Array(meas_array));
    }

    if !execution.notes.is_empty() {
        data.insert(
            "notes".to_string(),
            RecordValue::String(execution.notes.clone()),
        );
    }

    RecordEnvelope { meta, refs, data }
}

// ========================================================================
// Interactive wizard
// ========================================================================

/// Build wizard steps for interactively executing a verification case.
///
/// The wizard walks the user through:
/// 1. Confirming readiness to begin
/// 2. Each procedure step (with measurement capture where needed)
/// 3. Recording observations
/// 4. Confirming the overall result
pub fn build_wizard_steps(vc: &VerificationCase) -> Vec<WizardStep> {
    let mut steps = Vec::new();

    // Step 0: Confirm identity and readiness.
    steps.push(
        WizardStep::confirm(
            "ready",
            &format!(
                "Ready to execute verification case '{}'?",
                vc.name
            ),
        )
        .with_explanation(
            "This wizard will guide you through the verification \
             procedure step by step. You will be asked to confirm each \
             step and record any measurements.",
        ),
    );

    // One step per procedure step.
    for vs in &vc.steps {
        let step_id = format!("step_{}", vs.number);

        if vs.is_measurement {
            // For measurement steps, collect the numeric value.
            steps.push(
                WizardStep::number(
                    &format!("{step_id}_value"),
                    &format!(
                        "Step {}: {} -- Enter measured value:",
                        vs.number, vs.description
                    ),
                )
                .with_explanation(
                    "Record the measured value from this step. \
                     The unit and specification limits will be checked \
                     against the acceptance criteria.",
                ),
            );
            steps.push(
                WizardStep::string(
                    &format!("{step_id}_unit"),
                    &format!("Unit for '{}':", vs.description),
                )
                .with_default(""),
            );
            steps.push(
                WizardStep::confirm(
                    &format!("{step_id}_in_spec"),
                    &format!(
                        "Is the measurement for '{}' within specification?",
                        vs.description
                    ),
                ),
            );
        } else {
            // For non-measurement steps, just confirm completion.
            steps.push(
                WizardStep::confirm(
                    &step_id,
                    &format!(
                        "Step {}: {} -- Completed?",
                        vs.number, vs.description
                    ),
                ),
            );
        }
    }

    // Observations.
    steps.push(
        WizardStep::string("observations", "Any observations or notes?")
            .optional()
            .with_default(""),
    );

    // Overall result.
    steps.push(WizardStep::choice(
        "overall_result",
        "Overall verification result:",
        vec![
            ("pass", "Pass"),
            ("fail", "Fail"),
            ("conditional_pass", "Conditional Pass"),
            ("blocked", "Blocked"),
        ],
    ));

    // Conditional detail (always included; the CLI layer can skip if not needed).
    steps.push(
        WizardStep::string(
            "result_detail",
            "Provide detail for conditional pass or blocked result (if applicable):",
        )
        .optional()
        .with_default(""),
    );

    steps
}

// ========================================================================
// Coverage computation
// ========================================================================

/// Compute verification coverage by combining model traceability with
/// execution records.
///
/// For each requirement in the model, determines:
/// - Which verification cases cover it (from `verify` relationships)
/// - Whether any execution records show a passing result
pub fn coverage_from_records(
    models: &[Model],
    records: &[RecordEnvelope],
) -> CoverageReport {
    // Collect all requirement names across all models.
    let mut all_requirements: Vec<String> = Vec::new();
    for model in models {
        for def in &model.definitions {
            if def.kind == DefKind::Requirement {
                all_requirements.push(def.name.clone());
            }
        }
    }

    // Build a map: requirement -> set of verification case names (from model).
    let mut req_to_vc: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for req in &all_requirements {
        req_to_vc.insert(req.clone(), Vec::new());
    }
    for model in models {
        for v in &model.verifications {
            let req_name =
                sysml_core::model::simple_name(&v.requirement).to_string();
            if let Some(vcs) = req_to_vc.get_mut(&req_name) {
                vcs.push(v.by.clone());
            }
        }
    }

    // Build a map: requirement -> latest execution result (from records).
    // We look at execution records that reference requirements.
    let mut req_results: BTreeMap<String, ExecutionResult> = BTreeMap::new();

    for record in records {
        if record.meta.record_type != "execution" {
            continue;
        }

        let result = record
            .data
            .get("result")
            .and_then(|v| match v {
                RecordValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .map(parse_execution_result)
            .unwrap_or(ExecutionResult::Fail);

        // Find which requirements this record covers.
        if let Some(req_names) = record.refs.get("requirements") {
            for req_name in req_names {
                let simple =
                    sysml_core::model::simple_name(req_name).to_string();
                // Keep the "best" result: Pass > ConditionalPass > Fail > Blocked.
                let should_replace = match req_results.get(&simple) {
                    None => true,
                    Some(existing) => result_rank(&result) > result_rank(existing),
                };
                if should_replace {
                    req_results.insert(simple, result.clone());
                }
            }
        }
    }

    // Build coverage items.
    let mut items: Vec<CoverageItem> = Vec::new();
    let mut verified_count = 0usize;
    let mut passing_count = 0usize;
    let mut seen: HashSet<String> = HashSet::new();

    for req in &all_requirements {
        if !seen.insert(req.clone()) {
            continue; // deduplicate across models
        }
        let vcs = req_to_vc.get(req).cloned().unwrap_or_default();
        let last_result = req_results.get(req).cloned();
        let has_vc = !vcs.is_empty();
        let is_passing = last_result
            .as_ref()
            .map(|r| r.is_passing())
            .unwrap_or(false);

        if has_vc {
            verified_count += 1;
        }
        if is_passing {
            passing_count += 1;
        }

        items.push(CoverageItem {
            requirement: req.clone(),
            verification_cases: vcs,
            last_result,
            is_verified: is_passing,
        });
    }

    CoverageReport {
        total_requirements: items.len(),
        verified_requirements: verified_count,
        passing_requirements: passing_count,
        items,
    }
}

/// Parse an execution result string back into an [`ExecutionResult`].
fn parse_execution_result(s: &str) -> ExecutionResult {
    match s {
        "pass" => ExecutionResult::Pass,
        "fail" => ExecutionResult::Fail,
        "conditional_pass" => ExecutionResult::ConditionalPass(String::new()),
        "blocked" => ExecutionResult::Blocked(String::new()),
        _ => ExecutionResult::Fail,
    }
}

/// Numeric rank for comparing execution results (higher is better).
fn result_rank(r: &ExecutionResult) -> u8 {
    match r {
        ExecutionResult::Pass => 3,
        ExecutionResult::ConditionalPass(_) => 2,
        ExecutionResult::Fail => 1,
        ExecutionResult::Blocked(_) => 0,
    }
}

// ========================================================================
// SysML generation
// ========================================================================

/// Generate SysML text for a verification case definition.
pub fn verification_case_to_sysml(
    name: &str,
    requirements: &[String],
    method: &str,
    steps: &[String],
) -> String {
    let mut out = format!("verification def {} {{\n", name);
    for req in requirements {
        out.push_str(&format!("    verify requirement {};\n", req));
    }
    out.push_str(&format!("    attribute method = VerificationMethod::{};\n", method));
    for (i, step) in steps.iter().enumerate() {
        out.push_str(&format!("    action step{} {{ doc /* {} */ }}\n", i + 1, step));
    }
    out.push_str("}\n");
    out
}

/// Build wizard steps for interactive verification case creation.
pub fn build_verify_add_wizard(model: Option<&sysml_core::model::Model>) -> Vec<sysml_core::interactive::WizardStep> {
    use sysml_core::interactive::*;

    let method_choices = if let Some(m) = model {
        let choices = sysml_core::query::get_enum_choices(m, "VerificationMethod");
        if !choices.is_empty() {
            choices.into_iter().map(|(v, _)| ChoiceOption::new(&v, &v)).collect()
        } else {
            default_method_choices()
        }
    } else {
        default_method_choices()
    };

    vec![
        WizardStep::string("name", "Verification case name")
            .with_explanation("Name for the verification case definition."),
        WizardStep::string("requirements", "Requirements to verify (comma-separated)")
            .with_explanation("Which requirement definitions does this test verify?"),
        WizardStep {
            id: "method".into(),
            prompt: "Verification method".into(),
            explanation: Some("How will this be verified?".into()),
            kind: PromptKind::Choice(method_choices),
            required: true,
            default: None,
        },
        WizardStep::string("steps", "Test steps (comma-separated)")
            .with_explanation("Brief description of each test step.")
            .optional(),
    ]
}

/// Interpret wizard results into (element_name, sysml_text).
pub fn interpret_verify_add_wizard(result: &sysml_core::interactive::WizardResult) -> Option<(String, String)> {
    let name = result.get_string("name")?;
    let reqs_str = result.get_string("requirements")?;
    let method = result.get_string("method")?;
    let requirements: Vec<String> = reqs_str.split(',').map(|s| s.trim().to_string()).collect();
    let steps: Vec<String> = result
        .get_string("steps")
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let sysml = verification_case_to_sysml(name, &requirements, method, &steps);
    Some((name.to_string(), sysml))
}

fn default_method_choices() -> Vec<sysml_core::interactive::ChoiceOption> {
    use sysml_core::interactive::ChoiceOption;
    vec![
        ChoiceOption::new("test", "test"),
        ChoiceOption::new("analysis", "analysis"),
        ChoiceOption::new("inspection", "inspection"),
        ChoiceOption::new("demonstration", "demonstration"),
    ]
}

// ========================================================================
// Tests
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::interactive::PromptKind;
    use sysml_core::model::{Definition, Model, Span, Usage, Verification};

    // -- helpers --------------------------------------------------------

    fn empty_span() -> Span {
        Span::default()
    }

    fn make_req_def(name: &str) -> Definition {
        Definition {
            kind: DefKind::Requirement,
            name: name.to_string(),
            super_type: None,
            span: empty_span(),
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
        }
    }

    fn make_verif_def(name: &str) -> Definition {
        Definition {
            kind: DefKind::Verification,
            name: name.to_string(),
            super_type: None,
            span: empty_span(),
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
        }
    }

    fn make_usage(name: &str, kind: &str, parent: &str) -> Usage {
        Usage {
            kind: kind.to_string(),
            name: name.to_string(),
            type_ref: None,
            span: empty_span(),
            direction: None,
            is_conjugated: false,
            parent_def: Some(parent.to_string()),
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        }
    }

    fn make_model_with_verification() -> Model {
        let mut model = Model::new("test.sysml".to_string());

        model.definitions.push(make_req_def("StopDistanceReq"));
        model.definitions.push(make_req_def("ResponseTimeReq"));
        model.definitions.push(make_verif_def("BrakeTest"));

        model.verifications.push(Verification {
            requirement: "StopDistanceReq".to_string(),
            by: "BrakeTest".to_string(),
            span: empty_span(),
        });

        // Sub-usages inside BrakeTest.
        model
            .usages
            .push(make_usage("setupVehicle", "action", "BrakeTest"));

        let mut measure = make_usage("measureDistance", "attribute", "BrakeTest");
        measure.type_ref = Some("LengthValue".to_string());
        model.usages.push(measure);

        model
            .usages
            .push(make_usage("evaluateResult", "action", "BrakeTest"));

        // A constraint usage as acceptance criterion.
        let mut constraint = make_usage("distanceCheck", "constraint", "BrakeTest");
        constraint.value_expr = Some("distance <= 40.0".to_string());
        model.usages.push(constraint);

        model
    }

    fn make_execution(result: ExecutionResult) -> VerificationExecution {
        VerificationExecution {
            verification_case: "BrakeTest".to_string(),
            result,
            requirements_verified: vec!["StopDistanceReq".to_string()],
            measurements: vec![Measurement {
                name: "stoppingDistance".to_string(),
                value: 38.5,
                unit: "m".to_string(),
                within_spec: true,
            }],
            notes: "Dry road conditions".to_string(),
        }
    }

    // -- extraction tests -----------------------------------------------

    #[test]
    fn extract_finds_verification_defs() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);

        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "BrakeTest");
    }

    #[test]
    fn extract_links_requirements() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);

        assert_eq!(cases[0].requirements, vec!["StopDistanceReq"]);
    }

    #[test]
    fn extract_collects_steps() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);

        // 4 usages under BrakeTest: setupVehicle, measureDistance,
        // evaluateResult, distanceCheck
        assert_eq!(cases[0].steps.len(), 4);
        assert_eq!(cases[0].steps[0].number, 1);
        assert_eq!(cases[0].steps[0].description, "setupVehicle");
        assert!(!cases[0].steps[0].is_measurement);

        // measureDistance is an attribute usage -> is_measurement
        assert!(cases[0].steps[1].is_measurement);
        assert_eq!(
            cases[0].steps[1].description,
            "measureDistance: LengthValue"
        );
    }

    #[test]
    fn extract_acceptance_criteria() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);

        assert_eq!(cases[0].acceptance_criteria, vec!["distance <= 40.0"]);
    }

    #[test]
    fn extract_empty_model() {
        let model = Model::new("empty.sysml".to_string());
        let cases = extract_verification_cases(&model);
        assert!(cases.is_empty());
    }

    #[test]
    fn extract_no_verify_relationships() {
        let mut model = Model::new("test.sysml".to_string());
        model.definitions.push(make_verif_def("UnlinkedTest"));

        let cases = extract_verification_cases(&model);
        assert_eq!(cases.len(), 1);
        assert!(cases[0].requirements.is_empty());
    }

    // -- execution result tests -----------------------------------------

    #[test]
    fn execution_result_labels() {
        assert_eq!(ExecutionResult::Pass.label(), "pass");
        assert_eq!(ExecutionResult::Fail.label(), "fail");
        assert_eq!(
            ExecutionResult::ConditionalPass("minor".into()).label(),
            "conditional_pass"
        );
        assert_eq!(
            ExecutionResult::Blocked("no rig".into()).label(),
            "blocked"
        );
    }

    #[test]
    fn execution_result_is_passing() {
        assert!(ExecutionResult::Pass.is_passing());
        assert!(ExecutionResult::ConditionalPass("x".into()).is_passing());
        assert!(!ExecutionResult::Fail.is_passing());
        assert!(!ExecutionResult::Blocked("x".into()).is_passing());
    }

    // -- record creation tests ------------------------------------------

    #[test]
    fn create_record_has_correct_meta() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        assert_eq!(record.meta.tool, "verify");
        assert_eq!(record.meta.record_type, "execution");
        assert_eq!(record.meta.author, "alice");
        assert!(record.meta.id.starts_with("verify-execution-"));
    }

    #[test]
    fn create_record_has_refs() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        assert_eq!(
            record.refs.get("requirements"),
            Some(&vec!["StopDistanceReq".to_string()])
        );
        assert_eq!(
            record.refs.get("verification_case"),
            Some(&vec!["BrakeTest".to_string()])
        );
    }

    #[test]
    fn create_record_pass_result() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        assert_eq!(
            record.data.get("result"),
            Some(&RecordValue::String("pass".into()))
        );
        // No condition or blocked_reason for a pass.
        assert!(record.data.get("condition").is_none());
        assert!(record.data.get("blocked_reason").is_none());
    }

    #[test]
    fn create_record_conditional_pass() {
        let exec = make_execution(ExecutionResult::ConditionalPass(
            "minor deviation observed".into(),
        ));
        let record = create_execution_record(&exec, "bob");

        assert_eq!(
            record.data.get("result"),
            Some(&RecordValue::String("conditional_pass".into()))
        );
        assert_eq!(
            record.data.get("condition"),
            Some(&RecordValue::String("minor deviation observed".into()))
        );
    }

    #[test]
    fn create_record_blocked() {
        let exec = make_execution(ExecutionResult::Blocked("test rig unavailable".into()));
        let record = create_execution_record(&exec, "carol");

        assert_eq!(
            record.data.get("result"),
            Some(&RecordValue::String("blocked".into()))
        );
        assert_eq!(
            record.data.get("blocked_reason"),
            Some(&RecordValue::String("test rig unavailable".into()))
        );
    }

    #[test]
    fn create_record_includes_measurements() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        let measurements = record.data.get("measurements").unwrap();
        if let RecordValue::Array(arr) = measurements {
            assert_eq!(arr.len(), 1);
            if let RecordValue::Table(tbl) = &arr[0] {
                assert_eq!(
                    tbl.get("name"),
                    Some(&RecordValue::String("stoppingDistance".into()))
                );
                assert_eq!(tbl.get("value"), Some(&RecordValue::Float(38.5)));
                assert_eq!(
                    tbl.get("unit"),
                    Some(&RecordValue::String("m".into()))
                );
                assert_eq!(
                    tbl.get("within_spec"),
                    Some(&RecordValue::Bool(true))
                );
            } else {
                panic!("expected Table in measurements array");
            }
        } else {
            panic!("expected Array for measurements");
        }
    }

    #[test]
    fn create_record_includes_notes() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        assert_eq!(
            record.data.get("notes"),
            Some(&RecordValue::String("Dry road conditions".into()))
        );
    }

    #[test]
    fn create_record_omits_empty_notes() {
        let mut exec = make_execution(ExecutionResult::Pass);
        exec.notes = String::new();
        let record = create_execution_record(&exec, "alice");

        assert!(record.data.get("notes").is_none());
    }

    #[test]
    fn create_record_omits_empty_measurements() {
        let mut exec = make_execution(ExecutionResult::Pass);
        exec.measurements.clear();
        let record = create_execution_record(&exec, "alice");

        assert!(record.data.get("measurements").is_none());
    }

    #[test]
    fn record_round_trips_through_toml() {
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");
        let toml_str = record.to_toml_string();

        let parsed = RecordEnvelope::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.meta.tool, "verify");
        assert_eq!(parsed.meta.record_type, "execution");
        assert_eq!(parsed.refs, record.refs);
        assert_eq!(
            parsed.data.get("result"),
            Some(&RecordValue::String("pass".into()))
        );
    }

    // -- wizard tests ---------------------------------------------------

    #[test]
    fn wizard_has_ready_step() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);
        let steps = build_wizard_steps(&cases[0]);

        assert_eq!(steps[0].id, "ready");
        assert!(matches!(steps[0].kind, PromptKind::Confirm));
    }

    #[test]
    fn wizard_measurement_step_has_value_unit_spec() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);
        let steps = build_wizard_steps(&cases[0]);

        // Find the measurement step (step 2 = measureDistance).
        let value_step = steps
            .iter()
            .find(|s| s.id == "step_2_value")
            .expect("should have measurement value step");
        assert!(matches!(value_step.kind, PromptKind::Number { .. }));

        let unit_step = steps
            .iter()
            .find(|s| s.id == "step_2_unit")
            .expect("should have measurement unit step");
        assert!(matches!(unit_step.kind, PromptKind::String));

        let spec_step = steps
            .iter()
            .find(|s| s.id == "step_2_in_spec")
            .expect("should have in-spec confirmation step");
        assert!(matches!(spec_step.kind, PromptKind::Confirm));
    }

    #[test]
    fn wizard_non_measurement_step_is_confirm() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);
        let steps = build_wizard_steps(&cases[0]);

        let step1 = steps
            .iter()
            .find(|s| s.id == "step_1")
            .expect("should have step_1");
        assert!(matches!(step1.kind, PromptKind::Confirm));
    }

    #[test]
    fn wizard_ends_with_result_choice() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);
        let steps = build_wizard_steps(&cases[0]);

        let result_step = steps
            .iter()
            .find(|s| s.id == "overall_result")
            .expect("should have overall_result step");
        if let PromptKind::Choice(opts) = &result_step.kind {
            assert_eq!(opts.len(), 4);
            let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
            assert!(values.contains(&"pass"));
            assert!(values.contains(&"fail"));
            assert!(values.contains(&"conditional_pass"));
            assert!(values.contains(&"blocked"));
        } else {
            panic!("expected Choice for overall_result");
        }
    }

    #[test]
    fn wizard_has_observations_step() {
        let model = make_model_with_verification();
        let cases = extract_verification_cases(&model);
        let steps = build_wizard_steps(&cases[0]);

        let obs = steps
            .iter()
            .find(|s| s.id == "observations")
            .expect("should have observations step");
        assert!(!obs.required); // optional
    }

    #[test]
    fn wizard_empty_case_has_minimal_steps() {
        let vc = VerificationCase {
            name: "EmptyTest".to_string(),
            qualified_name: None,
            requirements: vec![],
            steps: vec![],
            acceptance_criteria: vec![],
        };
        let steps = build_wizard_steps(&vc);
        // ready + observations + overall_result + result_detail = 4
        assert_eq!(steps.len(), 4);
    }

    // -- coverage tests -------------------------------------------------

    #[test]
    fn coverage_no_requirements() {
        let model = Model::new("empty.sysml".to_string());
        let report = coverage_from_records(&[model], &[]);

        assert_eq!(report.total_requirements, 0);
        assert_eq!(report.verified_requirements, 0);
        assert_eq!(report.passing_requirements, 0);
        assert!(report.items.is_empty());
        // Edge case: 0 requirements => 100% by convention.
        assert!((report.coverage_pct() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn coverage_unverified_requirements() {
        let model = make_model_with_verification();
        let report = coverage_from_records(&[model], &[]);

        assert_eq!(report.total_requirements, 2);
        // Only StopDistanceReq has a verify link.
        assert_eq!(report.verified_requirements, 1);
        assert_eq!(report.passing_requirements, 0);
    }

    #[test]
    fn coverage_with_passing_record() {
        let model = make_model_with_verification();
        let exec = make_execution(ExecutionResult::Pass);
        let record = create_execution_record(&exec, "alice");

        let report = coverage_from_records(&[model], &[record]);

        assert_eq!(report.total_requirements, 2);
        assert_eq!(report.verified_requirements, 1);
        assert_eq!(report.passing_requirements, 1);

        let stop_item = report
            .items
            .iter()
            .find(|i| i.requirement == "StopDistanceReq")
            .unwrap();
        assert!(stop_item.is_verified);
        assert_eq!(stop_item.last_result, Some(ExecutionResult::Pass));
    }

    #[test]
    fn coverage_with_failing_record() {
        let model = make_model_with_verification();
        let exec = make_execution(ExecutionResult::Fail);
        let record = create_execution_record(&exec, "bob");

        let report = coverage_from_records(&[model], &[record]);

        assert_eq!(report.passing_requirements, 0);

        let stop_item = report
            .items
            .iter()
            .find(|i| i.requirement == "StopDistanceReq")
            .unwrap();
        assert!(!stop_item.is_verified);
        assert_eq!(stop_item.last_result, Some(ExecutionResult::Fail));
    }

    #[test]
    fn coverage_best_result_wins() {
        let model = make_model_with_verification();

        let fail_exec = make_execution(ExecutionResult::Fail);
        let fail_record = create_execution_record(&fail_exec, "bob");

        let pass_exec = make_execution(ExecutionResult::Pass);
        let pass_record = create_execution_record(&pass_exec, "alice");

        let report =
            coverage_from_records(&[model], &[fail_record, pass_record]);

        let stop_item = report
            .items
            .iter()
            .find(|i| i.requirement == "StopDistanceReq")
            .unwrap();
        // Pass ranks higher than Fail, so it should be kept.
        assert_eq!(stop_item.last_result, Some(ExecutionResult::Pass));
        assert!(stop_item.is_verified);
    }

    #[test]
    fn coverage_pct_calculation() {
        let report = CoverageReport {
            total_requirements: 4,
            verified_requirements: 3,
            passing_requirements: 2,
            items: vec![],
        };

        assert!((report.coverage_pct() - 0.75).abs() < f64::EPSILON);
        assert!((report.pass_pct() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn coverage_multiple_models() {
        let model1 = make_model_with_verification();
        let mut model2 = Model::new("other.sysml".to_string());
        model2.definitions.push(make_req_def("ThermalReq"));

        let report = coverage_from_records(&[model1, model2], &[]);

        assert_eq!(report.total_requirements, 3);
        assert_eq!(report.verified_requirements, 1);
    }

    #[test]
    fn coverage_deduplicates_across_models() {
        // Same requirement name in two models should only appear once.
        let mut model1 = Model::new("a.sysml".to_string());
        model1.definitions.push(make_req_def("SharedReq"));

        let mut model2 = Model::new("b.sysml".to_string());
        model2.definitions.push(make_req_def("SharedReq"));

        let report = coverage_from_records(&[model1, model2], &[]);

        assert_eq!(report.total_requirements, 1);
    }

    // -- parse_execution_result tests -----------------------------------

    #[test]
    fn parse_known_results() {
        assert_eq!(parse_execution_result("pass"), ExecutionResult::Pass);
        assert_eq!(parse_execution_result("fail"), ExecutionResult::Fail);
        assert_eq!(
            parse_execution_result("conditional_pass"),
            ExecutionResult::ConditionalPass(String::new())
        );
        assert_eq!(
            parse_execution_result("blocked"),
            ExecutionResult::Blocked(String::new())
        );
    }

    #[test]
    fn parse_unknown_result_defaults_to_fail() {
        assert_eq!(
            parse_execution_result("unknown"),
            ExecutionResult::Fail
        );
    }

    // -- result_rank tests ----------------------------------------------

    #[test]
    fn rank_ordering() {
        assert!(result_rank(&ExecutionResult::Pass)
            > result_rank(&ExecutionResult::ConditionalPass(String::new())));
        assert!(result_rank(&ExecutionResult::ConditionalPass(String::new()))
            > result_rank(&ExecutionResult::Fail));
        assert!(result_rank(&ExecutionResult::Fail)
            > result_rank(&ExecutionResult::Blocked(String::new())));
    }

    // -- SysML generator tests ----------------------------------------------

    #[test]
    fn verification_case_to_sysml_basic() {
        let sysml = verification_case_to_sysml(
            "TestBrakes",
            &["BrakeReq".to_string()],
            "test",
            &["Apply brake pedal".to_string(), "Measure stopping distance".to_string()],
        );
        assert!(sysml.contains("verification def TestBrakes {"));
        assert!(sysml.contains("verify requirement BrakeReq;"));
        assert!(sysml.contains("VerificationMethod::test"));
        assert!(sysml.contains("step1"));
        assert!(sysml.contains("step2"));
    }

    #[test]
    fn build_verify_wizard_defaults() {
        let steps = build_verify_add_wizard(None);
        assert!(steps.len() >= 3);
        assert_eq!(steps[0].id, "name");
        assert_eq!(steps[1].id, "requirements");
        assert_eq!(steps[2].id, "method");
    }

    #[test]
    fn interpret_verify_wizard() {
        use sysml_core::interactive::*;
        let mut result = WizardResult::new();
        result.set("name", WizardAnswer::String("TestBrakes".into()));
        result.set("requirements", WizardAnswer::String("BrakeReq,SafetyReq".into()));
        result.set("method", WizardAnswer::String("test".into()));
        result.set("steps", WizardAnswer::String("Push pedal,Check distance".into()));
        let (name, sysml) = interpret_verify_add_wizard(&result).unwrap();
        assert_eq!(name, "TestBrakes");
        assert!(sysml.contains("verify requirement BrakeReq;"));
        assert!(sysml.contains("verify requirement SafetyReq;"));
    }
}
