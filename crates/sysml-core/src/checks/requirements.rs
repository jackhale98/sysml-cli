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

        let mut diagnostics = Vec::new();

        for def in req_defs {
            if !requirement_traced(model, def, &model.satisfactions, |s| &s.requirement) {
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

/// True when a requirement def is traced by any of `items` — either
/// directly (target names the def or its `<id>`) or through one of its
/// usages (target names a usage typed by the def, or that usage's `<id>`).
fn requirement_traced<'a, T>(
    model: &'a Model,
    def: &crate::model::Definition,
    items: &'a [T],
    target: impl Fn(&'a T) -> &'a str,
) -> bool {
    use crate::model::target_matches;
    let usages: Vec<_> = model
        .usages
        .iter()
        .filter(|u| {
            u.kind == "requirement"
                && u.type_ref
                    .as_deref()
                    .is_some_and(|t| simple_name(t) == def.name)
        })
        .collect();
    items.iter().any(|item| {
        let t = target(item);
        target_matches(t, &def.name, def.short_name.as_deref())
            || usages
                .iter()
                .any(|u| target_matches(t, &u.name, u.short_name.as_deref()))
    })
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

        let mut diagnostics = Vec::new();

        for def in req_defs {
            if !requirement_traced(model, def, &model.verifications, |v| &v.requirement) {
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
