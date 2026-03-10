/// Root Cause Analysis (RCA) — shared by both NCRs and CAPAs.
///
/// Provides structured RCA methodologies (5 Why, Fishbone/Ishikawa)
/// as interactive wizard sequences.

use std::collections::BTreeMap;
use serde::Serialize;
use sysml_core::interactive::WizardStep;
use sysml_core::record::{generate_record_id, now_iso8601, RecordEnvelope, RecordMeta, RecordValue};

use crate::enums::RootCauseMethod;

/// Root cause analysis linked to an NCR or CAPA.
#[derive(Debug, Clone, Serialize)]
pub struct RootCauseAnalysis {
    pub source_id: String,
    pub method: RootCauseMethod,
    pub findings: Vec<String>,
    pub root_cause: String,
}

/// Build a 5 Why analysis wizard.
///
/// Returns six wizard steps: five successive "Why?" prompts plus a
/// final root cause summary.
pub fn build_five_why_steps() -> Vec<WizardStep> {
    let mut steps = Vec::with_capacity(6);

    for i in 1..=5 {
        let prompt = match i {
            1 => "Why did the nonconformance occur? (1st Why)".to_string(),
            2 => "Why did that happen? (2nd Why)".to_string(),
            3 => "Why did that happen? (3rd Why)".to_string(),
            4 => "Why did that happen? (4th Why)".to_string(),
            5 => "Why did that happen? (5th Why)".to_string(),
            _ => unreachable!(),
        };

        let explanation = match i {
            1 => "Start with the immediate, observable failure. Describe what \
                  went wrong in concrete terms.",
            2 => "Look one level deeper. What condition or event allowed the \
                  first answer to occur?",
            3 => "Continue drilling down. Avoid jumping to solutions -- focus \
                  on understanding the causal chain.",
            4 => "You should be approaching systemic or process-level causes \
                  at this point.",
            5 => "The fifth why typically reveals the root cause: a management \
                  system, process, or cultural gap.",
            _ => unreachable!(),
        };

        steps.push(
            WizardStep::string(format!("why_{i}"), prompt)
                .with_explanation(explanation),
        );
    }

    steps.push(
        WizardStep::string("root_cause", "Based on the 5 Whys, what is the root cause?")
            .with_explanation(
                "Synthesize the causal chain into a single, actionable root cause \
                 statement. This should describe the fundamental systemic issue, \
                 not just a symptom.",
            ),
    );

    steps
}

/// Build a Fishbone (Ishikawa) analysis wizard.
///
/// Returns seven wizard steps: six standard Ishikawa categories
/// (Man, Machine, Method, Material, Measurement, Environment)
/// plus a final root cause summary.
pub fn build_fishbone_steps() -> Vec<WizardStep> {
    vec![
        WizardStep::string(
            "man",
            "Man (People): What personnel factors may have contributed?",
        )
        .with_explanation(
            "Consider training gaps, fatigue, skill level, staffing, \
             communication breakdowns, or procedural non-compliance.",
        )
        .optional(),
        WizardStep::string(
            "machine",
            "Machine (Equipment): What equipment factors may have contributed?",
        )
        .with_explanation(
            "Consider equipment age, maintenance history, calibration status, \
             capability, or environmental conditions affecting machinery.",
        )
        .optional(),
        WizardStep::string(
            "method",
            "Method (Process): What process or procedure factors may have contributed?",
        )
        .with_explanation(
            "Consider work instructions, standard operating procedures, process \
             validation status, or sequence of operations.",
        )
        .optional(),
        WizardStep::string(
            "material",
            "Material: What material or component factors may have contributed?",
        )
        .with_explanation(
            "Consider raw material quality, incoming inspection results, \
             supplier changes, storage conditions, or shelf life.",
        )
        .optional(),
        WizardStep::string(
            "measurement",
            "Measurement: What measurement or inspection factors may have contributed?",
        )
        .with_explanation(
            "Consider gage R&R, measurement uncertainty, inspection criteria, \
             sampling plans, or test method validation.",
        )
        .optional(),
        WizardStep::string(
            "environment",
            "Environment: What environmental factors may have contributed?",
        )
        .with_explanation(
            "Consider temperature, humidity, cleanliness, lighting, vibration, \
             or other ambient conditions in the work area.",
        )
        .optional(),
        WizardStep::string(
            "root_cause",
            "Based on the Fishbone analysis, what is the root cause?",
        )
        .with_explanation(
            "Synthesize findings from the contributing categories into a single, \
             actionable root cause statement.",
        ),
    ]
}

