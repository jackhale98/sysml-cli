/// Quality domain for SysML v2 models.
///
/// Provides three distinct quality item types:
///
/// - **NCR** (Nonconformance Report) — documents an observed nonconformance.
///   NCRs are findings: they describe what went wrong, which part or lot is
///   affected, and what disposition the material review board decides.
///
/// - **CAPA** (Corrective/Preventive Action) — a formal action program to
///   address root causes and prevent recurrence. CAPAs may originate from
///   NCRs, audit findings, customer complaints, or proactive improvement.
///   They are distinct from NCRs — an NCR says "what went wrong", a CAPA
///   says "what we're doing about it."
///
/// - **Process Deviation** — a planned, approved departure from a standard
///   process. Unlike NCRs (which are unplanned), deviations are pre-approved
///   variations with defined scope and duration.
///
/// Each type has its own lifecycle, rules, and record envelope format.
/// Root cause analysis (5 Why, Fishbone) is shared infrastructure used
/// by both NCRs and CAPAs.

pub mod enums;
pub mod ncr;
pub mod capa;
pub mod deviation;
pub mod rca;
pub mod trend;

// Re-export key types at crate root for convenience.
pub use enums::*;
pub use ncr::{Ncr, create_ncr, create_ncr_record, disposition_ncr, link_capa};
pub use capa::{Capa, CapaAction, CapaSource, CapaType, create_capa, create_capa_record,
               add_action, set_root_cause};
pub use deviation::{ProcessDeviation, create_deviation, create_deviation_record,
                    approve_deviation, deny_deviation, activate_deviation};
pub use rca::{RootCauseAnalysis, build_five_why_steps, build_fishbone_steps, create_rca_record};
pub use trend::{TrendItem, trend_analysis, check_escalation};
