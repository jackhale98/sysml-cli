/// Risk management domain for SysML v2 models.
///
/// Provides types and functions for identifying, assessing, and mitigating
/// risks extracted from SysML models.  Integrates with `sysml-core`'s
/// record envelope system for audit-trail persistence and the interactive
/// wizard framework for guided risk entry.

use std::collections::BTreeMap;

use serde::Serialize;
use sysml_core::interactive::{ChoiceOption, PromptKind, WizardStep};
use sysml_core::model::{DefKind, Model};
use sysml_core::record::{generate_record_id, RecordEnvelope, RecordMeta, RecordValue};

// =========================================================================
// Enums
// =========================================================================

/// Severity of a risk's consequence, following MIL-STD-882E categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityLevel {
    Negligible,
    Marginal,
    Moderate,
    Critical,
    Catastrophic,
}

impl SeverityLevel {
    /// Numeric score (1-5) used in RPN computation.
    pub fn value(&self) -> u32 {
        match self {
            Self::Negligible => 1,
            Self::Marginal => 2,
            Self::Moderate => 3,
            Self::Critical => 4,
            Self::Catastrophic => 5,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Negligible => "Negligible",
            Self::Marginal => "Marginal",
            Self::Moderate => "Moderate",
            Self::Critical => "Critical",
            Self::Catastrophic => "Catastrophic",
        }
    }

    /// Parse from a numeric value (1-5).
    pub fn from_value(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Negligible),
            2 => Some(Self::Marginal),
            3 => Some(Self::Moderate),
            4 => Some(Self::Critical),
            5 => Some(Self::Catastrophic),
            _ => None,
        }
    }

    /// All variants in ascending order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Negligible,
            Self::Marginal,
            Self::Moderate,
            Self::Critical,
            Self::Catastrophic,
        ]
    }
}

/// Probability of a risk occurring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LikelihoodLevel {
    Improbable,
    Remote,
    Occasional,
    Probable,
    Frequent,
}

impl LikelihoodLevel {
    /// Numeric score (1-5) used in RPN computation.
    pub fn value(&self) -> u32 {
        match self {
            Self::Improbable => 1,
            Self::Remote => 2,
            Self::Occasional => 3,
            Self::Probable => 4,
            Self::Frequent => 5,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Improbable => "Improbable",
            Self::Remote => "Remote",
            Self::Occasional => "Occasional",
            Self::Probable => "Probable",
            Self::Frequent => "Frequent",
        }
    }

    /// Parse from a numeric value (1-5).
    pub fn from_value(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Improbable),
            2 => Some(Self::Remote),
            3 => Some(Self::Occasional),
            4 => Some(Self::Probable),
            5 => Some(Self::Frequent),
            _ => None,
        }
    }

    /// All variants in ascending order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Improbable,
            Self::Remote,
            Self::Occasional,
            Self::Probable,
            Self::Frequent,
        ]
    }
}

/// Ability to detect a risk before it causes harm (inverted per FMEA:
/// higher score = harder to detect = worse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectabilityLevel {
    AlmostCertain,
    High,
    Moderate,
    Low,
    AlmostImpossible,
}

impl DetectabilityLevel {
    /// Numeric score (1-5); higher means harder to detect (FMEA convention).
    pub fn value(&self) -> u32 {
        match self {
            Self::AlmostCertain => 1,
            Self::High => 2,
            Self::Moderate => 3,
            Self::Low => 4,
            Self::AlmostImpossible => 5,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::AlmostCertain => "Almost Certain",
            Self::High => "High",
            Self::Moderate => "Moderate",
            Self::Low => "Low",
            Self::AlmostImpossible => "Almost Impossible",
        }
    }

    /// Parse from a numeric value (1-5).
    pub fn from_value(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::AlmostCertain),
            2 => Some(Self::High),
            3 => Some(Self::Moderate),
            4 => Some(Self::Low),
            5 => Some(Self::AlmostImpossible),
            _ => None,
        }
    }

    /// All variants in ascending order.
    pub fn all() -> &'static [Self] {
        &[
            Self::AlmostCertain,
            Self::High,
            Self::Moderate,
            Self::Low,
            Self::AlmostImpossible,
        ]
    }
}

/// Domain category for a risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory {
    Technical,
    Schedule,
    Cost,
    Safety,
    Regulatory,
    SupplyChain,
    Environmental,
}

impl RiskCategory {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Technical => "Technical",
            Self::Schedule => "Schedule",
            Self::Cost => "Cost",
            Self::Safety => "Safety",
            Self::Regulatory => "Regulatory",
            Self::SupplyChain => "Supply Chain",
            Self::Environmental => "Environmental",
        }
    }

    /// All variants.
    pub fn all() -> &'static [Self] {
        &[
            Self::Technical,
            Self::Schedule,
            Self::Cost,
            Self::Safety,
            Self::Regulatory,
            Self::SupplyChain,
            Self::Environmental,
        ]
    }

    /// Parse from a lowercase string identifier.
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "technical" => Some(Self::Technical),
            "schedule" => Some(Self::Schedule),
            "cost" => Some(Self::Cost),
            "safety" => Some(Self::Safety),
            "regulatory" => Some(Self::Regulatory),
            "supply_chain" => Some(Self::SupplyChain),
            "environmental" => Some(Self::Environmental),
            _ => None,
        }
    }

    /// Lowercase identifier for serialization.
    fn id(&self) -> &'static str {
        match self {
            Self::Technical => "technical",
            Self::Schedule => "schedule",
            Self::Cost => "cost",
            Self::Safety => "safety",
            Self::Regulatory => "regulatory",
            Self::SupplyChain => "supply_chain",
            Self::Environmental => "environmental",
        }
    }
}

/// Lifecycle status of a risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Identified,
    Analyzing,
    Mitigating,
    Monitoring,
    Closed,
    Accepted,
}

impl RiskStatus {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Identified => "Identified",
            Self::Analyzing => "Analyzing",
            Self::Mitigating => "Mitigating",
            Self::Monitoring => "Monitoring",
            Self::Closed => "Closed",
            Self::Accepted => "Accepted",
        }
    }

    /// Parse from a lowercase string identifier.
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "identified" => Some(Self::Identified),
            "analyzing" => Some(Self::Analyzing),
            "mitigating" => Some(Self::Mitigating),
            "monitoring" => Some(Self::Monitoring),
            "closed" => Some(Self::Closed),
            "accepted" => Some(Self::Accepted),
            _ => None,
        }
    }
}

/// Strategy for addressing a risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MitigationStrategy {
    Avoid,
    Transfer,
    Reduce,
    Accept,
    Contingency,
}

impl MitigationStrategy {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Avoid => "Avoid",
            Self::Transfer => "Transfer",
            Self::Reduce => "Reduce",
            Self::Accept => "Accept",
            Self::Contingency => "Contingency",
        }
    }

    /// Parse from a lowercase string identifier.
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "avoid" => Some(Self::Avoid),
            "transfer" => Some(Self::Transfer),
            "reduce" => Some(Self::Reduce),
            "accept" => Some(Self::Accept),
            "contingency" => Some(Self::Contingency),
            _ => None,
        }
    }
}

