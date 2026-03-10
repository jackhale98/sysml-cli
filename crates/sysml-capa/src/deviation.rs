/// Process Deviation — a planned, approved departure from standard processes.
///
/// Process deviations are distinct from NCRs: they are pre-approved variations,
/// not unplanned failures. Examples include using an alternate material for a
/// limited production run, or temporarily relaxing a dimensional tolerance.

use std::collections::BTreeMap;
use serde::Serialize;
use sysml_core::record::{generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue};

use crate::enums::{DeviationScope, DeviationStatus};

/// A process deviation request and its approval state.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessDeviation {
    pub id: String,
    pub title: String,
    pub description: String,
    /// What is being deviated from (spec, SOP, drawing, etc.).
    pub standard_ref: String,
    /// The proposed alternate condition.
    pub proposed_condition: String,
    pub scope: DeviationScope,
    /// How many units or how long the deviation applies.
    pub quantity_or_duration: String,
    pub justification: String,
    /// Risk assessment: what could go wrong.
    pub risk_assessment: Option<String>,
    pub status: DeviationStatus,
    pub owner: String,
    pub approver: Option<String>,
    pub created: String,
    /// Affected part names.
    pub affected_parts: Vec<String>,
}

/// Create a new process deviation with generated ID and `Requested` status.
pub fn create_deviation(
    title: &str,
    description: &str,
    standard_ref: &str,
    proposed_condition: &str,
    scope: DeviationScope,
    quantity_or_duration: &str,
    justification: &str,
    owner: &str,
) -> ProcessDeviation {
    let id = generate_record_id("quality", "deviation", owner);
    ProcessDeviation {
        id,
        title: title.to_string(),
        description: description.to_string(),
        standard_ref: standard_ref.to_string(),
        proposed_condition: proposed_condition.to_string(),
        scope,
        quantity_or_duration: quantity_or_duration.to_string(),
        justification: justification.to_string(),
        risk_assessment: None,
        status: DeviationStatus::Requested,
        owner: owner.to_string(),
        approver: None,
        created: now_iso8601(),
        affected_parts: Vec::new(),
    }
}

/// Approve a deviation.
pub fn approve_deviation(dev: &mut ProcessDeviation, approver: &str) {
    dev.status = DeviationStatus::Approved;
    dev.approver = Some(approver.to_string());
}

/// Deny a deviation.
pub fn deny_deviation(dev: &mut ProcessDeviation, approver: &str) {
    dev.status = DeviationStatus::Denied;
    dev.approver = Some(approver.to_string());
}

/// Activate an approved deviation.
pub fn activate_deviation(dev: &mut ProcessDeviation) -> Result<(), &'static str> {
    if dev.status != DeviationStatus::Approved {
        return Err("can only activate an approved deviation");
    }
    dev.status = DeviationStatus::Active;
    Ok(())
}

/// Create a [`RecordEnvelope`] for a process deviation.
pub fn create_deviation_record(dev: &ProcessDeviation, author: &str) -> RecordEnvelope {
    let id = generate_record_id("quality", "deviation", author);

    let mut refs = BTreeMap::new();
    refs.insert("deviation".to_string(), vec![dev.id.clone()]);
    if !dev.affected_parts.is_empty() {
        refs.insert("parts".to_string(), dev.affected_parts.clone());
    }

    let mut data = BTreeMap::new();
    data.insert("title".into(), RecordValue::String(dev.title.clone()));
    data.insert("description".into(), RecordValue::String(dev.description.clone()));
    data.insert("standard_ref".into(), RecordValue::String(dev.standard_ref.clone()));
    data.insert("proposed_condition".into(), RecordValue::String(dev.proposed_condition.clone()));
    data.insert("scope".into(), RecordValue::String(dev.scope.label().to_string()));
    data.insert("quantity_or_duration".into(), RecordValue::String(dev.quantity_or_duration.clone()));
    data.insert("justification".into(), RecordValue::String(dev.justification.clone()));
    data.insert("status".into(), RecordValue::String(dev.status.label().to_string()));
    data.insert("owner".into(), RecordValue::String(dev.owner.clone()));
    if let Some(ra) = &dev.risk_assessment {
        data.insert("risk_assessment".into(), RecordValue::String(ra.clone()));
    }
    if let Some(approver) = &dev.approver {
        data.insert("approver".into(), RecordValue::String(approver.clone()));
    }

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "quality".into(),
            record_type: "deviation".into(),
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

    fn sample_deviation() -> ProcessDeviation {
        create_deviation(
            "Use alternate alloy for lot 42",
            "6061-T6 out of stock, use 6063-T5 for limited run",
            "SPEC-MAT-001 rev C",
            "6063-T5 aluminum instead of 6061-T6",
            DeviationScope::Lot,
            "500 units",
            "Supplier shortage; 6063-T5 meets min yield strength",
            "charlie",
        )
    }

    #[test]
    fn create_deviation_generates_id_and_requested_status() {
        let dev = sample_deviation();
        assert!(dev.id.starts_with("quality-deviation-"));
        assert_eq!(dev.status, DeviationStatus::Requested);
        assert!(dev.approver.is_none());
        assert!(dev.risk_assessment.is_none());
    }

    #[test]
    fn approve_sets_status_and_approver() {
        let mut dev = sample_deviation();
        approve_deviation(&mut dev, "quality_mgr");
        assert_eq!(dev.status, DeviationStatus::Approved);
        assert_eq!(dev.approver.as_deref(), Some("quality_mgr"));
    }

    #[test]
    fn deny_sets_status_and_approver() {
        let mut dev = sample_deviation();
        deny_deviation(&mut dev, "quality_mgr");
        assert_eq!(dev.status, DeviationStatus::Denied);
        assert_eq!(dev.approver.as_deref(), Some("quality_mgr"));
    }

    #[test]
    fn activate_requires_approved_status() {
        let mut dev = sample_deviation();
        assert!(activate_deviation(&mut dev).is_err());

        approve_deviation(&mut dev, "quality_mgr");
        assert!(activate_deviation(&mut dev).is_ok());
        assert_eq!(dev.status, DeviationStatus::Active);
    }

    #[test]
    fn deviation_record_structure() {
        let dev = sample_deviation();
        let rec = create_deviation_record(&dev, "charlie");
        assert_eq!(rec.meta.tool, "quality");
        assert_eq!(rec.meta.record_type, "deviation");
        assert!(rec.refs.contains_key("deviation"));
        assert_eq!(
            rec.data.get("scope"),
            Some(&RecordValue::String("Lot/Batch".into()))
        );
        assert_eq!(
            rec.data.get("status"),
            Some(&RecordValue::String("Requested".into()))
        );
    }

    #[test]
    fn deviation_record_round_trips_toml() {
        let dev = sample_deviation();
        let rec = create_deviation_record(&dev, "charlie");
        let toml = rec.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.tool, "quality");
        assert_eq!(parsed.meta.record_type, "deviation");
    }

    #[test]
    fn deviation_serializes() {
        let dev = sample_deviation();
        let json = serde_json::to_string(&dev).unwrap();
        assert!(json.contains("\"requested\""));
        assert!(json.contains("\"lot\""));
    }

    #[test]
    fn deviation_scope_all() {
        assert_eq!(DeviationScope::all().len(), 5);
    }
}
