/// Corrective and Preventive Action (CAPA) — formal action programs.
///
/// A CAPA is distinct from an NCR: the NCR documents what went wrong,
/// while the CAPA defines and tracks the corrective/preventive actions
/// taken to address root causes and prevent recurrence.
///
/// CAPAs may originate from NCRs, audit findings, customer complaints,
/// or proactive process improvement initiatives.

use std::collections::BTreeMap;
use serde::Serialize;
use sysml_core::record::{generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue};

use crate::enums::{CapaStatus, CorrectiveActionType};

/// Source that triggered a CAPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapaSource {
    /// Triggered by one or more NCRs.
    Ncr,
    /// Triggered by an audit finding.
    AuditFinding,
    /// Triggered by a customer complaint.
    CustomerComplaint,
    /// Proactive process improvement.
    ProcessImprovement,
    /// Triggered by a regulatory observation.
    RegulatoryObservation,
    /// Triggered by management review.
    ManagementReview,
}

impl CapaSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ncr => "NCR",
            Self::AuditFinding => "Audit Finding",
            Self::CustomerComplaint => "Customer Complaint",
            Self::ProcessImprovement => "Process Improvement",
            Self::RegulatoryObservation => "Regulatory Observation",
            Self::ManagementReview => "Management Review",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Ncr, Self::AuditFinding, Self::CustomerComplaint,
            Self::ProcessImprovement, Self::RegulatoryObservation,
            Self::ManagementReview,
        ]
    }
}

/// Whether the CAPA is corrective (fix existing problem) or
/// preventive (prevent potential problem).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapaType {
    Corrective,
    Preventive,
}

impl CapaType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Corrective => "Corrective",
            Self::Preventive => "Preventive",
        }
    }
}

/// A corrective or preventive action program.
#[derive(Debug, Clone, Serialize)]
pub struct Capa {
    pub id: String,
    pub title: String,
    pub description: String,
    pub capa_type: CapaType,
    pub source: CapaSource,
    /// IDs of source items (NCR IDs, audit finding refs, etc.).
    pub source_refs: Vec<String>,
    pub root_cause: Option<String>,
    pub actions: Vec<CapaAction>,
    pub status: CapaStatus,
    pub owner: String,
    pub created: String,
}

/// An individual action within a CAPA program.
#[derive(Debug, Clone, Serialize)]
pub struct CapaAction {
    pub id: String,
    pub action_type: CorrectiveActionType,
    pub description: String,
    pub owner: String,
    pub due_date: String,
    pub completed: bool,
    pub verification_ref: Option<String>,
}

/// Create a new CAPA with generated ID and `Initiated` status.
pub fn create_capa(
    title: &str,
    description: &str,
    capa_type: CapaType,
    source: CapaSource,
    source_refs: Vec<String>,
    owner: &str,
) -> Capa {
    let id = generate_record_id("quality", "capa", owner);
    Capa {
        id,
        title: title.to_string(),
        description: description.to_string(),
        capa_type,
        source,
        source_refs,
        root_cause: None,
        actions: Vec::new(),
        status: CapaStatus::Initiated,
        owner: owner.to_string(),
        created: now_iso8601(),
    }
}

/// Add an action to a CAPA.
pub fn add_action(capa: &mut Capa, action: CapaAction) {
    capa.actions.push(action);
    if capa.status == CapaStatus::PlanningActions || capa.status == CapaStatus::RootCauseAnalysis {
        capa.status = CapaStatus::Implementing;
    }
}

/// Set the root cause on a CAPA and advance to `PlanningActions`.
pub fn set_root_cause(capa: &mut Capa, root_cause: &str) {
    capa.root_cause = Some(root_cause.to_string());
    if capa.status == CapaStatus::RootCauseAnalysis || capa.status == CapaStatus::Initiated {
        capa.status = CapaStatus::PlanningActions;
    }
}