// =========================================================================
// Domain structs
// =========================================================================

/// A risk definition extracted from or created for a SysML model.
#[derive(Debug, Clone, Serialize)]
pub struct RiskDef {
    pub id: String,
    pub title: String,
    pub category: Option<RiskCategory>,
    pub status: Option<RiskStatus>,
    pub severity: Option<SeverityLevel>,
    pub likelihood: Option<LikelihoodLevel>,
    pub detectability: Option<DetectabilityLevel>,
    /// Risk Priority Number (S * L * D); `None` if any factor is missing.
    pub rpn: Option<u32>,
    pub owner: Option<String>,
    pub notes: Option<String>,
}

impl RiskDef {
    /// Recompute the RPN from the current severity, likelihood, and
    /// detectability.  Updates `self.rpn` in place and returns the value.
    pub fn recompute_rpn(&mut self) -> Option<u32> {
        self.rpn = match (&self.severity, &self.likelihood, &self.detectability) {
            (Some(s), Some(l), Some(d)) => Some(compute_rpn(s, l, d)),
            _ => None,
        };
        self.rpn
    }
}

/// A point-in-time assessment of a risk.
#[derive(Debug, Clone, Serialize)]
pub struct RiskAssessment {
    pub risk_id: String,
    pub severity: SeverityLevel,
    pub likelihood: LikelihoodLevel,
    pub detectability: DetectabilityLevel,
    pub rpn: u32,
    pub notes: Option<String>,
    pub assessed_by: String,
}

/// A mitigation action linked to a risk.
#[derive(Debug, Clone, Serialize)]
pub struct Mitigation {
    pub id: String,
    pub risk_id: String,
    pub description: String,
    pub strategy: MitigationStrategy,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub status: Option<RiskStatus>,
}

// =========================================================================
// RPN computation
// =========================================================================

/// Compute the Risk Priority Number: Severity * Likelihood * Detectability.
///
/// The RPN ranges from 1 (lowest) to 125 (highest).
pub fn compute_rpn(
    severity: &SeverityLevel,
    likelihood: &LikelihoodLevel,
    detectability: &DetectabilityLevel,
) -> u32 {
    severity.value() * likelihood.value() * detectability.value()
}

// =========================================================================
// Model extraction
// =========================================================================

/// Scan a parsed SysML model for part definitions that represent risks.
///
/// A definition is considered a risk if:
/// - It specializes `RiskDef` (via `super_type`), or
/// - Its name contains the substring `"Risk"`.
///
/// Severity and likelihood are extracted from attribute usages whose names
/// match `severity` or `likelihood` and whose `value_expr` can be parsed as
/// a 1-5 integer.
pub fn extract_risks(model: &Model) -> Vec<RiskDef> {
    let mut risks = Vec::new();

    for def in &model.definitions {
        if def.kind != DefKind::Part {
            continue;
        }

        let is_risk_specialization = def
            .super_type
            .as_deref()
            .map(|s| s == "RiskDef" || s.ends_with("::RiskDef"))
            .unwrap_or(false);

        let name_contains_risk = def.name.contains("Risk");

        if !is_risk_specialization && !name_contains_risk {
            continue;
        }

        let usages = model.usages_in_def(&def.name);

        let severity = extract_level(&usages, "severity")
            .and_then(SeverityLevel::from_value);
        let likelihood = extract_level(&usages, "likelihood")
            .and_then(LikelihoodLevel::from_value);
        let detectability = extract_level(&usages, "detectability")
            .and_then(DetectabilityLevel::from_value);

        let rpn = match (&severity, &likelihood, &detectability) {
            (Some(s), Some(l), Some(d)) => Some(compute_rpn(s, l, d)),
            _ => None,
        };

        let owner = extract_string_attr(&usages, "owner");
        let notes = extract_string_attr(&usages, "notes");
        let category_str = extract_string_attr(&usages, "category");
        let status_str = extract_string_attr(&usages, "status");

        risks.push(RiskDef {
            id: def.name.clone(),
            title: def.name.clone(),
            category: category_str.and_then(|s| RiskCategory::from_str_value(&s)),
            status: status_str.and_then(|s| RiskStatus::from_str_value(&s)),
            severity,
            likelihood,
            detectability,
            rpn,
            owner,
            notes,
        });
    }

    risks
}

