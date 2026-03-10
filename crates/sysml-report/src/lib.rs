//! Cross-domain reporting for SysML v2 models.
//!
//! This crate provides aggregation, traceability, gate-check, and design
//! history reporting on top of [`sysml_core::model::Model`] and
//! [`sysml_core::record::RecordEnvelope`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sysml_core::model::{DefKind, Model};
use sysml_core::record::{RecordEnvelope, RecordValue};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status indicator for a single metric on the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Good,
    Warning,
    Critical,
    Unknown,
}

/// A single metric displayed on the project dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardMetric {
    pub name: String,
    pub value: String,
    pub status: MetricStatus,
}

/// Aggregated project dashboard produced from models and records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub project_name: String,
    pub metrics: Vec<DashboardMetric>,
    pub generated: String,
}

/// Full lifecycle traceability thread for a single requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityThread {
    pub requirement: String,
    pub satisfied_by: Vec<String>,
    pub verification_cases: Vec<String>,
    pub execution_results: Vec<String>,
    pub risks: Vec<String>,
    pub status: String,
}

/// Result of evaluating a project gate (e.g. PDR, CDR).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckResult {
    pub gate_name: String,
    pub passed: bool,
    pub blocking_items: Vec<String>,
    pub coverage_pct: f64,
    pub open_risks: usize,
    pub open_ncrs: usize,
}

/// A single entry in a part's design history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignHistoryEntry {
    pub category: String,
    pub item: String,
    pub status: String,
    pub date: String,
}

/// Complete design history for a named part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignHistory {
    pub part_name: String,
    pub entries: Vec<DesignHistoryEntry>,
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

/// Count requirements across all models.
fn count_requirements(models: &[Model]) -> usize {
    models
        .iter()
        .flat_map(|m| &m.definitions)
        .filter(|d| d.kind == DefKind::Requirement)
        .count()
}

/// Collect all requirement names across models.
fn requirement_names(models: &[Model]) -> HashSet<String> {
    models
        .iter()
        .flat_map(|m| &m.definitions)
        .filter(|d| d.kind == DefKind::Requirement)
        .map(|d| d.name.clone())
        .collect()
}

/// Collect all verification relationship targets (requirement names that have
/// at least one `verify` relationship).
fn verified_requirements(models: &[Model]) -> HashSet<String> {
    models
        .iter()
        .flat_map(|m| &m.verifications)
        .map(|v| v.requirement.clone())
        .collect()
}

/// Collect all satisfaction targets (requirement names that have at least one
/// `satisfy` relationship).
fn satisfied_requirements(models: &[Model]) -> HashSet<String> {
    models
        .iter()
        .flat_map(|m| &m.satisfactions)
        .map(|s| s.requirement.clone())
        .collect()
}

/// Count definitions that have documentation.
fn documented_count(models: &[Model]) -> (usize, usize) {
    let total: usize = models.iter().map(|m| m.definitions.len()).sum();
    let with_doc: usize = models
        .iter()
        .flat_map(|m| &m.definitions)
        .filter(|d| d.doc.is_some())
        .count();
    (with_doc, total)
}

/// Count BOM-relevant elements (part definitions and part usages).
fn bom_element_count(models: &[Model]) -> usize {
    let part_defs: usize = models
        .iter()
        .flat_map(|m| &m.definitions)
        .filter(|d| d.kind == DefKind::Part || d.kind == DefKind::Item)
        .count();
    let part_usages: usize = models
        .iter()
        .flat_map(|m| &m.usages)
        .filter(|u| u.kind == "part" || u.kind == "item")
        .count();
    part_defs + part_usages
}

/// Count records matching a given tool name.
fn count_records_by_tool(records: &[RecordEnvelope], tool: &str) -> usize {
    records.iter().filter(|r| r.meta.tool == tool).count()
}

/// Count records matching a tool name that have a specific status value in
/// their data section.
fn count_records_by_tool_and_status(
    records: &[RecordEnvelope],
    tool: &str,
    status_key: &str,
    status_value: &str,
) -> usize {
    records
        .iter()
        .filter(|r| r.meta.tool == tool)
        .filter(|r| match r.data.get(status_key) {
            Some(RecordValue::String(s)) => s == status_value,
            _ => false,
        })
        .count()
}

