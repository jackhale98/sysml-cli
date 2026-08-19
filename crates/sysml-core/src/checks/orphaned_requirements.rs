//! Flags `requirement def` declarations that are never referenced by a
//! satisfy/verify relationship anywhere in the model. Distinct from W002/W003
//! which target requirement *usages*; this check targets *definitions* that
//! were authored but never wired into traceability.

use std::collections::HashSet;

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, DefKind, Model};

pub struct OrphanedRequirementCheck;

impl Check for OrphanedRequirementCheck {
    fn name(&self) -> &'static str {
        "orphan-req"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut referenced: HashSet<&str> = HashSet::new();
        for s in &model.satisfactions {
            referenced.insert(simple_name(&s.requirement));
        }
        for v in &model.verifications {
            referenced.insert(simple_name(&v.requirement));
        }
        // Also count any usage typed by this requirement as a reference.
        for u in &model.usages {
            if let Some(ref t) = u.type_ref {
                referenced.insert(simple_name(t));
            }
        }
        // Treat derivation/refinement (via `subsets` or `redefinition`) as
        // a reference too.
        for u in &model.usages {
            if let Some(ref sub) = u.subsets {
                referenced.insert(simple_name(sub));
            }
            if let Some(ref red) = u.redefinition {
                referenced.insert(simple_name(red));
            }
        }
        // Definitions that specialize a requirement (`requirement def R2 :> R1`)
        // count as references to the parent.
        for d in &model.definitions {
            if let Some(ref s) = d.super_type {
                referenced.insert(simple_name(s));
            }
        }

        // Satisfy/verify statements may target a requirement usage's
        // `<id>` short name (`satisfy reqs.REQ2;`) — resolve those ids to
        // the usage's type so the def counts as referenced.
        let usage_ids: Vec<(&str, &str)> = model
            .usages
            .iter()
            .filter(|u| u.kind == "requirement")
            .filter_map(|u| {
                Some((
                    crate::model::unquote_name(u.short_name.as_deref()?),
                    simple_name(u.type_ref.as_deref()?),
                ))
            })
            .collect();
        let id_referenced: HashSet<&str> = usage_ids
            .iter()
            .filter(|(id, _)| referenced.contains(id))
            .map(|(_, ty)| *ty)
            .collect();

        let mut diagnostics = Vec::new();
        for def in &model.definitions {
            if def.kind != DefKind::Requirement {
                continue;
            }
            if referenced.contains(def.name.as_str()) || id_referenced.contains(def.name.as_str()) {
                continue;
            }
            // Cross-file wiring: referenced (typed, specialized, subset) or
            // satisfied/verified by a sibling file in the same invocation.
            let ext =
                |t: &String| crate::model::target_matches(t, &def.name, def.short_name.as_deref());
            if model.external_references.iter().any(|r| r == &def.name)
                || model.external_satisfied.iter().any(ext)
                || model.external_verified.iter().any(ext)
            {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    &model.file,
                    def.span.clone(),
                    codes::ORPHANED_REQUIREMENT,
                    format!(
                        "requirement def `{}` is never satisfied, verified, or specialized",
                        def.name
                    ),
                )
                .with_suggestion(format!(
                    "either remove `requirement def {}` or wire it via `satisfy`/`verify` from a part",
                    def.name
                )),
            );
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn unreferenced_requirement_warns() {
        let source = "requirement def MaxMass;\n";
        let model = parse_file("test.sysml", source);
        let diags = OrphanedRequirementCheck.run(&model);
        assert!(
            diags
                .iter()
                .any(|d| d.code == codes::ORPHANED_REQUIREMENT && d.message.contains("MaxMass")),
            "unreferenced requirement should warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn satisfied_requirement_does_not_warn() {
        let source = r#"
            requirement def MaxMass;
            part def Vehicle {
                satisfy MaxMass;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = OrphanedRequirementCheck.run(&model);
        assert!(
            !diags.iter().any(|d| d.message.contains("MaxMass")),
            "satisfied requirement should not warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn specialized_requirement_does_not_warn() {
        let source = r#"
            requirement def Base;
            requirement def Derived :> Base;
        "#;
        let model = parse_file("test.sysml", source);
        let diags = OrphanedRequirementCheck.run(&model);
        // Base is specialized -> not orphan; Derived has no satisfy -> still orphan.
        assert!(
            !diags.iter().any(|d| d.message.contains("`Base`")),
            "specialized parent requirement should not warn"
        );
    }

    #[test]
    fn non_requirement_def_is_not_flagged() {
        let source = "part def UnusedPart;\n";
        let model = parse_file("test.sysml", source);
        let diags = OrphanedRequirementCheck.run(&model);
        assert!(diags.is_empty(), "non-requirement defs are not in scope");
    }
}