/// Try to extract a numeric level (1-5) from a named attribute usage.
fn extract_level(usages: &[&sysml_core::model::Usage], attr_name: &str) -> Option<u32> {
    for u in usages {
        if u.name == attr_name {
            if let Some(expr) = &u.value_expr {
                if let Ok(v) = expr.trim().parse::<u32>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Try to extract a string value from a named attribute usage.
fn extract_string_attr(usages: &[&sysml_core::model::Usage], attr_name: &str) -> Option<String> {
    for u in usages {
        if u.name == attr_name {
            if let Some(expr) = &u.value_expr {
                let trimmed = expr.trim().trim_matches('"');
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

// =========================================================================
// Record creation
// =========================================================================

/// Create a [`RecordEnvelope`] for a newly identified risk.
pub fn create_risk_record(risk: &RiskDef, author: &str) -> RecordEnvelope {
    let id = generate_record_id("risk", "entity", author);

    let mut refs = BTreeMap::new();
    refs.insert("risk".to_string(), vec![risk.id.clone()]);

    let mut data = BTreeMap::new();
    data.insert("title".into(), RecordValue::String(risk.title.clone()));
    if let Some(cat) = &risk.category {
        data.insert("category".into(), RecordValue::String(cat.label().to_string()));
    }
    if let Some(st) = &risk.status {
        data.insert("status".into(), RecordValue::String(st.label().to_string()));
    }
    if let Some(s) = &risk.severity {
        data.insert("severity".into(), RecordValue::Integer(s.value() as i64));
        data.insert(
            "severity_label".into(),
            RecordValue::String(s.label().to_string()),
        );
    }
    if let Some(l) = &risk.likelihood {
        data.insert("likelihood".into(), RecordValue::Integer(l.value() as i64));
        data.insert(
            "likelihood_label".into(),
            RecordValue::String(l.label().to_string()),
        );
    }
    if let Some(d) = &risk.detectability {
        data.insert("detectability".into(), RecordValue::Integer(d.value() as i64));
        data.insert(
            "detectability_label".into(),
            RecordValue::String(d.label().to_string()),
        );
    }
    if let Some(rpn) = risk.rpn {
        data.insert("rpn".into(), RecordValue::Integer(rpn as i64));
    }
    if let Some(owner) = &risk.owner {
        data.insert("owner".into(), RecordValue::String(owner.clone()));
    }
    if let Some(notes) = &risk.notes {
        data.insert("notes".into(), RecordValue::String(notes.clone()));
    }

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "risk".into(),
            record_type: "entity".into(),
            created: sysml_core::record::now_iso8601(),
            author: author.into(),
        },
        refs,
        data,
    }
}

/// Create a [`RecordEnvelope`] for a risk assessment (append-only history).
pub fn create_assessment_record(assessment: &RiskAssessment, author: &str) -> RecordEnvelope {
    let id = generate_record_id("risk", "assessment", author);

    let mut refs = BTreeMap::new();
    refs.insert("risk".to_string(), vec![assessment.risk_id.clone()]);

    let mut data = BTreeMap::new();
    data.insert(
        "severity".into(),
        RecordValue::Integer(assessment.severity.value() as i64),
    );
    data.insert(
        "severity_label".into(),
        RecordValue::String(assessment.severity.label().to_string()),
    );
    data.insert(
        "likelihood".into(),
        RecordValue::Integer(assessment.likelihood.value() as i64),
    );
    data.insert(
        "likelihood_label".into(),
        RecordValue::String(assessment.likelihood.label().to_string()),
    );
    data.insert(
        "detectability".into(),
        RecordValue::Integer(assessment.detectability.value() as i64),
    );
    data.insert(
        "detectability_label".into(),
        RecordValue::String(assessment.detectability.label().to_string()),
    );
    data.insert("rpn".into(), RecordValue::Integer(assessment.rpn as i64));
    if let Some(notes) = &assessment.notes {
        data.insert("notes".into(), RecordValue::String(notes.clone()));
    }
    data.insert(
        "assessed_by".into(),
        RecordValue::String(assessment.assessed_by.clone()),
    );

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "risk".into(),
            record_type: "assessment".into(),
            created: sysml_core::record::now_iso8601(),
            author: author.into(),
        },
        refs,
        data,
    }
}

/// Create a [`RecordEnvelope`] for a mitigation action.
pub fn create_mitigation_record(mitigation: &Mitigation, author: &str) -> RecordEnvelope {
    let id = generate_record_id("risk", "mitigation", author);

    let mut refs = BTreeMap::new();
    refs.insert("risk".to_string(), vec![mitigation.risk_id.clone()]);
    refs.insert("mitigation".to_string(), vec![mitigation.id.clone()]);

    let mut data = BTreeMap::new();
    data.insert(
        "description".into(),
        RecordValue::String(mitigation.description.clone()),
    );
    data.insert(
        "strategy".into(),
        RecordValue::String(mitigation.strategy.label().to_string()),
    );
    if let Some(owner) = &mitigation.owner {
        data.insert("owner".into(), RecordValue::String(owner.clone()));
    }
    if let Some(due) = &mitigation.due_date {
        data.insert("due_date".into(), RecordValue::String(due.clone()));
    }
    if let Some(st) = &mitigation.status {
        data.insert("status".into(), RecordValue::String(st.label().to_string()));
    }

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "risk".into(),
            record_type: "mitigation".into(),
            created: sysml_core::record::now_iso8601(),
            author: author.into(),
        },
        refs,
        data,
    }
}

// =========================================================================
// Interactive wizards
// =========================================================================

/// Build a wizard flow for creating a new risk definition.
pub fn build_risk_wizard() -> Vec<WizardStep> {
    vec![
        WizardStep::string("title", "What is the risk title?")
            .with_explanation(
                "A concise name for the risk, e.g. 'Battery Thermal Runaway'. \
                 This becomes the identifier in the risk register.",
            ),
        WizardStep {
            id: "category".into(),
            prompt: "What category does this risk belong to?".into(),
            explanation: Some(
                "Risk categories help organize risks by domain. Technical risks \
                 relate to engineering feasibility, Safety risks concern harm to \
                 people, Regulatory risks involve compliance, etc."
                    .into(),
            ),
            kind: PromptKind::Choice(
                RiskCategory::all()
                    .iter()
                    .map(|c| ChoiceOption::new(c.id(), c.label()))
                    .collect(),
            ),
            required: true,
            default: None,
        },
        WizardStep {
            id: "severity".into(),
            prompt: "How severe would the consequence be if this risk occurs?".into(),
            explanation: Some(
                "Severity rates the worst-case impact (MIL-STD-882E scale):\n\
                 1 = Negligible (minor inconvenience)\n\
                 2 = Marginal (minor injury or degradation)\n\
                 3 = Moderate (significant damage or injury)\n\
                 4 = Critical (severe injury or major system loss)\n\
                 5 = Catastrophic (death or total system loss)"
                    .into(),
            ),
            kind: PromptKind::Choice(
                SeverityLevel::all()
                    .iter()
                    .map(|s| {
                        ChoiceOption::new(
                            &s.value().to_string(),
                            &format!("{} ({})", s.label(), s.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: None,
        },
        WizardStep {
            id: "likelihood".into(),
            prompt: "How likely is this risk to occur?".into(),
            explanation: Some(
                "Likelihood rates the probability of occurrence:\n\
                 1 = Improbable (extremely unlikely)\n\
                 2 = Remote (unlikely but possible)\n\
                 3 = Occasional (may occur sometimes)\n\
                 4 = Probable (will occur several times)\n\
                 5 = Frequent (likely to occur often)"
                    .into(),
            ),
            kind: PromptKind::Choice(
                LikelihoodLevel::all()
                    .iter()
                    .map(|l| {
                        ChoiceOption::new(
                            &l.value().to_string(),
                            &format!("{} ({})", l.label(), l.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: None,
        },
        WizardStep {
            id: "detectability".into(),
            prompt: "How easy is it to detect this risk before it causes harm?".into(),
            explanation: Some(
                "Detectability rates how well existing controls can catch the risk \
                 (FMEA convention -- higher score = harder to detect):\n\
                 1 = Almost Certain (current controls will almost always detect)\n\
                 2 = High (good chance of detection)\n\
                 3 = Moderate (may or may not be detected)\n\
                 4 = Low (unlikely to be detected)\n\
                 5 = Almost Impossible (no known detection method)"
                    .into(),
            ),
            kind: PromptKind::Choice(
                DetectabilityLevel::all()
                    .iter()
                    .map(|d| {
                        ChoiceOption::new(
                            &d.value().to_string(),
                            &format!("{} ({})", d.label(), d.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: None,
        },
        WizardStep::string("owner", "Who owns this risk?")
            .with_explanation("The person or team responsible for tracking and mitigating this risk.")
            .optional(),
        WizardStep::string("notes", "Any additional notes?")
            .with_explanation(
                "Free-form notes, context, or references. These are stored in \
                 the risk record for future reference.",
            )
            .optional(),
    ]
}

/// Build a wizard flow for assessing an existing risk.
///
/// Pre-populates context from the risk being assessed.
pub fn build_assessment_wizard(risk: &RiskDef) -> Vec<WizardStep> {
    let context = format!(
        "Assessing risk: {} (current RPN: {})",
        risk.title,
        risk.rpn.map_or("N/A".to_string(), |r| r.to_string()),
    );

    vec![
        WizardStep {
            id: "severity".into(),
            prompt: format!("{context}\nWhat is the current severity?"),
            explanation: Some(
                "Re-evaluate the severity for this assessment cycle. Has the \
                 potential impact changed since last review?"
                    .into(),
            ),
            kind: PromptKind::Choice(
                SeverityLevel::all()
                    .iter()
                    .map(|s| {
                        ChoiceOption::new(
                            &s.value().to_string(),
                            &format!("{} ({})", s.label(), s.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: risk.severity.map(|s| s.value().to_string()),
        },
        WizardStep {
            id: "likelihood".into(),
            prompt: "What is the current likelihood?".into(),
            explanation: Some(
                "Has the probability changed due to new information, design \
                 changes, or implemented mitigations?"
                    .into(),
            ),
            kind: PromptKind::Choice(
                LikelihoodLevel::all()
                    .iter()
                    .map(|l| {
                        ChoiceOption::new(
                            &l.value().to_string(),
                            &format!("{} ({})", l.label(), l.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: risk.likelihood.map(|l| l.value().to_string()),
        },
        WizardStep {
            id: "detectability".into(),
            prompt: "What is the current detectability?".into(),
            explanation: Some(
                "Have new tests, inspections, or monitoring been added that \
                 improve detection capability?"
                    .into(),
            ),
            kind: PromptKind::Choice(
                DetectabilityLevel::all()
                    .iter()
                    .map(|d| {
                        ChoiceOption::new(
                            &d.value().to_string(),
                            &format!("{} ({})", d.label(), d.value()),
                        )
                    })
                    .collect(),
            ),
            required: true,
            default: risk.detectability.map(|d| d.value().to_string()),
        },
        WizardStep::string("notes", "Assessment notes")
            .with_explanation("Record rationale for any changes in the assessment scores.")
            .optional(),
    ]
}

// =========================================================================
// Risk matrix
// =========================================================================

/// A 5x5 risk matrix indexed by severity (rows) and likelihood (columns).
///
/// Each cell contains the IDs of risks that fall in that position.
#[derive(Debug, Clone, Serialize)]
pub struct RiskMatrix {
    /// `cells[severity_index][likelihood_index]` where both indices are 0-4
    /// (corresponding to levels 1-5).
    pub cells: [[Vec<String>; 5]; 5],
}

impl RiskMatrix {
    /// Create an empty matrix.
    pub fn new() -> Self {
        Self {
            cells: Default::default(),
        }
    }

    /// Render the matrix as ASCII art.
    ///
    /// Rows are severity (top = Catastrophic), columns are likelihood
    /// (left = Improbable).
    pub fn to_text(&self) -> String {
        let mut out = String::new();

        // Column headers
        out.push_str("                  | Improb. | Remote  | Occasnl | Probabl | Frequnt |\n");
        out.push_str("------------------+---------+---------+---------+---------+---------+\n");

        // Rows from highest severity (index 4 = Catastrophic) to lowest
        let row_labels = [
            "Negligible",
            "Marginal  ",
            "Moderate  ",
            "Critical  ",
            "Catastroph",
        ];
        for sev_idx in (0..5).rev() {
            out.push_str(&format!("{:18}|", row_labels[sev_idx]));
            for lik_idx in 0..5 {
                let ids = &self.cells[sev_idx][lik_idx];
                let cell_text = if ids.is_empty() {
                    "   -   ".to_string()
                } else if ids.len() == 1 {
                    format!("{:^7}", truncate_id(&ids[0], 7))
                } else {
                    format!("{:^7}", format!("({})", ids.len()))
                };
                out.push_str(&format!(" {} |", cell_text));
            }
            out.push('\n');
        }

        out
    }
}

impl Default for RiskMatrix {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate a string to `max_len`, adding ".." if truncated.
fn truncate_id(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 2 {
        format!("{}..", &s[..max_len - 2])
    } else {
        s[..max_len].to_string()
    }
}

/// Organize risks into a 5x5 severity x likelihood grid.
///
/// Risks that lack either severity or likelihood are excluded from the
/// matrix.
pub fn generate_risk_matrix(risks: &[RiskDef]) -> RiskMatrix {
    let mut matrix = RiskMatrix::new();

    for risk in risks {
        if let (Some(sev), Some(lik)) = (&risk.severity, &risk.likelihood) {
            let sev_idx = (sev.value() - 1) as usize;
            let lik_idx = (lik.value() - 1) as usize;
            matrix.cells[sev_idx][lik_idx].push(risk.id.clone());
        }
    }

    matrix
}

// =========================================================================
// FMEA table
// =========================================================================

/// A single row in an FMEA (Failure Mode and Effects Analysis) worksheet.
#[derive(Debug, Clone, Serialize)]
pub struct FmeaRow {
    pub item: String,
    pub failure_mode: String,
    pub severity: u32,
    pub likelihood: u32,
    pub detectability: u32,
    pub rpn: u32,
    pub mitigation: String,
    pub status: String,
}

/// Generate FMEA worksheet data from risk definitions.
///
/// Each risk becomes one row. Risks with incomplete scoring data use 0 for
/// missing fields and an RPN of 0.
pub fn generate_fmea_table(risks: &[RiskDef]) -> Vec<FmeaRow> {
    risks
        .iter()
        .map(|risk| {
            let sev = risk.severity.map(|s| s.value()).unwrap_or(0);
            let lik = risk.likelihood.map(|l| l.value()).unwrap_or(0);
            let det = risk.detectability.map(|d| d.value()).unwrap_or(0);
            let rpn = risk.rpn.unwrap_or(0);

            FmeaRow {
                item: risk.id.clone(),
                failure_mode: risk.title.clone(),
                severity: sev,
                likelihood: lik,
                detectability: det,
                rpn,
                mitigation: risk.notes.clone().unwrap_or_default(),
                status: risk
                    .status
                    .map(|s| s.label().to_string())
                    .unwrap_or_else(|| "Unknown".to_string()),
            }
        })
        .collect()
}

// =========================================================================
// Risk trend
// =========================================================================

/// Extract (date, RPN) pairs from a series of assessments for trend charting.
///
/// Returns assessments in the order given, using the index as a stand-in
/// date string of the form `"#0"`, `"#1"`, etc.  Callers with access to
/// record envelopes should substitute real timestamps from the envelope's
/// `created` field.
pub fn risk_trend(assessments: &[RiskAssessment]) -> Vec<(String, u32)> {
    assessments
        .iter()
        .enumerate()
        .map(|(i, a)| (format!("#{i}"), a.rpn))
        .collect()
}

// =========================================================================
// SysML generation
// =========================================================================

/// Generate SysML text for a risk definition.
pub fn risk_to_sysml(
    name: &str,
    title: &str,
    severity: &str,
    likelihood: &str,
    detectability: &str,
    category: Option<&str>,
) -> String {
    let mut out = format!("part {} : RiskDef {{\n", name);
    out.push_str(&format!("    doc /* {} */\n", title));
    out.push_str(&format!(
        "    attribute redefines severity = SeverityLevel::{};\n",
        severity
    ));
    out.push_str(&format!(
        "    attribute redefines likelihood = LikelihoodLevel::{};\n",
        likelihood
    ));
    out.push_str(&format!(
        "    attribute redefines detectability = DetectabilityLevel::{};\n",
        detectability
    ));
    if let Some(cat) = category {
        out.push_str(&format!(
            "    attribute redefines category = RiskCategory::{};\n",
            cat
        ));
    }
    out.push_str("}\n");
    out
}

/// Build wizard steps for interactive risk creation.
/// If a model is provided, enum choices are extracted from it.
pub fn build_risk_add_wizard(model: Option<&sysml_core::model::Model>) -> Vec<sysml_core::interactive::WizardStep> {
    use sysml_core::interactive::*;

    let severity_choices = if let Some(m) = model {
        let choices = sysml_core::query::get_enum_choices(m, "SeverityLevel");
        if !choices.is_empty() {
            choices.into_iter().map(|(v, d)| {
                let mut opt = ChoiceOption::new(&v, &v);
                if let Some(desc) = d {
                    opt = opt.with_description(desc);
                }
                opt
            }).collect()
        } else {
            default_severity_choices()
        }
    } else {
        default_severity_choices()
    };

    let likelihood_choices = if let Some(m) = model {
        let choices = sysml_core::query::get_enum_choices(m, "LikelihoodLevel");
        if !choices.is_empty() {
            choices.into_iter().map(|(v, _)| ChoiceOption::new(&v, &v)).collect()
        } else {
            default_likelihood_choices()
        }
    } else {
        default_likelihood_choices()
    };

    let detectability_choices = if let Some(m) = model {
        let choices = sysml_core::query::get_enum_choices(m, "DetectabilityLevel");
        if !choices.is_empty() {
            choices.into_iter().map(|(v, _)| ChoiceOption::new(&v, &v)).collect()
        } else {
            default_detectability_choices()
        }
    } else {
        default_detectability_choices()
    };

    vec![
        WizardStep::string("title", "Risk title")
            .with_explanation("A brief description of the risk being identified."),
        WizardStep {
            id: "severity".into(),
            prompt: "Severity level".into(),
            explanation: Some("How severe would the impact be if this risk occurs?".into()),
            kind: PromptKind::Choice(severity_choices),
            required: true,
            default: None,
        },
        WizardStep {
            id: "likelihood".into(),
            prompt: "Likelihood level".into(),
            explanation: Some("How likely is this risk to occur?".into()),
            kind: PromptKind::Choice(likelihood_choices),
            required: true,
            default: None,
        },
        WizardStep {
            id: "detectability".into(),
            prompt: "Detectability level".into(),
            explanation: Some("How easily can this risk be detected before impact?".into()),
            kind: PromptKind::Choice(detectability_choices),
            required: true,
            default: None,
        },
    ]
}

/// Interpret wizard results into (element_name, sysml_text).
pub fn interpret_risk_add_wizard(result: &sysml_core::interactive::WizardResult) -> Option<(String, String)> {
    let title = result.get_string("title")?;
    let severity = result.get_string("severity")?;
    let likelihood = result.get_string("likelihood")?;
    let detectability = result.get_string("detectability")?;

    // Generate a camelCase name from the title
    let name = format!("risk{}", title_to_identifier(title));
    let sysml = risk_to_sysml(&name, title, severity, likelihood, detectability, None);
    Some((name, sysml))
}

fn title_to_identifier(title: &str) -> String {
    title
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect()
}

fn default_severity_choices() -> Vec<sysml_core::interactive::ChoiceOption> {
    use sysml_core::interactive::ChoiceOption;
    vec![
        ChoiceOption::new("negligible", "negligible"),
        ChoiceOption::new("marginal", "marginal"),
        ChoiceOption::new("moderate", "moderate"),
        ChoiceOption::new("critical", "critical"),
        ChoiceOption::new("catastrophic", "catastrophic"),
    ]
}

fn default_likelihood_choices() -> Vec<sysml_core::interactive::ChoiceOption> {
    use sysml_core::interactive::ChoiceOption;
    vec![
        ChoiceOption::new("improbable", "improbable"),
        ChoiceOption::new("remote", "remote"),
        ChoiceOption::new("occasional", "occasional"),
        ChoiceOption::new("probable", "probable"),
        ChoiceOption::new("frequent", "frequent"),
    ]
}

fn default_detectability_choices() -> Vec<sysml_core::interactive::ChoiceOption> {
    use sysml_core::interactive::ChoiceOption;
    vec![
        ChoiceOption::new("high", "high"),
        ChoiceOption::new("moderate", "moderate"),
        ChoiceOption::new("low", "low"),
        ChoiceOption::new("very_low", "very low"),
        ChoiceOption::new("undetectable", "undetectable"),
    ]
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::model::{Definition, Model, Span, Usage};

    // -- Enum tests ---------------------------------------------------------

    #[test]
    fn severity_values_and_labels() {
        assert_eq!(SeverityLevel::Negligible.value(), 1);
        assert_eq!(SeverityLevel::Catastrophic.value(), 5);
        assert_eq!(SeverityLevel::Moderate.label(), "Moderate");
        assert_eq!(SeverityLevel::all().len(), 5);
    }

    #[test]
    fn severity_from_value_roundtrip() {
        for s in SeverityLevel::all() {
            assert_eq!(SeverityLevel::from_value(s.value()), Some(*s));
        }
        assert_eq!(SeverityLevel::from_value(0), None);
        assert_eq!(SeverityLevel::from_value(6), None);
    }

    #[test]
    fn likelihood_values_and_labels() {
        assert_eq!(LikelihoodLevel::Improbable.value(), 1);
        assert_eq!(LikelihoodLevel::Frequent.value(), 5);
        assert_eq!(LikelihoodLevel::Occasional.label(), "Occasional");
        assert_eq!(LikelihoodLevel::all().len(), 5);
    }

    #[test]
    fn likelihood_from_value_roundtrip() {
        for l in LikelihoodLevel::all() {
            assert_eq!(LikelihoodLevel::from_value(l.value()), Some(*l));
        }
        assert_eq!(LikelihoodLevel::from_value(0), None);
        assert_eq!(LikelihoodLevel::from_value(6), None);
    }

    #[test]
    fn detectability_values_and_labels() {
        assert_eq!(DetectabilityLevel::AlmostCertain.value(), 1);
        assert_eq!(DetectabilityLevel::AlmostImpossible.value(), 5);
        assert_eq!(DetectabilityLevel::Moderate.label(), "Moderate");
        assert_eq!(DetectabilityLevel::all().len(), 5);
    }

    #[test]
    fn detectability_from_value_roundtrip() {
        for d in DetectabilityLevel::all() {
            assert_eq!(DetectabilityLevel::from_value(d.value()), Some(*d));
        }
        assert_eq!(DetectabilityLevel::from_value(0), None);
        assert_eq!(DetectabilityLevel::from_value(6), None);
    }

    #[test]
    fn risk_category_all_and_labels() {
        assert_eq!(RiskCategory::all().len(), 7);
        assert_eq!(RiskCategory::Technical.label(), "Technical");
        assert_eq!(RiskCategory::SupplyChain.label(), "Supply Chain");
    }

    #[test]
    fn risk_category_from_str_roundtrip() {
        assert_eq!(
            RiskCategory::from_str_value("technical"),
            Some(RiskCategory::Technical)
        );
        assert_eq!(
            RiskCategory::from_str_value("supply_chain"),
            Some(RiskCategory::SupplyChain)
        );
        assert_eq!(RiskCategory::from_str_value("unknown"), None);
    }

    #[test]
    fn risk_status_labels() {
        assert_eq!(RiskStatus::Identified.label(), "Identified");
        assert_eq!(RiskStatus::Mitigating.label(), "Mitigating");
        assert_eq!(RiskStatus::Closed.label(), "Closed");
    }

    #[test]
    fn risk_status_from_str_roundtrip() {
        assert_eq!(
            RiskStatus::from_str_value("analyzing"),
            Some(RiskStatus::Analyzing)
        );
        assert_eq!(
            RiskStatus::from_str_value("accepted"),
            Some(RiskStatus::Accepted)
        );
        assert_eq!(RiskStatus::from_str_value("bogus"), None);
    }

    #[test]
    fn mitigation_strategy_labels() {
        assert_eq!(MitigationStrategy::Avoid.label(), "Avoid");
        assert_eq!(MitigationStrategy::Contingency.label(), "Contingency");
    }

    #[test]
    fn mitigation_strategy_from_str_roundtrip() {
        assert_eq!(
            MitigationStrategy::from_str_value("reduce"),
            Some(MitigationStrategy::Reduce)
        );
        assert_eq!(
            MitigationStrategy::from_str_value("transfer"),
            Some(MitigationStrategy::Transfer)
        );
        assert_eq!(MitigationStrategy::from_str_value("nope"), None);
    }

    // -- RPN computation ----------------------------------------------------

    #[test]
    fn compute_rpn_min() {
        let rpn = compute_rpn(
            &SeverityLevel::Negligible,
            &LikelihoodLevel::Improbable,
            &DetectabilityLevel::AlmostCertain,
        );
        assert_eq!(rpn, 1);
    }

    #[test]
    fn compute_rpn_max() {
        let rpn = compute_rpn(
            &SeverityLevel::Catastrophic,
            &LikelihoodLevel::Frequent,
            &DetectabilityLevel::AlmostImpossible,
        );
        assert_eq!(rpn, 125);
    }

    #[test]
    fn compute_rpn_mid() {
        // 3 * 3 * 3 = 27
        let rpn = compute_rpn(
            &SeverityLevel::Moderate,
            &LikelihoodLevel::Occasional,
            &DetectabilityLevel::Moderate,
        );
        assert_eq!(rpn, 27);
    }

    #[test]
    fn compute_rpn_asymmetric() {
        // 5 * 1 * 1 = 5
        let rpn = compute_rpn(
            &SeverityLevel::Catastrophic,
            &LikelihoodLevel::Improbable,
            &DetectabilityLevel::AlmostCertain,
        );
        assert_eq!(rpn, 5);
    }

    // -- RiskDef recompute --------------------------------------------------

    #[test]
    fn risk_def_recompute_rpn_complete() {
        let mut risk = sample_risk();
        assert_eq!(risk.recompute_rpn(), Some(27));
        assert_eq!(risk.rpn, Some(27));
    }

    #[test]
    fn risk_def_recompute_rpn_incomplete() {
        let mut risk = sample_risk();
        risk.detectability = None;
        assert_eq!(risk.recompute_rpn(), None);
        assert_eq!(risk.rpn, None);
    }

    // -- Model extraction ---------------------------------------------------

    #[test]
    fn extract_risks_from_model() {
        let model = build_test_model();
        let risks = extract_risks(&model);
        assert_eq!(risks.len(), 2);

        let thermal = risks.iter().find(|r| r.id == "ThermalRisk").unwrap();
        assert_eq!(thermal.severity, Some(SeverityLevel::Critical));
        assert_eq!(thermal.likelihood, Some(LikelihoodLevel::Occasional));
        assert_eq!(thermal.rpn, Some(4 * 3 * 2));

        let supply = risks.iter().find(|r| r.id == "SupplyChainRisk").unwrap();
        assert!(supply.severity.is_none());
        assert!(supply.rpn.is_none());
    }

    #[test]
    fn extract_risks_ignores_non_risk_defs() {
        let mut model = Model::new("test.sysml".into());
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "Vehicle".into(),
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
        assert!(extract_risks(&model).is_empty());
    }

    #[test]
    fn extract_risks_by_supertype() {
        let mut model = Model::new("test.sysml".into());
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "MyIssue".into(),
            super_type: Some("RiskDef".into()),
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
        let risks = extract_risks(&model);
        assert_eq!(risks.len(), 1);
        assert_eq!(risks[0].id, "MyIssue");
    }

    // -- Record creation ----------------------------------------------------

    #[test]
    fn create_risk_record_structure() {
        let risk = sample_risk();
        let envelope = create_risk_record(&risk, "alice");

        assert_eq!(envelope.meta.tool, "risk");
        assert_eq!(envelope.meta.record_type, "entity");
        assert_eq!(envelope.meta.author, "alice");
        assert!(envelope.refs.contains_key("risk"));
        assert_eq!(
            envelope.data.get("title"),
            Some(&RecordValue::String("ThermalRisk".into()))
        );
        assert_eq!(
            envelope.data.get("severity"),
            Some(&RecordValue::Integer(3))
        );
        assert_eq!(
            envelope.data.get("rpn"),
            Some(&RecordValue::Integer(27))
        );
    }

    #[test]
    fn create_risk_record_minimal() {
        let risk = RiskDef {
            id: "R1".into(),
            title: "Minimal".into(),
            category: None,
            status: None,
            severity: None,
            likelihood: None,
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        };
        let envelope = create_risk_record(&risk, "bob");
        assert_eq!(envelope.meta.tool, "risk");
        // Should not have severity/likelihood/rpn keys
        assert!(!envelope.data.contains_key("severity"));
        assert!(!envelope.data.contains_key("rpn"));
    }

    #[test]
    fn create_assessment_record_structure() {
        let assessment = sample_assessment();
        let envelope = create_assessment_record(&assessment, "charlie");

        assert_eq!(envelope.meta.tool, "risk");
        assert_eq!(envelope.meta.record_type, "assessment");
        assert_eq!(
            envelope.data.get("rpn"),
            Some(&RecordValue::Integer(60))
        );
        assert_eq!(
            envelope.data.get("assessed_by"),
            Some(&RecordValue::String("charlie_assessor".into()))
        );
    }

    #[test]
    fn create_mitigation_record_structure() {
        let mitigation = sample_mitigation();
        let envelope = create_mitigation_record(&mitigation, "dave");

        assert_eq!(envelope.meta.tool, "risk");
        assert_eq!(envelope.meta.record_type, "mitigation");
        assert!(envelope.refs.contains_key("risk"));
        assert!(envelope.refs.contains_key("mitigation"));
        assert_eq!(
            envelope.data.get("strategy"),
            Some(&RecordValue::String("Reduce".into()))
        );
        assert_eq!(
            envelope.data.get("due_date"),
            Some(&RecordValue::String("2026-06-01".into()))
        );
    }

    #[test]
    fn record_toml_round_trip() {
        let risk = sample_risk();
        let envelope = create_risk_record(&risk, "tester");
        let toml = envelope.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.meta.tool, "risk");
        assert_eq!(parsed.meta.record_type, "entity");
        assert_eq!(parsed.data.get("title"), envelope.data.get("title"));
        assert_eq!(parsed.data.get("rpn"), envelope.data.get("rpn"));
    }

    // -- Wizard tests -------------------------------------------------------

    #[test]
    fn risk_wizard_has_expected_steps() {
        let steps = build_risk_wizard();
        assert_eq!(steps.len(), 7);

        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["title", "category", "severity", "likelihood", "detectability", "owner", "notes"]
        );

        // Title is required, notes is optional
        assert!(steps[0].required);
        assert!(!steps[6].required);

        // All steps have explanations
        for step in &steps {
            assert!(
                step.explanation.is_some(),
                "step '{}' should have an explanation",
                step.id
            );
        }
    }

    #[test]
    fn risk_wizard_category_choices() {
        let steps = build_risk_wizard();
        let cat_step = &steps[1];
        if let PromptKind::Choice(opts) = &cat_step.kind {
            assert_eq!(opts.len(), RiskCategory::all().len());
            assert_eq!(opts[0].value, "technical");
            assert_eq!(opts[0].label, "Technical");
        } else {
            panic!("expected Choice for category step");
        }
    }

    #[test]
    fn risk_wizard_severity_choices() {
        let steps = build_risk_wizard();
        let sev_step = &steps[2];
        if let PromptKind::Choice(opts) = &sev_step.kind {
            assert_eq!(opts.len(), 5);
            assert_eq!(opts[0].value, "1");
            assert!(opts[0].label.contains("Negligible"));
            assert_eq!(opts[4].value, "5");
            assert!(opts[4].label.contains("Catastrophic"));
        } else {
            panic!("expected Choice for severity step");
        }
    }

    #[test]
    fn assessment_wizard_has_defaults() {
        let risk = sample_risk();
        let steps = build_assessment_wizard(&risk);
        assert_eq!(steps.len(), 4);

        // Severity step should have a default matching the risk's severity
        assert_eq!(steps[0].default.as_deref(), Some("3"));
        // Likelihood step
        assert_eq!(steps[1].default.as_deref(), Some("3"));
        // Detectability step
        assert_eq!(steps[2].default.as_deref(), Some("3"));
    }

    #[test]
    fn assessment_wizard_no_defaults_when_none() {
        let risk = RiskDef {
            id: "R1".into(),
            title: "Incomplete".into(),
            category: None,
            status: None,
            severity: None,
            likelihood: None,
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        };
        let steps = build_assessment_wizard(&risk);
        assert!(steps[0].default.is_none());
        assert!(steps[1].default.is_none());
        assert!(steps[2].default.is_none());
    }

    // -- Risk matrix --------------------------------------------------------

    #[test]
    fn generate_risk_matrix_empty() {
        let matrix = generate_risk_matrix(&[]);
        for row in &matrix.cells {
            for cell in row {
                assert!(cell.is_empty());
            }
        }
    }

    #[test]
    fn generate_risk_matrix_placement() {
        let risks = vec![
            RiskDef {
                id: "R1".into(),
                title: "R1".into(),
                category: None,
                status: None,
                severity: Some(SeverityLevel::Critical),       // 4 -> idx 3
                likelihood: Some(LikelihoodLevel::Occasional), // 3 -> idx 2
                detectability: None,
                rpn: None,
                owner: None,
                notes: None,
            },
            RiskDef {
                id: "R2".into(),
                title: "R2".into(),
                category: None,
                status: None,
                severity: Some(SeverityLevel::Critical),       // 4 -> idx 3
                likelihood: Some(LikelihoodLevel::Occasional), // 3 -> idx 2
                detectability: None,
                rpn: None,
                owner: None,
                notes: None,
            },
        ];

        let matrix = generate_risk_matrix(&risks);
        assert_eq!(matrix.cells[3][2].len(), 2);
        assert!(matrix.cells[3][2].contains(&"R1".to_string()));
        assert!(matrix.cells[3][2].contains(&"R2".to_string()));

        // Other cells should be empty
        assert!(matrix.cells[0][0].is_empty());
        assert!(matrix.cells[4][4].is_empty());
    }

    #[test]
    fn generate_risk_matrix_skips_incomplete() {
        let risks = vec![RiskDef {
            id: "R1".into(),
            title: "R1".into(),
            category: None,
            status: None,
            severity: Some(SeverityLevel::Critical),
            likelihood: None, // missing
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        }];

        let matrix = generate_risk_matrix(&risks);
        for row in &matrix.cells {
            for cell in row {
                assert!(cell.is_empty());
            }
        }
    }

    #[test]
    fn risk_matrix_to_text_contains_headers() {
        let matrix = generate_risk_matrix(&[]);
        let text = matrix.to_text();
        assert!(text.contains("Improb."));
        assert!(text.contains("Frequnt"));
        assert!(text.contains("Catastroph"));
        assert!(text.contains("Negligible"));
    }

    #[test]
    fn risk_matrix_to_text_shows_risk_id() {
        let risks = vec![RiskDef {
            id: "R1".into(),
            title: "R1".into(),
            category: None,
            status: None,
            severity: Some(SeverityLevel::Negligible),
            likelihood: Some(LikelihoodLevel::Improbable),
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        }];
        let text = generate_risk_matrix(&risks).to_text();
        assert!(text.contains("R1"));
    }

    #[test]
    fn risk_matrix_to_text_shows_count_for_multiple() {
        let risks = vec![
            RiskDef {
                id: "A".into(),
                title: "A".into(),
                category: None,
                status: None,
                severity: Some(SeverityLevel::Critical),
                likelihood: Some(LikelihoodLevel::Frequent),
                detectability: None,
                rpn: None,
                owner: None,
                notes: None,
            },
            RiskDef {
                id: "B".into(),
                title: "B".into(),
                category: None,
                status: None,
                severity: Some(SeverityLevel::Critical),
                likelihood: Some(LikelihoodLevel::Frequent),
                detectability: None,
                rpn: None,
                owner: None,
                notes: None,
            },
        ];
        let text = generate_risk_matrix(&risks).to_text();
        assert!(text.contains("(2)"));
    }

    // -- FMEA table ---------------------------------------------------------

    #[test]
    fn generate_fmea_table_complete() {
        let risk = sample_risk();
        let rows = generate_fmea_table(&[risk]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item, "ThermalRisk");
        assert_eq!(rows[0].severity, 3);
        assert_eq!(rows[0].likelihood, 3);
        assert_eq!(rows[0].detectability, 3);
        assert_eq!(rows[0].rpn, 27);
    }

    #[test]
    fn generate_fmea_table_incomplete() {
        let risk = RiskDef {
            id: "R1".into(),
            title: "Partial".into(),
            category: None,
            status: None,
            severity: Some(SeverityLevel::Critical),
            likelihood: None,
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        };
        let rows = generate_fmea_table(&[risk]);
        assert_eq!(rows[0].severity, 4);
        assert_eq!(rows[0].likelihood, 0);
        assert_eq!(rows[0].detectability, 0);
        assert_eq!(rows[0].rpn, 0);
        assert_eq!(rows[0].status, "Unknown");
    }

    #[test]
    fn generate_fmea_table_empty() {
        assert!(generate_fmea_table(&[]).is_empty());
    }

    #[test]
    fn generate_fmea_table_preserves_status() {
        let risk = RiskDef {
            id: "R1".into(),
            title: "R1".into(),
            category: None,
            status: Some(RiskStatus::Mitigating),
            severity: None,
            likelihood: None,
            detectability: None,
            rpn: None,
            owner: None,
            notes: None,
        };
        let rows = generate_fmea_table(&[risk]);
        assert_eq!(rows[0].status, "Mitigating");
    }

    // -- Risk trend ---------------------------------------------------------

    #[test]
    fn risk_trend_empty() {
        assert!(risk_trend(&[]).is_empty());
    }

    #[test]
    fn risk_trend_ordering() {
        let assessments = vec![
            RiskAssessment {
                risk_id: "R1".into(),
                severity: SeverityLevel::Critical,
                likelihood: LikelihoodLevel::Frequent,
                detectability: DetectabilityLevel::Low,
                rpn: 80,
                notes: None,
                assessed_by: "alice".into(),
            },
            RiskAssessment {
                risk_id: "R1".into(),
                severity: SeverityLevel::Moderate,
                likelihood: LikelihoodLevel::Occasional,
                detectability: DetectabilityLevel::Moderate,
                rpn: 27,
                notes: None,
                assessed_by: "alice".into(),
            },
        ];
        let trend = risk_trend(&assessments);
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[0], ("#0".to_string(), 80));
        assert_eq!(trend[1], ("#1".to_string(), 27));
    }

    #[test]
    fn risk_trend_single_assessment() {
        let assessments = vec![sample_assessment()];
        let trend = risk_trend(&assessments);
        assert_eq!(trend.len(), 1);
        assert_eq!(trend[0].1, 60);
    }

    // -- Helper: truncate_id ------------------------------------------------

    #[test]
    fn truncate_id_short() {
        assert_eq!(truncate_id("R1", 7), "R1");
    }

    #[test]
    fn truncate_id_exact() {
        assert_eq!(truncate_id("R123456", 7), "R123456");
    }

    #[test]
    fn truncate_id_long() {
        assert_eq!(truncate_id("ReallyLongRiskId", 7), "Reall..");
    }

    // -- Serialization sanity -----------------------------------------------

    #[test]
    fn risk_def_serializes_to_json() {
        let risk = sample_risk();
        let json = serde_json::to_string(&risk).unwrap();
        assert!(json.contains("\"severity\""));
        assert!(json.contains("\"moderate\""));
    }

    #[test]
    fn fmea_row_serializes_to_json() {
        let risk = sample_risk();
        let rows = generate_fmea_table(&[risk]);
        let json = serde_json::to_string(&rows[0]).unwrap();
        assert!(json.contains("\"rpn\""));
        assert!(json.contains("27"));
    }

    #[test]
    fn risk_matrix_serializes_to_json() {
        let matrix = generate_risk_matrix(&[]);
        let json = serde_json::to_string(&matrix).unwrap();
        assert!(json.contains("\"cells\""));
    }

    // -- SysML generator tests ----------------------------------------------

    #[test]
    fn risk_to_sysml_basic() {
        let sysml = risk_to_sysml(
            "riskBrakeFail", "Brake system failure",
            "critical", "remote", "moderate", None,
        );
        assert!(sysml.contains("part riskBrakeFail : RiskDef {"));
        assert!(sysml.contains("doc /* Brake system failure */"));
        assert!(sysml.contains("SeverityLevel::critical"));
        assert!(sysml.contains("LikelihoodLevel::remote"));
        assert!(sysml.contains("DetectabilityLevel::moderate"));
    }

    #[test]
    fn risk_to_sysml_with_category() {
        let sysml = risk_to_sysml(
            "riskOverheat", "Overheating", "marginal", "occasional",
            "high", Some("safety"),
        );
        assert!(sysml.contains("RiskCategory::safety"));
    }

    #[test]
    fn build_risk_wizard_defaults() {
        let steps = build_risk_add_wizard(None);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].id, "title");
        assert_eq!(steps[1].id, "severity");
        assert_eq!(steps[2].id, "likelihood");
        assert_eq!(steps[3].id, "detectability");
    }

    #[test]
    fn interpret_risk_wizard() {
        use sysml_core::interactive::*;
        let mut result = WizardResult::new();
        result.set("title", WizardAnswer::String("Brake leak".into()));
        result.set("severity", WizardAnswer::String("critical".into()));
        result.set("likelihood", WizardAnswer::String("remote".into()));
        result.set("detectability", WizardAnswer::String("moderate".into()));
        let (name, sysml) = interpret_risk_add_wizard(&result).unwrap();
        assert_eq!(name, "riskBrakeLeak");
        assert!(sysml.contains("SeverityLevel::critical"));
    }

    #[test]
    fn title_to_identifier_converts() {
        assert_eq!(title_to_identifier("brake fluid leak"), "BrakeFluidLeak");
        assert_eq!(title_to_identifier("Overheating"), "Overheating");
    }

    // -- Test helpers -------------------------------------------------------

    fn sample_risk() -> RiskDef {
        RiskDef {
            id: "ThermalRisk".into(),
            title: "ThermalRisk".into(),
            category: Some(RiskCategory::Technical),
            status: Some(RiskStatus::Identified),
            severity: Some(SeverityLevel::Moderate),
            likelihood: Some(LikelihoodLevel::Occasional),
            detectability: Some(DetectabilityLevel::Moderate),
            rpn: Some(27),
            owner: Some("thermal_team".into()),
            notes: Some("Monitor battery temps".into()),
        }
    }

    fn sample_assessment() -> RiskAssessment {
        RiskAssessment {
            risk_id: "ThermalRisk".into(),
            severity: SeverityLevel::Critical,
            likelihood: LikelihoodLevel::Occasional,
            detectability: DetectabilityLevel::Moderate,
            rpn: 60,
            notes: Some("Increased after field reports".into()),
            assessed_by: "charlie_assessor".into(),
        }
    }

    fn sample_mitigation() -> Mitigation {
        Mitigation {
            id: "MIT-001".into(),
            risk_id: "ThermalRisk".into(),
            description: "Add thermal cutoff circuit".into(),
            strategy: MitigationStrategy::Reduce,
            owner: Some("hw_team".into()),
            due_date: Some("2026-06-01".into()),
            status: Some(RiskStatus::Mitigating),
        }
    }

    fn build_test_model() -> Model {
        let mut model = Model::new("risks.sysml".into());

        // A risk def that has "Risk" in the name
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "ThermalRisk".into(),
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

        // Attribute usages inside ThermalRisk
        model.usages.push(Usage {
            kind: "attribute".into(),
            name: "severity".into(),
            type_ref: None,
            span: Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("ThermalRisk".into()),
            multiplicity: None,
            value_expr: Some("4".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        model.usages.push(Usage {
            kind: "attribute".into(),
            name: "likelihood".into(),
            type_ref: None,
            span: Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("ThermalRisk".into()),
            multiplicity: None,
            value_expr: Some("3".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        model.usages.push(Usage {
            kind: "attribute".into(),
            name: "detectability".into(),
            type_ref: None,
            span: Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("ThermalRisk".into()),
            multiplicity: None,
            value_expr: Some("2".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        // Another risk by name, incomplete
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "SupplyChainRisk".into(),
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

        // A non-risk definition (should be ignored)
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "Vehicle".into(),
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

        model
    }
}