/// Create a [`RecordEnvelope`] for a CAPA.
pub fn create_capa_record(capa: &Capa, author: &str) -> RecordEnvelope {
    let id = generate_record_id("quality", "capa", author);

    let mut refs = BTreeMap::new();
    refs.insert("capa".to_string(), vec![capa.id.clone()]);
    if !capa.source_refs.is_empty() {
        refs.insert("source".to_string(), capa.source_refs.clone());
    }

    let mut data = BTreeMap::new();
    data.insert("title".into(), RecordValue::String(capa.title.clone()));
    data.insert("description".into(), RecordValue::String(capa.description.clone()));
    data.insert("capa_type".into(), RecordValue::String(capa.capa_type.label().to_string()));
    data.insert("source".into(), RecordValue::String(capa.source.label().to_string()));
    data.insert("status".into(), RecordValue::String(capa.status.label().to_string()));
    data.insert("owner".into(), RecordValue::String(capa.owner.clone()));
    if let Some(rc) = &capa.root_cause {
        data.insert("root_cause".into(), RecordValue::String(rc.clone()));
    }
    data.insert(
        "action_count".into(),
        RecordValue::String(capa.actions.len().to_string()),
    );

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "quality".into(),
            record_type: "capa".into(),
            created: now_iso8601(),
            author: author.into(),
        },
        refs,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::CorrectiveActionType;

    #[test]
    fn create_capa_generates_id_and_initiated_status() {
        let capa = create_capa(
            "Fix brake rotor tolerance",
            "Address recurring dimensional failures in brake rotors",
            CapaType::Corrective,
            CapaSource::Ncr,
            vec!["NCR-001".into()],
            "alice",
        );
        assert!(capa.id.starts_with("quality-capa-"));
        assert_eq!(capa.status, CapaStatus::Initiated);
        assert!(capa.root_cause.is_none());
        assert!(capa.actions.is_empty());
    }

    #[test]
    fn set_root_cause_advances_status() {
        let mut capa = create_capa(
            "Fix issue", "desc",
            CapaType::Corrective, CapaSource::Ncr, vec![], "bob",
        );
        set_root_cause(&mut capa, "Missing SOP for tool offset checks");
        assert_eq!(capa.root_cause.as_deref(), Some("Missing SOP for tool offset checks"));
        assert_eq!(capa.status, CapaStatus::PlanningActions);
    }

    #[test]
    fn add_action_advances_status() {
        let mut capa = create_capa(
            "Fix issue", "desc",
            CapaType::Corrective, CapaSource::Ncr, vec![], "bob",
        );
        set_root_cause(&mut capa, "Root cause");

        let action = CapaAction {
            id: "CA-001".into(),
            action_type: CorrectiveActionType::ProcedureUpdate,
            description: "Update turning SOP".into(),
            owner: "bob".into(),
            due_date: "2026-04-01".into(),
            completed: false,
            verification_ref: None,
        };
        add_action(&mut capa, action);
        assert_eq!(capa.actions.len(), 1);
        assert_eq!(capa.status, CapaStatus::Implementing);
    }

    #[test]
    fn capa_source_labels() {
        assert_eq!(CapaSource::Ncr.label(), "NCR");
        assert_eq!(CapaSource::AuditFinding.label(), "Audit Finding");
        assert_eq!(CapaSource::CustomerComplaint.label(), "Customer Complaint");
    }

    #[test]
    fn capa_type_labels() {
        assert_eq!(CapaType::Corrective.label(), "Corrective");
        assert_eq!(CapaType::Preventive.label(), "Preventive");
    }

    #[test]
    fn capa_record_structure() {
        let capa = create_capa(
            "Fix rotor", "Fix dimensional issue",
            CapaType::Corrective, CapaSource::Ncr,
            vec!["NCR-001".into()], "alice",
        );
        let rec = create_capa_record(&capa, "alice");
        assert_eq!(rec.meta.tool, "quality");
        assert_eq!(rec.meta.record_type, "capa");
        assert!(rec.refs.contains_key("capa"));
        assert!(rec.refs.contains_key("source"));
        assert_eq!(
            rec.data.get("capa_type"),
            Some(&RecordValue::String("Corrective".into()))
        );
        assert_eq!(
            rec.data.get("source"),
            Some(&RecordValue::String("NCR".into()))
        );
    }

    #[test]
    fn capa_record_round_trips_toml() {
        let capa = create_capa(
            "Fix rotor", "Fix dimensional issue",
            CapaType::Corrective, CapaSource::Ncr, vec![], "alice",
        );
        let rec = create_capa_record(&capa, "alice");
        let toml = rec.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.tool, "quality");
        assert_eq!(parsed.meta.record_type, "capa");
    }

    #[test]
    fn capa_serializes() {
        let capa = create_capa(
            "Test CAPA", "description",
            CapaType::Preventive, CapaSource::ProcessImprovement, vec![], "dave",
        );
        let json = serde_json::to_string(&capa).unwrap();
        assert!(json.contains("\"preventive\""));
        assert!(json.contains("\"process_improvement\""));
    }
}