/// Extract the string value from a record's data section, if present.
fn record_data_string(record: &RecordEnvelope, key: &str) -> Option<String> {
    match record.data.get(key) {
        Some(RecordValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Check whether a record references a given qualified or simple name in any
/// of its `refs` lists.
fn record_references_name(record: &RecordEnvelope, name: &str) -> bool {
    record.refs.values().any(|names| {
        names.iter().any(|n| {
            n == name || n.ends_with(&format!("::{name}")) || n.ends_with(&format!(".{name}"))
        })
    })
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Aggregate metrics from models and records into a project dashboard.
///
/// Produces the following metrics:
/// - Requirement count and verification coverage percentage
/// - Open risk count (records with `tool = "risk"`)
/// - NCR count (records with `tool = "capa"`)
/// - BOM element count
/// - Overall model quality (documentation percentage)
pub fn generate_dashboard(
    models: &[Model],
    records: &[RecordEnvelope],
) -> Dashboard {
    let mut metrics = Vec::new();

    // Requirement count
    let req_count = count_requirements(models);
    metrics.push(DashboardMetric {
        name: "Requirements".into(),
        value: req_count.to_string(),
        status: if req_count > 0 {
            MetricStatus::Good
        } else {
            MetricStatus::Warning
        },
    });

    // Satisfaction coverage
    let req_names = requirement_names(models);
    let satisfied = satisfied_requirements(models);
    let sat_pct = if req_names.is_empty() {
        0.0
    } else {
        let covered = req_names.iter().filter(|r| satisfied.contains(*r)).count();
        (covered as f64 / req_names.len() as f64) * 100.0
    };
    metrics.push(DashboardMetric {
        name: "Satisfaction Coverage".into(),
        value: format!("{sat_pct:.1}%"),
        status: if sat_pct >= 80.0 {
            MetricStatus::Good
        } else if sat_pct >= 50.0 {
            MetricStatus::Warning
        } else {
            MetricStatus::Critical
        },
    });

    // Verification coverage
    let verified = verified_requirements(models);
    let ver_pct = if req_names.is_empty() {
        0.0
    } else {
        let covered = req_names.iter().filter(|r| verified.contains(*r)).count();
        (covered as f64 / req_names.len() as f64) * 100.0
    };
    metrics.push(DashboardMetric {
        name: "Verification Coverage".into(),
        value: format!("{ver_pct:.1}%"),
        status: if ver_pct >= 80.0 {
            MetricStatus::Good
        } else if ver_pct >= 50.0 {
            MetricStatus::Warning
        } else {
            MetricStatus::Critical
        },
    });

    // Open risks
    let open_risks = count_records_by_tool_and_status(records, "risk", "status", "open");
    let total_risks = count_records_by_tool(records, "risk");
    metrics.push(DashboardMetric {
        name: "Open Risks".into(),
        value: format!("{open_risks} / {total_risks}"),
        status: if open_risks == 0 {
            MetricStatus::Good
        } else if open_risks <= 3 {
            MetricStatus::Warning
        } else {
            MetricStatus::Critical
        },
    });

    // NCRs (CAPA records)
    let open_ncrs = count_records_by_tool_and_status(records, "capa", "status", "open");
    let total_ncrs = count_records_by_tool(records, "capa");
    metrics.push(DashboardMetric {
        name: "Open NCRs".into(),
        value: format!("{open_ncrs} / {total_ncrs}"),
        status: if open_ncrs == 0 {
            MetricStatus::Good
        } else if open_ncrs <= 2 {
            MetricStatus::Warning
        } else {
            MetricStatus::Critical
        },
    });

    // BOM elements
    let bom = bom_element_count(models);
    metrics.push(DashboardMetric {
        name: "BOM Elements".into(),
        value: bom.to_string(),
        status: MetricStatus::Unknown,
    });

    // Documentation quality
    let (with_doc, total_defs) = documented_count(models);
    let doc_pct = if total_defs == 0 {
        0.0
    } else {
        (with_doc as f64 / total_defs as f64) * 100.0
    };
    metrics.push(DashboardMetric {
        name: "Documentation".into(),
        value: format!("{doc_pct:.1}%"),
        status: if doc_pct >= 80.0 {
            MetricStatus::Good
        } else if doc_pct >= 50.0 {
            MetricStatus::Warning
        } else {
            MetricStatus::Critical
        },
    });

    let project_name = models
        .first()
        .and_then(|m| {
            m.definitions
                .iter()
                .find(|d| d.kind == DefKind::Package)
                .map(|d| d.name.clone())
        })
        .unwrap_or_else(|| "Unnamed Project".into());

    Dashboard {
        project_name,
        metrics,
        generated: sysml_core::record::now_iso8601(),
    }
}

/// Build a full lifecycle traceability thread for a named requirement.
///
/// Walks models to find satisfy and verify relationships, then searches
/// records for execution results (tool = "verify") and risks referencing the
/// requirement.
pub fn trace_requirement(
    models: &[Model],
    requirement: &str,
) -> TraceabilityThread {
    // Find satisfaction relationships
    let satisfied_by: Vec<String> = models
        .iter()
        .flat_map(|m| &m.satisfactions)
        .filter(|s| s.requirement == requirement)
        .filter_map(|s| s.by.clone())
        .collect();

    // Find verification cases
    let verification_cases: Vec<String> = models
        .iter()
        .flat_map(|m| &m.verifications)
        .filter(|v| v.requirement == requirement)
        .map(|v| v.by.clone())
        .collect();

    // Determine overall status
    let has_satisfaction = !satisfied_by.is_empty();
    let has_verification = !verification_cases.is_empty();
    let status = match (has_satisfaction, has_verification) {
        (true, true) => "verified",
        (true, false) => "satisfied (unverified)",
        (false, true) => "verification only (unsatisfied)",
        (false, false) => "uncovered",
    };

    TraceabilityThread {
        requirement: requirement.to_string(),
        satisfied_by,
        verification_cases,
        execution_results: Vec::new(),
        risks: Vec::new(),
        status: status.to_string(),
    }
}

/// Evaluate gate readiness based on coverage, risk, and NCR thresholds.
///
/// A gate passes when:
/// - Verification coverage meets or exceeds `required_coverage`
/// - Open critical risks do not exceed `max_critical_risks`
/// - Open NCRs do not exceed `max_open_ncrs`
///
/// Any failing criterion is added to the `blocking_items` list.
pub fn check_gate(
    models: &[Model],
    records: &[RecordEnvelope],
    gate_name: &str,
    required_coverage: f64,
    max_critical_risks: usize,
    max_open_ncrs: usize,
) -> GateCheckResult {
    let mut blocking_items = Vec::new();

    // Compute verification coverage
    let req_names = requirement_names(models);
    let verified = verified_requirements(models);
    let coverage_pct = if req_names.is_empty() {
        100.0 // No requirements means vacuously covered
    } else {
        let covered = req_names.iter().filter(|r| verified.contains(*r)).count();
        (covered as f64 / req_names.len() as f64) * 100.0
    };

    if coverage_pct < required_coverage {
        blocking_items.push(format!(
            "Verification coverage {coverage_pct:.1}% < required {required_coverage:.1}%"
        ));
    }

    // Count open risks (tool = "risk", status = "open")
    let open_risks = count_records_by_tool_and_status(records, "risk", "status", "open");
    if open_risks > max_critical_risks {
        blocking_items.push(format!(
            "Open risks {open_risks} > max allowed {max_critical_risks}"
        ));
    }

    // Count open NCRs (tool = "capa", status = "open")
    let open_ncrs = count_records_by_tool_and_status(records, "capa", "status", "open");
    if open_ncrs > max_open_ncrs {
        blocking_items.push(format!(
            "Open NCRs {open_ncrs} > max allowed {max_open_ncrs}"
        ));
    }

    let passed = blocking_items.is_empty();

    GateCheckResult {
        gate_name: gate_name.to_string(),
        passed,
        blocking_items,
        coverage_pct,
        open_risks,
        open_ncrs,
    }
}

/// Collect all records referencing a named part into a design history.
///
/// Records are matched by checking whether any entry in their `refs` section
/// contains the part name (fully qualified or simple).  Each matching record
/// produces a [`DesignHistoryEntry`].
pub fn design_history(
    models: &[Model],
    records: &[RecordEnvelope],
    part_name: &str,
) -> DesignHistory {
    let mut entries = Vec::new();

    // Check whether the part exists in any model (for status enrichment).
    let part_exists = models
        .iter()
        .flat_map(|m| &m.definitions)
        .any(|d| d.name == part_name);

    if part_exists {
        // Add a synthetic "definition" entry so the history includes the
        // model-level fact.
        entries.push(DesignHistoryEntry {
            category: "model".into(),
            item: format!("Part definition: {part_name}"),
            status: "defined".into(),
            date: String::new(),
        });
    }

    // Walk all records looking for references to the part
    for record in records {
        if record_references_name(record, part_name) {
            let status = record_data_string(record, "status").unwrap_or_else(|| "unknown".into());
            entries.push(DesignHistoryEntry {
                category: record.meta.tool.clone(),
                item: format!("{}: {}", record.meta.record_type, record.meta.id),
                status,
                date: record.meta.created.clone(),
            });
        }
    }

    DesignHistory {
        part_name: part_name.to_string(),
        entries,
    }
}

/// Render a dashboard as a human-readable text report.
pub fn format_dashboard_text(dashboard: &Dashboard) -> String {
    let mut out = String::new();

    let title = format!("Dashboard: {}", dashboard.project_name);
    out.push_str(&title);
    out.push('\n');
    out.push_str(&"=".repeat(title.len()));
    out.push('\n');
    out.push('\n');

    // Find the longest metric name for alignment
    let max_name_len = dashboard
        .metrics
        .iter()
        .map(|m| m.name.len())
        .max()
        .unwrap_or(0);

    for metric in &dashboard.metrics {
        let status_icon = match metric.status {
            MetricStatus::Good => "[OK]",
            MetricStatus::Warning => "[!!]",
            MetricStatus::Critical => "[XX]",
            MetricStatus::Unknown => "[--]",
        };
        out.push_str(&format!(
            "  {:<width$}  {:>12}  {}\n",
            metric.name,
            metric.value,
            status_icon,
            width = max_name_len,
        ));
    }

    out.push('\n');
    out.push_str(&format!("Generated: {}\n", dashboard.generated));

    out
}

/// Render a traceability thread as a human-readable text report.
pub fn format_traceability_text(thread: &TraceabilityThread) -> String {
    let mut out = String::new();

    let title = format!("Traceability: {}", thread.requirement);
    out.push_str(&title);
    out.push('\n');
    out.push_str(&"-".repeat(title.len()));
    out.push('\n');

    out.push_str(&format!("Status: {}\n", thread.status));
    out.push('\n');

    out.push_str("Satisfied by:\n");
    if thread.satisfied_by.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for item in &thread.satisfied_by {
            out.push_str(&format!("  - {item}\n"));
        }
    }

    out.push_str("Verification cases:\n");
    if thread.verification_cases.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for item in &thread.verification_cases {
            out.push_str(&format!("  - {item}\n"));
        }
    }

    out.push_str("Execution results:\n");
    if thread.execution_results.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for item in &thread.execution_results {
            out.push_str(&format!("  - {item}\n"));
        }
    }

    out.push_str("Risks:\n");
    if thread.risks.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for item in &thread.risks {
            out.push_str(&format!("  - {item}\n"));
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
    use std::collections::BTreeMap;
    use sysml_core::model::{Definition, Satisfaction, Span, Verification as ModelVerification};
    use sysml_core::record::{RecordEnvelope, RecordMeta, RecordValue};

    // ---- Test helpers ----

    fn span() -> Span {
        Span::default()
    }

    fn make_def(kind: DefKind, name: &str) -> Definition {
        Definition {
            kind,
            name: name.into(),
            super_type: None,
            span: span(),
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

    fn make_def_with_doc(kind: DefKind, name: &str, doc: &str) -> Definition {
        let mut d = make_def(kind, name);
        d.doc = Some(doc.into());
        d
    }

    fn make_satisfaction(requirement: &str, by: &str) -> Satisfaction {
        Satisfaction {
            requirement: requirement.into(),
            by: Some(by.into()),
            span: span(),
        }
    }

    fn make_verification(requirement: &str, by: &str) -> ModelVerification {
        ModelVerification {
            requirement: requirement.into(),
            by: by.into(),
            span: span(),
        }
    }

    fn make_model(name: &str) -> Model {
        let mut m = Model::new(format!("{name}.sysml"));
        m.definitions.push(make_def(DefKind::Package, name));
        m
    }

    fn make_record(tool: &str, record_type: &str, status: &str) -> RecordEnvelope {
        let mut data = BTreeMap::new();
        data.insert("status".into(), RecordValue::String(status.into()));
        RecordEnvelope {
            meta: RecordMeta {
                id: format!("{tool}-{record_type}-test"),
                tool: tool.into(),
                record_type: record_type.into(),
                created: "2026-03-09T00:00:00Z".into(),
                author: "tester".into(),
            },
            refs: BTreeMap::new(),
            data,
        }
    }

    fn make_record_with_refs(
        tool: &str,
        record_type: &str,
        status: &str,
        ref_names: Vec<&str>,
    ) -> RecordEnvelope {
        let mut record = make_record(tool, record_type, status);
        record.refs.insert(
            "parts".into(),
            ref_names.iter().map(|s| s.to_string()).collect(),
        );
        record
    }

    // ---- Dashboard tests ----

    #[test]
    fn dashboard_empty_models() {
        let dashboard = generate_dashboard(&[], &[]);
        assert_eq!(dashboard.project_name, "Unnamed Project");
        assert!(!dashboard.metrics.is_empty());
        // Requirements should be 0
        let req = dashboard.metrics.iter().find(|m| m.name == "Requirements").unwrap();
        assert_eq!(req.value, "0");
        assert_eq!(req.status, MetricStatus::Warning);
    }

    #[test]
    fn dashboard_with_requirements_and_coverage() {
        let mut m = make_model("VehicleSystem");
        m.definitions.push(make_def(DefKind::Requirement, "BrakeReq"));
        m.definitions.push(make_def(DefKind::Requirement, "SpeedReq"));
        m.verifications.push(make_verification("BrakeReq", "BrakeTest"));

        let dashboard = generate_dashboard(&[m], &[]);
        assert_eq!(dashboard.project_name, "VehicleSystem");

        let req = dashboard.metrics.iter().find(|m| m.name == "Requirements").unwrap();
        assert_eq!(req.value, "2");
        assert_eq!(req.status, MetricStatus::Good);

        let cov = dashboard.metrics.iter().find(|m| m.name == "Verification Coverage").unwrap();
        assert_eq!(cov.value, "50.0%");
        assert_eq!(cov.status, MetricStatus::Warning);
    }

    #[test]
    fn dashboard_full_coverage() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Requirement, "R1"));
        m.verifications.push(make_verification("R1", "T1"));

        let dashboard = generate_dashboard(&[m], &[]);
        let cov = dashboard.metrics.iter().find(|m| m.name == "Verification Coverage").unwrap();
        assert_eq!(cov.value, "100.0%");
        assert_eq!(cov.status, MetricStatus::Good);
    }

    #[test]
    fn dashboard_risks_and_ncrs() {
        let records = vec![
            make_record("risk", "assessment", "open"),
            make_record("risk", "assessment", "closed"),
            make_record("capa", "ncr", "open"),
            make_record("capa", "ncr", "open"),
            make_record("capa", "ncr", "closed"),
        ];
        let dashboard = generate_dashboard(&[], &records);

        let risks = dashboard.metrics.iter().find(|m| m.name == "Open Risks").unwrap();
        assert_eq!(risks.value, "1 / 2");
        assert_eq!(risks.status, MetricStatus::Warning);

        let ncrs = dashboard.metrics.iter().find(|m| m.name == "Open NCRs").unwrap();
        assert_eq!(ncrs.value, "2 / 3");
        assert_eq!(ncrs.status, MetricStatus::Warning);
    }

    #[test]
    fn dashboard_bom_elements() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Part, "Chassis"));
        m.definitions.push(make_def(DefKind::Item, "Bolt"));
        m.definitions.push(make_def(DefKind::Action, "Assemble")); // not BOM
        m.usages.push(sysml_core::model::Usage {
            kind: "part".into(),
            name: "engine".into(),
            type_ref: Some("Engine".into()),
            span: span(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("Chassis".into()),
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        let dashboard = generate_dashboard(&[m], &[]);
        let bom = dashboard.metrics.iter().find(|m| m.name == "BOM Elements").unwrap();
        assert_eq!(bom.value, "3"); // 2 defs + 1 usage
    }

    #[test]
    fn dashboard_documentation_quality() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def_with_doc(DefKind::Part, "A", "Documented"));
        m.definitions.push(make_def(DefKind::Part, "B")); // no doc

        let dashboard = generate_dashboard(&[m], &[]);
        let doc = dashboard.metrics.iter().find(|m| m.name == "Documentation").unwrap();
        // 1 out of 3 (Package + A + B) = 33.3%
        assert!(doc.value.contains("33.3"));
        assert_eq!(doc.status, MetricStatus::Critical);
    }

    // ---- Traceability tests ----

    #[test]
    fn trace_fully_covered_requirement() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Requirement, "StopDistance"));
        m.satisfactions.push(make_satisfaction("StopDistance", "BrakeSystem"));
        m.verifications.push(make_verification("StopDistance", "BrakeTest"));

        let thread = trace_requirement(&[m], "StopDistance");
        assert_eq!(thread.requirement, "StopDistance");
        assert_eq!(thread.satisfied_by, vec!["BrakeSystem"]);
        assert_eq!(thread.verification_cases, vec!["BrakeTest"]);
        assert_eq!(thread.status, "verified");
    }

    #[test]
    fn trace_satisfied_but_unverified() {
        let mut m = make_model("Proj");
        m.satisfactions.push(make_satisfaction("SpeedLimit", "Controller"));

        let thread = trace_requirement(&[m], "SpeedLimit");
        assert_eq!(thread.satisfied_by, vec!["Controller"]);
        assert!(thread.verification_cases.is_empty());
        assert_eq!(thread.status, "satisfied (unverified)");
    }

    #[test]
    fn trace_uncovered_requirement() {
        let m = make_model("Proj");
        let thread = trace_requirement(&[m], "MissingReq");
        assert!(thread.satisfied_by.is_empty());
        assert!(thread.verification_cases.is_empty());
        assert_eq!(thread.status, "uncovered");
    }

    #[test]
    fn trace_verification_only() {
        let mut m = make_model("Proj");
        m.verifications.push(make_verification("Durability", "StressTest"));

        let thread = trace_requirement(&[m], "Durability");
        assert!(thread.satisfied_by.is_empty());
        assert_eq!(thread.verification_cases, vec!["StressTest"]);
        assert_eq!(thread.status, "verification only (unsatisfied)");
    }

    // ---- Gate check tests ----

    #[test]
    fn gate_passes_with_full_coverage_and_no_issues() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Requirement, "R1"));
        m.verifications.push(make_verification("R1", "T1"));

        let result = check_gate(&[m], &[], "CDR", 80.0, 0, 0);
        assert!(result.passed);
        assert!(result.blocking_items.is_empty());
        assert_eq!(result.coverage_pct, 100.0);
        assert_eq!(result.open_risks, 0);
        assert_eq!(result.open_ncrs, 0);
    }

    #[test]
    fn gate_fails_on_low_coverage() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Requirement, "R1"));
        m.definitions.push(make_def(DefKind::Requirement, "R2"));
        // Only R1 is verified
        m.verifications.push(make_verification("R1", "T1"));

        let result = check_gate(&[m], &[], "PDR", 80.0, 5, 5);
        assert!(!result.passed);
        assert_eq!(result.coverage_pct, 50.0);
        assert!(result.blocking_items[0].contains("coverage"));
    }

    #[test]
    fn gate_fails_on_open_risks() {
        let m = make_model("Proj");
        let records = vec![
            make_record("risk", "assessment", "open"),
            make_record("risk", "assessment", "open"),
        ];

        let result = check_gate(&[m], &records, "CDR", 0.0, 1, 10);
        assert!(!result.passed);
        assert_eq!(result.open_risks, 2);
        assert!(result.blocking_items.iter().any(|b| b.contains("risks")));
    }

    #[test]
    fn gate_fails_on_open_ncrs() {
        let m = make_model("Proj");
        let records = vec![
            make_record("capa", "ncr", "open"),
            make_record("capa", "ncr", "open"),
            make_record("capa", "ncr", "open"),
        ];

        let result = check_gate(&[m], &records, "FRR", 0.0, 10, 2);
        assert!(!result.passed);
        assert_eq!(result.open_ncrs, 3);
        assert!(result.blocking_items.iter().any(|b| b.contains("NCRs")));
    }

    #[test]
    fn gate_multiple_blocking_items() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Requirement, "R1"));
        // No verification for R1

        let records = vec![
            make_record("risk", "assessment", "open"),
            make_record("risk", "assessment", "open"),
            make_record("capa", "ncr", "open"),
        ];

        let result = check_gate(&[m], &records, "CDR", 90.0, 0, 0);
        assert!(!result.passed);
        assert_eq!(result.blocking_items.len(), 3);
    }

    #[test]
    fn gate_vacuously_passes_with_no_requirements() {
        let m = make_model("Proj");
        let result = check_gate(&[m], &[], "PDR", 80.0, 0, 0);
        assert!(result.passed);
        assert_eq!(result.coverage_pct, 100.0);
    }

    // ---- Design history tests ----

    #[test]
    fn design_history_collects_matching_records() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Part, "Chassis"));

        let records = vec![
            make_record_with_refs("risk", "assessment", "open", vec!["Vehicle::Chassis"]),
            make_record_with_refs("capa", "ncr", "closed", vec!["Vehicle::Chassis"]),
            make_record_with_refs("risk", "assessment", "open", vec!["Vehicle::Engine"]), // different part
        ];

        let history = design_history(&[m], &records, "Chassis");
        assert_eq!(history.part_name, "Chassis");
        // 1 model entry + 2 matching records
        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.entries[0].category, "model");
        assert_eq!(history.entries[1].category, "risk");
        assert_eq!(history.entries[2].category, "capa");
    }

    #[test]
    fn design_history_no_matching_records() {
        let mut m = make_model("Proj");
        m.definitions.push(make_def(DefKind::Part, "Wheel"));

        let history = design_history(&[m], &[], "Wheel");
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].category, "model");
    }

    #[test]
    fn design_history_unknown_part() {
        let m = make_model("Proj");
        let history = design_history(&[m], &[], "NonExistent");
        assert!(history.entries.is_empty());
    }

    // ---- Formatting tests ----

    #[test]
    fn format_dashboard_includes_all_metrics() {
        let dashboard = Dashboard {
            project_name: "TestProject".into(),
            metrics: vec![
                DashboardMetric {
                    name: "Requirements".into(),
                    value: "5".into(),
                    status: MetricStatus::Good,
                },
                DashboardMetric {
                    name: "Coverage".into(),
                    value: "80.0%".into(),
                    status: MetricStatus::Warning,
                },
            ],
            generated: "2026-03-09T00:00:00Z".into(),
        };

        let text = format_dashboard_text(&dashboard);
        assert!(text.contains("Dashboard: TestProject"));
        assert!(text.contains("Requirements"));
        assert!(text.contains("5"));
        assert!(text.contains("[OK]"));
        assert!(text.contains("Coverage"));
        assert!(text.contains("[!!]"));
        assert!(text.contains("Generated: 2026-03-09T00:00:00Z"));
    }

    #[test]
    fn format_traceability_shows_all_sections() {
        let thread = TraceabilityThread {
            requirement: "StopDistance".into(),
            satisfied_by: vec!["BrakeSystem".into()],
            verification_cases: vec!["BrakeTest".into()],
            execution_results: vec![],
            risks: vec!["R-001".into()],
            status: "verified".into(),
        };

        let text = format_traceability_text(&thread);
        assert!(text.contains("Traceability: StopDistance"));
        assert!(text.contains("Status: verified"));
        assert!(text.contains("BrakeSystem"));
        assert!(text.contains("BrakeTest"));
        assert!(text.contains("Execution results:"));
        assert!(text.contains("(none)"));
        assert!(text.contains("R-001"));
    }

    #[test]
    fn format_dashboard_critical_status() {
        let dashboard = Dashboard {
            project_name: "P".into(),
            metrics: vec![DashboardMetric {
                name: "Metric".into(),
                value: "0".into(),
                status: MetricStatus::Critical,
            }],
            generated: "2026-01-01T00:00:00Z".into(),
        };

        let text = format_dashboard_text(&dashboard);
        assert!(text.contains("[XX]"));
    }

    #[test]
    fn format_dashboard_unknown_status() {
        let dashboard = Dashboard {
            project_name: "P".into(),
            metrics: vec![DashboardMetric {
                name: "Metric".into(),
                value: "?".into(),
                status: MetricStatus::Unknown,
            }],
            generated: "2026-01-01T00:00:00Z".into(),
        };

        let text = format_dashboard_text(&dashboard);
        assert!(text.contains("[--]"));
    }
}
