//! Checks for unsatisfied and unverified requirements.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, DefKind, Model};

pub struct UnsatisfiedReqCheck;

impl Check for UnsatisfiedReqCheck {
    fn name(&self) -> &'static str {
        "unsatisfied"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        // Collect all requirement definition names
        let req_defs: Vec<_> = model
            .definitions
            .iter()
            .filter(|d| d.kind == DefKind::Requirement)
            .collect();

        // Collect all satisfied requirement names (normalize to simple name)
        let satisfied: std::collections::HashSet<&str> = model
            .satisfactions
            .iter()
            .map(|s| simple_name(&s.requirement))
            .collect();

        let mut diagnostics = Vec::new();

        for def in req_defs {
            if !satisfied.contains(def.name.as_str()) {
                diagnostics.push(Diagnostic::warning(
                    &model.file,
                    def.span.clone(),
                    codes::UNSATISFIED_REQ,
                    format!(
                        "requirement def `{}` has no corresponding satisfy statement",
                        def.name,
                    ),
                ));
            }
        }

        diagnostics
    }
}

pub struct UnverifiedReqCheck;

impl Check for UnverifiedReqCheck {
    fn name(&self) -> &'static str {
        "unverified"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let req_defs: Vec<_> = model
            .definitions
            .iter()
            .filter(|d| d.kind == DefKind::Requirement)
            .collect();

        let verified: std::collections::HashSet<&str> = model
            .verifications
            .iter()
            .map(|v| simple_name(&v.requirement))
            .collect();

        let mut diagnostics = Vec::new();

        for def in req_defs {
            if !verified.contains(def.name.as_str()) {
                diagnostics.push(Diagnostic::warning(
                    &model.file,
                    def.span.clone(),
                    codes::UNVERIFIED_REQ,
                    format!(
                        "requirement def `{}` has no corresponding verify statement",
                        def.name,
                    ),
                ));
            }
        }

        diagnostics
    }
}
