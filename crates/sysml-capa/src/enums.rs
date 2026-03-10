/// Shared enums for the quality domain (NCR, CAPA, Process Deviation).

use serde::Serialize;

// =========================================================================
// Nonconformance classification
// =========================================================================

/// Classification of a nonconformance by defect type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonconformanceCategory {
    Dimensional,
    Material,
    Cosmetic,
    Functional,
    Workmanship,
    Documentation,
    Labeling,
    Packaging,
    Contamination,
    Software,
}

impl NonconformanceCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dimensional => "Dimensional",
            Self::Material => "Material",
            Self::Cosmetic => "Cosmetic",
            Self::Functional => "Functional",
            Self::Workmanship => "Workmanship",
            Self::Documentation => "Documentation",
            Self::Labeling => "Labeling",
            Self::Packaging => "Packaging",
            Self::Contamination => "Contamination",
            Self::Software => "Software",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Dimensional => "dimensional",
            Self::Material => "material",
            Self::Cosmetic => "cosmetic",
            Self::Functional => "functional",
            Self::Workmanship => "workmanship",
            Self::Documentation => "documentation",
            Self::Labeling => "labeling",
            Self::Packaging => "packaging",
            Self::Contamination => "contamination",
            Self::Software => "software",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Dimensional, Self::Material, Self::Cosmetic, Self::Functional,
            Self::Workmanship, Self::Documentation, Self::Labeling, Self::Packaging,
            Self::Contamination, Self::Software,
        ]
    }
}

// =========================================================================
// Severity
// =========================================================================

/// Severity classification of a nonconformance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityClass {
    Critical,
    Major,
    Minor,
    Observation,
}

impl SeverityClass {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Observation => "Observation",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Observation => "observation",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Critical, Self::Major, Self::Minor, Self::Observation]
    }
}

// =========================================================================
// Disposition (MRB decision)
// =========================================================================

/// Material review board disposition for a nonconforming item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    UseAsIs,
    Rework,
    Repair,
    Scrap,
    ReturnToVendor,
    SortAndScreen,
    Deviate,
}

impl Disposition {
    pub fn label(&self) -> &'static str {
        match self {
            Self::UseAsIs => "Use As Is",
            Self::Rework => "Rework",
            Self::Repair => "Repair",
            Self::Scrap => "Scrap",
            Self::ReturnToVendor => "Return to Vendor",
            Self::SortAndScreen => "Sort and Screen",
            Self::Deviate => "Deviate",
        }
    }
}

// =========================================================================
// Corrective action types
// =========================================================================

/// Type of corrective or preventive action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectiveActionType {
    DesignChange,
    ProcessChange,
    SupplierChange,
    ToolingChange,
    TrainingRetraining,
    ProcedureUpdate,
    InspectionEnhancement,
    Containment,
    NoActionRequired,
}

impl CorrectiveActionType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DesignChange => "Design Change",
            Self::ProcessChange => "Process Change",
            Self::SupplierChange => "Supplier Change",
            Self::ToolingChange => "Tooling Change",
            Self::TrainingRetraining => "Training/Retraining",
            Self::ProcedureUpdate => "Procedure Update",
            Self::InspectionEnhancement => "Inspection Enhancement",
            Self::Containment => "Containment",
            Self::NoActionRequired => "No Action Required",
        }
    }
}

// =========================================================================
// Root cause analysis methodology
// =========================================================================

/// Root cause analysis methodology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootCauseMethod {
    FiveWhy,
    Fishbone,
    FaultTreeAnalysis,
    EightD,
    KepnerTregoe,
    IsIsNot,
    ParetoAnalysis,
}

impl RootCauseMethod {
    pub fn label(&self) -> &'static str {
        match self {
            Self::FiveWhy => "5 Why",
            Self::Fishbone => "Fishbone (Ishikawa)",
            Self::FaultTreeAnalysis => "Fault Tree Analysis",
            Self::EightD => "8D",
            Self::KepnerTregoe => "Kepner-Tregoe",
            Self::IsIsNot => "IS/IS NOT",
            Self::ParetoAnalysis => "Pareto Analysis",
        }
    }
}

// =========================================================================
// Lifecycle statuses — each quality item type has its own lifecycle
// =========================================================================

/// NCR lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NcrStatus {
    /// NCR created, awaiting review.
    Open,
    /// Under investigation / root cause analysis.
    Investigating,
    /// Disposition determined by MRB.
    Dispositioned,
    /// All corrective actions verified effective.
    Verified,
    /// NCR closed with all actions complete.
    Closed,
    /// Previously closed, reopened due to recurrence or new findings.
    Reopened,
}

impl NcrStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Investigating => "Investigating",
            Self::Dispositioned => "Dispositioned",
            Self::Verified => "Verified",
            Self::Closed => "Closed",
            Self::Reopened => "Reopened",
        }
    }
}

/// CAPA lifecycle status.
///
/// A CAPA is a formal corrective or preventive action program,
/// distinct from the NCR it may originate from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapaStatus {
    /// CAPA initiated, scope being defined.
    Initiated,
    /// Root cause analysis in progress.
    RootCauseAnalysis,
    /// Root cause identified, action plan being developed.
    PlanningActions,
    /// Corrective/preventive actions being implemented.
    Implementing,
    /// Actions complete, effectiveness verification in progress.
    VerifyingEffectiveness,
    /// Verified effective, pending closure approval.
    PendingClosure,
    /// CAPA closed — actions verified effective.
    Closed,
    /// Closed but effectiveness check failed — requires re-evaluation.
    ClosedIneffective,
}

impl CapaStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Initiated => "Initiated",
            Self::RootCauseAnalysis => "Root Cause Analysis",
            Self::PlanningActions => "Planning Actions",
            Self::Implementing => "Implementing",
            Self::VerifyingEffectiveness => "Verifying Effectiveness",
            Self::PendingClosure => "Pending Closure",
            Self::Closed => "Closed",
            Self::ClosedIneffective => "Closed (Ineffective)",
        }
    }
}

/// Process Deviation lifecycle status.
///
/// A process deviation is a planned, approved departure from a
/// standard process — distinct from an NCR (which is unplanned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviationStatus {
    /// Deviation request submitted.
    Requested,
    /// Under review by quality / engineering.
    UnderReview,
    /// Approved — deviation may proceed.
    Approved,
    /// Denied — deviation not permitted.
    Denied,
    /// Deviation in effect (active).
    Active,
    /// Deviation period expired or completed.
    Expired,
    /// Deviation closed after review of impact.
    Closed,
}

impl DeviationStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Requested => "Requested",
            Self::UnderReview => "Under Review",
            Self::Approved => "Approved",
            Self::Denied => "Denied",
            Self::Active => "Active",
            Self::Expired => "Expired",
            Self::Closed => "Closed",
        }
    }
}

/// Deviation scope — what level is being deviated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviationScope {
    /// Single lot or batch.
    Lot,
    /// Specific process step.
    ProcessStep,
    /// Entire product line.
    ProductLine,
    /// Temporary (time-limited).
    Temporary,
    /// Permanent change (will update standard).
    Permanent,
}

impl DeviationScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Lot => "Lot/Batch",
            Self::ProcessStep => "Process Step",
            Self::ProductLine => "Product Line",
            Self::Temporary => "Temporary",
            Self::Permanent => "Permanent",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Lot, Self::ProcessStep, Self::ProductLine, Self::Temporary, Self::Permanent]
    }
}
