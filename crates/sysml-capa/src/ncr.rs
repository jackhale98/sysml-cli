/// Nonconformance Report (NCR) — documents an observed nonconformance.
///
/// NCRs are findings: they describe what went wrong, which part or lot is
/// affected, and what disposition the material review board decides.
/// NCRs may trigger CAPAs but are distinct items with their own lifecycle.

use std::collections::BTreeMap;
use serde::Serialize;
use sysml_core::record::{generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue};

use crate::enums::{Disposition, NcrStatus, NonconformanceCategory, SeverityClass};

/// A nonconformance report (NCR).
#[derive(Debug, Clone, Serialize)]
pub struct Ncr {
    pub id: String,
    pub part_name: String,
    pub lot_id: Option<String>,
    pub supplier: Option<String>,
    pub category: NonconformanceCategory,
    pub severity: SeverityClass,
    pub description: String,
    pub disposition: Option<Disposition>,
    pub status: NcrStatus,
    pub created: String,
    pub owner: String,
    /// Linked CAPA IDs (an NCR may trigger one or more CAPAs).
    pub linked_capas: Vec<String>,
}

/// Create a new NCR with generated ID and `Open` status.
pub fn create_ncr(
    part_name: &str,
    category: NonconformanceCategory,
    severity: SeverityClass,
    description: &str,
    owner: &str,
) -> Ncr {
    let id = generate_record_id("quality", "ncr", owner);
    Ncr {
        id,
        part_name: part_name.to_string(),
        lot_id: None,
        supplier: None,
        category,
        severity,
        description: description.to_string(),
        disposition: None,
        status: NcrStatus::Open,
        created: now_iso8601(),
        owner: owner.to_string(),
        linked_capas: Vec::new(),
    }
}

/// Set the disposition on an NCR and advance to `Dispositioned`.
pub fn disposition_ncr(ncr: &mut Ncr, disposition: Disposition) {
    ncr.disposition = Some(disposition);
    ncr.status = NcrStatus::Dispositioned;
}

/// Link a CAPA to this NCR.
pub fn link_capa(ncr: &mut Ncr, capa_id: &str) {
    if !ncr.linked_capas.contains(&capa_id.to_string()) {
        ncr.linked_capas.push(capa_id.to_string());
    }
}

