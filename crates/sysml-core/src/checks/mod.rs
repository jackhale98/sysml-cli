//! Validation checks for SysML v2 models.
//!
//! Each check module implements the `Check` trait, which takes a `Model`
//! and returns a list of `Diagnostic` entries.

pub mod calculations;
pub mod constraints;
pub mod duplicates;
pub mod imports;
pub mod missing_docs;
pub mod multiplicity;
pub mod naming;
pub mod orphaned_requirements;
pub mod ports;
pub mod references;
pub mod value_constraints;
pub mod requirements;
pub mod self_specialization;
pub mod syntax;
pub mod unbound_ports;

use crate::diagnostic::Diagnostic;
use crate::model::Model;

/// A validation check that can be run against a model.
pub trait Check {
    /// Unique name for this check (used in --disable flag).
    fn name(&self) -> &'static str;

    /// Run the check and return any diagnostics.
    fn run(&self, model: &Model) -> Vec<Diagnostic>;
}

/// All available checks.
pub fn all_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(syntax::SyntaxCheck),
        Box::new(duplicates::DuplicateCheck),
        Box::new(references::UnusedDefCheck),
        Box::new(references::UnresolvedTypeCheck),
        Box::new(requirements::UnsatisfiedReqCheck),
        Box::new(requirements::UnverifiedReqCheck),
        Box::new(ports::PortConnectionCheck),
        Box::new(constraints::ConstraintCheck),
        Box::new(calculations::CalcReturnCheck),
        Box::new(imports::ImportCycleCheck),
        Box::new(multiplicity::MultiplicityCheck),
        Box::new(missing_docs::MissingDocCheck),
        Box::new(naming::NamingConventionCheck),
        Box::new(orphaned_requirements::OrphanedRequirementCheck),
        Box::new(self_specialization::SelfSpecializationCheck),
        Box::new(unbound_ports::UnboundPortCheck),
    ]
}