/// Create a [`RecordEnvelope`] for a root cause analysis.
pub fn create_rca_record(rca: &RootCauseAnalysis, author: &str) -> RecordEnvelope {
    let id = generate_record_id("quality", "rca", author);

    let mut refs = BTreeMap::new();
    refs.insert("source".to_string(), vec![rca.source_id.clone()]);

    let mut data = BTreeMap::new();
    data.insert("method".into(), RecordValue::String(rca.method.label().to_string()));
    data.insert("root_cause".into(), RecordValue::String(rca.root_cause.clone()));
    data.insert(
        "findings".into(),
        RecordValue::Array(
            rca.findings.iter().map(|f| RecordValue::String(f.clone())).collect(),
        ),
    );

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "quality".into(),
            record_type: "rca".into(),
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
    use sysml_core::record::RecordValue;

    fn sample_rca() -> RootCauseAnalysis {
        RootCauseAnalysis {
            source_id: "NCR-001".into(),
            method: RootCauseMethod::FiveWhy,
            findings: vec![
                "Rotor OD measured 250.5mm vs 250.0mm spec".into(),
                "Lathe tool offset drifted during shift".into(),
                "No mid-shift tool offset verification".into(),
                "SOP does not require periodic re-check".into(),
                "Process validation gap in turning operations".into(),
            ],
            root_cause: "Missing periodic tool offset verification in turning SOP".into(),
        }
    }

    #[test]
    fn five_why_step_count() {
        let steps = build_five_why_steps();
        assert_eq!(steps.len(), 6);
    }

    #[test]
    fn five_why_step_ids() {
        let steps = build_five_why_steps();
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["why_1", "why_2", "why_3", "why_4", "why_5", "root_cause"]);
    }

    #[test]
    fn five_why_all_required() {
        let steps = build_five_why_steps();
        assert!(steps.iter().all(|s| s.required));
    }

    #[test]
    fn five_why_has_explanations() {
        let steps = build_five_why_steps();
        assert!(steps.iter().all(|s| s.explanation.is_some()));
    }

    #[test]
    fn fishbone_step_count() {
        let steps = build_fishbone_steps();
        assert_eq!(steps.len(), 7);
    }

    #[test]
    fn fishbone_step_ids() {
        let steps = build_fishbone_steps();
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["man", "machine", "method", "material", "measurement", "environment", "root_cause"]);
    }

    #[test]
    fn fishbone_categories_optional_root_cause_required() {
        let steps = build_fishbone_steps();
        for step in &steps[..6] {
            assert!(!step.required, "category step '{}' should be optional", step.id);
        }
        assert!(steps[6].required);
    }

    #[test]
    fn rca_record_structure() {
        let rca = sample_rca();
        let rec = create_rca_record(&rca, "alice");
        assert_eq!(rec.meta.tool, "quality");
        assert_eq!(rec.meta.record_type, "rca");
        assert!(rec.refs.contains_key("source"));
        assert_eq!(
            rec.data.get("method"),
            Some(&RecordValue::String("5 Why".into()))
        );
        match rec.data.get("findings") {
            Some(RecordValue::Array(arr)) => assert_eq!(arr.len(), 5),
            other => panic!("expected findings array, got {other:?}"),
        }
    }

    #[test]
    fn rca_record_round_trips_toml() {
        let rca = sample_rca();
        let rec = create_rca_record(&rca, "alice");
        let toml = rec.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.record_type, "rca");
        assert_eq!(parsed.data, rec.data);
    }

    #[test]
    fn rca_serializes() {
        let rca = sample_rca();
        let json = serde_json::to_string(&rca).unwrap();
        assert!(json.contains("\"five_why\""));
        assert!(json.contains("\"root_cause\""));
    }
}