/// Create a [`RecordEnvelope`] for an NCR.
pub fn create_ncr_record(ncr: &Ncr, author: &str) -> RecordEnvelope {
    let id = generate_record_id("quality", "ncr", author);

    let mut refs = BTreeMap::new();
    refs.insert("ncr".to_string(), vec![ncr.id.clone()]);
    refs.insert("part".to_string(), vec![ncr.part_name.clone()]);
    if !ncr.linked_capas.is_empty() {
        refs.insert("capa".to_string(), ncr.linked_capas.clone());
    }

    let mut data = BTreeMap::new();
    data.insert("part_name".into(), RecordValue::String(ncr.part_name.clone()));
    data.insert("category".into(), RecordValue::String(ncr.category.label().to_string()));
    data.insert("severity".into(), RecordValue::String(ncr.severity.label().to_string()));
    data.insert("description".into(), RecordValue::String(ncr.description.clone()));
    data.insert("status".into(), RecordValue::String(ncr.status.label().to_string()));
    data.insert("owner".into(), RecordValue::String(ncr.owner.clone()));
    if let Some(lot) = &ncr.lot_id {
        data.insert("lot_id".into(), RecordValue::String(lot.clone()));
    }
    if let Some(supplier) = &ncr.supplier {
        data.insert("supplier".into(), RecordValue::String(supplier.clone()));
    }
    if let Some(disp) = &ncr.disposition {
        data.insert("disposition".into(), RecordValue::String(disp.label().to_string()));
    }

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "quality".into(),
            record_type: "ncr".into(),
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

    fn sample_ncr() -> Ncr {
        Ncr {
            id: "NCR-001".into(),
            part_name: "BrakeRotor".into(),
            lot_id: Some("LOT-2026-03".into()),
            supplier: Some("AcmeCasting".into()),
            category: NonconformanceCategory::Dimensional,
            severity: SeverityClass::Major,
            description: "OD out of tolerance by 0.5mm".into(),
            disposition: None,
            status: NcrStatus::Open,
            created: "2026-03-01T10:00:00Z".into(),
            owner: "alice".into(),
            linked_capas: Vec::new(),
        }
    }

    #[test]
    fn create_ncr_generates_id_and_open_status() {
        let ncr = create_ncr(
            "Widget",
            NonconformanceCategory::Functional,
            SeverityClass::Critical,
            "Widget fails under load",
            "charlie",
        );
        assert!(ncr.id.starts_with("quality-ncr-"));
        assert_eq!(ncr.status, NcrStatus::Open);
        assert!(ncr.disposition.is_none());
        assert!(ncr.linked_capas.is_empty());
    }

    #[test]
    fn disposition_sets_value_and_advances_status() {
        let mut ncr = sample_ncr();
        disposition_ncr(&mut ncr, Disposition::Rework);
        assert_eq!(ncr.disposition, Some(Disposition::Rework));
        assert_eq!(ncr.status, NcrStatus::Dispositioned);
    }

    #[test]
    fn link_capa_adds_id_no_duplicates() {
        let mut ncr = sample_ncr();
        link_capa(&mut ncr, "CAPA-001");
        link_capa(&mut ncr, "CAPA-001");
        link_capa(&mut ncr, "CAPA-002");
        assert_eq!(ncr.linked_capas, vec!["CAPA-001", "CAPA-002"]);
    }

    #[test]
    fn ncr_record_structure() {
        let ncr = sample_ncr();
        let rec = create_ncr_record(&ncr, "alice");
        assert_eq!(rec.meta.tool, "quality");
        assert_eq!(rec.meta.record_type, "ncr");
        assert!(rec.refs.contains_key("ncr"));
        assert!(rec.refs.contains_key("part"));
        assert_eq!(
            rec.data.get("category"),
            Some(&RecordValue::String("Dimensional".into()))
        );
        assert_eq!(
            rec.data.get("severity"),
            Some(&RecordValue::String("Major".into()))
        );
    }

    #[test]
    fn ncr_record_includes_optional_fields() {
        let ncr = sample_ncr();
        let rec = create_ncr_record(&ncr, "alice");
        assert!(rec.data.contains_key("lot_id"));
        assert!(rec.data.contains_key("supplier"));
    }

    #[test]
    fn ncr_record_omits_none_fields() {
        let mut ncr = sample_ncr();
        ncr.lot_id = None;
        ncr.supplier = None;
        ncr.disposition = None;
        let rec = create_ncr_record(&ncr, "alice");
        assert!(!rec.data.contains_key("lot_id"));
        assert!(!rec.data.contains_key("supplier"));
        assert!(!rec.data.contains_key("disposition"));
    }

    #[test]
    fn ncr_record_includes_linked_capas() {
        let mut ncr = sample_ncr();
        ncr.linked_capas = vec!["CAPA-001".into(), "CAPA-002".into()];
        let rec = create_ncr_record(&ncr, "alice");
        assert_eq!(rec.refs.get("capa").unwrap().len(), 2);
    }

    #[test]
    fn ncr_serializes() {
        let ncr = sample_ncr();
        let json = serde_json::to_string(&ncr).unwrap();
        assert!(json.contains("\"part_name\""));
        assert!(json.contains("\"dimensional\""));
        assert!(json.contains("\"major\""));
    }

    #[test]
    fn ncr_record_round_trips_toml() {
        let ncr = sample_ncr();
        let rec = create_ncr_record(&ncr, "alice");
        let toml = rec.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.tool, "quality");
        assert_eq!(parsed.meta.record_type, "ncr");
        assert_eq!(parsed.data, rec.data);
    }
}
