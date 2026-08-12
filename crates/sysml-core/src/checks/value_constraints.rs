//! Evaluate `assert constraint`s against concrete model values (W017).
//!
//! The domain libraries carry their validation rules as constraints —
//! `FmeaRating` asserts `that >= 1 and that <= 10`, `LimitRange` asserts
//! `lower <= nominal and nominal <= upper`. This check makes those rules
//! live: any concrete value in the model whose declared type (directly or
//! transitively) carries constraints is checked, so libraries add lint
//! rules by writing SysML, never Rust.
//!
//! Checked value sites:
//! 1. Metadata annotation values (`@Fmea { severity = 12; }`) — each
//!    field's type comes from the metadata def's attribute declarations.
//! 2. Typed usages with a direct value (`attribute s : FmeaRating = 12;`)
//!    — the value binds `that`.
//! 3. Typed usages with body values (`attribute t : LimitRange {
//!    :>> lower = 5.0; ... }`) — child values bind by name for
//!    multi-attribute constraints.
//!
//! Constraints whose variables cannot be resolved to concrete numbers are
//! skipped — this check never guesses.

use std::collections::HashSet;

use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, DefKind, Model, Usage};
use crate::sim::expr::{Env, Value};

/// Project-wide value-constraint evaluation. `target_files` limits which
/// files receive diagnostics (libraries provide the rules; the checked
/// model provides the values).
pub fn check_value_constraints(models: &[Model], target_files: &[String]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for model in models {
        if !target_files.iter().any(|f| f == &model.file) {
            continue;
        }

        // --- 1. Metadata annotation values -----------------------------
        for ann in &model.annotations {
            let meta_name = simple_name(&ann.metadata_type);
            // Any-kind lookup: the annotation's type name is already strong
            // evidence, and `metadata def` occasionally parses under other
            // def kinds (grammar GLR quirk, fixed upstream).
            let Some(meta_def_model) = find_def_model(models, meta_name, None) else {
                continue;
            };
            for (field, raw) in &ann.values {
                let Some((v, _)) = crate::sim::resolve::parse_value_with_unit(raw) else {
                    continue;
                };
                // The field's declared type on the metadata def.
                let Some(field_type) = member_type(models, meta_def_model, meta_name, field)
                else {
                    continue;
                };
                let mut env = Env::new();
                env.bind("that", Value::Number(v));
                env.bind(field.clone(), Value::Number(v));
                for (cexpr, cname, tname) in constraints_of(models, &field_type) {
                    if let Some(false) = eval_constraint(&cexpr, &env) {
                        diagnostics.push(violation(
                            &model.file,
                            ann.span.clone(),
                            &format!(
                                "@{} {field} = {raw} violates constraint {} of `{}` ({})",
                                meta_name,
                                cname.as_deref().map(|n| format!("`{n}`")).unwrap_or_else(
                                    || "".to_string()
                                ),
                                tname,
                                cexpr.trim()
                            ),
                        ));
                    }
                }
            }
        }

        // --- 2 & 3. Typed usages with concrete values -------------------
        for u in &model.usages {
            let Some(ref t) = u.type_ref else { continue };
            let type_name = simple_name(t).to_string();
            let constraints = constraints_of(models, &type_name);
            if constraints.is_empty() {
                continue;
            }

            let mut env = Env::new();
            let mut bound = false;
            if let Some(ref ve) = u.value_expr {
                if let Some((v, _)) = crate::sim::resolve::parse_value_with_unit(ve) {
                    env.bind("that", Value::Number(v));
                    bound = true;
                }
            }
            for child in body_values(model, u) {
                if let Some(ref ve) = child.value_expr {
                    if let Some((v, _)) = crate::sim::resolve::parse_value_with_unit(ve) {
                        env.bind(child.name.clone(), Value::Number(v));
                        bound = true;
                    }
                }
            }
            if !bound {
                continue;
            }
            for (cexpr, cname, tname) in &constraints {
                if let Some(false) = eval_constraint(cexpr, &env) {
                    diagnostics.push(violation(
                        &model.file,
                        u.span.clone(),
                        &format!(
                            "`{}` violates constraint {} of `{}` ({})",
                            u.name,
                            cname.as_deref().map(|n| format!("`{n}`")).unwrap_or_default(),
                            tname,
                            cexpr.trim()
                        ),
                    ));
                }
            }
        }
    }

    diagnostics
}

fn violation(file: &str, span: crate::model::Span, msg: &str) -> Diagnostic {
    Diagnostic::warning(file, span, codes::CONSTRAINT_VIOLATION, msg.to_string())
        .with_suggestion("the constraint comes from the value's declared type — fix the value or the type".to_string())
}

/// Evaluate a constraint expression; None when it cannot be evaluated
/// (unbound variables, non-numeric operands) — never guess.
fn eval_constraint(expr: &str, env: &Env) -> Option<bool> {
    let parsed = crate::sim::expr_parser::parse_expr_str(expr).ok()?;
    crate::sim::eval::evaluate_constraint(&parsed, env).ok()
}

/// All assert-constraint expressions on a definition and its supertypes:
/// (expression, constraint name, owning type name).
fn constraints_of(models: &[Model], type_name: &str) -> Vec<(String, Option<String>, String)> {
    let mut out = Vec::new();
    let mut current = type_name.to_string();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let Some(m) = find_def_model(models, &current, None) else {
            break;
        };
        for u in &m.usages {
            if u.kind == "constraint"
                && u.parent_def.as_deref() == Some(current.as_str())
            {
                if let Some(ref expr) = u.value_expr {
                    out.push((
                        expr.clone(),
                        if u.name.is_empty() { None } else { Some(u.name.clone()) },
                        current.clone(),
                    ));
                }
            }
        }
        let Some(def) = m.definitions.iter().find(|d| d.name == current) else {
            break;
        };
        match def.super_type.as_deref() {
            Some(s) => current = simple_name(s).to_string(),
            None => break,
        }
    }
    out
}

/// The model that defines `name` (optionally restricted by kind).
fn find_def_model<'a>(
    models: &'a [Model],
    name: &str,
    kind: Option<DefKind>,
) -> Option<&'a Model> {
    models.iter().find(|m| {
        m.definitions
            .iter()
            .any(|d| d.name == name && kind.map_or(true, |k| d.kind == k))
    })
}

/// The declared type of member `field` on definition `def_name`
/// (walking the specialization chain).
fn member_type(
    models: &[Model],
    start_model: &Model,
    def_name: &str,
    field: &str,
) -> Option<String> {
    let mut current = def_name.to_string();
    let mut seen = HashSet::new();
    let mut m = start_model;
    while seen.insert(current.clone()) {
        if let Some(u) = m.usages.iter().find(|u| {
            u.name == field && u.parent_def.as_deref() == Some(current.as_str())
        }) {
            return u.type_ref.as_deref().map(|t| simple_name(t).to_string());
        }
        let def = m.definitions.iter().find(|d| d.name == current)?;
        let sup = simple_name(def.super_type.as_deref()?).to_string();
        m = find_def_model(models, &sup, None)?;
        current = sup;
    }
    None
}

/// Direct body members of a usage carrying values.
fn body_values<'a>(m: &'a Model, parent: &Usage) -> Vec<&'a Usage> {
    m.usages
        .iter()
        .filter(|u| {
            u.parent_def.as_deref() == Some(parent.name.as_str())
                && u.span.start_byte >= parent.span.start_byte
                && u.span.end_byte <= parent.span.end_byte
                && u.span.start_byte != parent.span.start_byte
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    const LIB: &str = r#"
        package Lib {
            metadata def Fmea {
                attribute severity : FmeaRating;
                attribute occurrence : FmeaRating;
            }
            attribute def FmeaRating {
                assert constraint inRange { that >= 1 and that <= 10 }
            }
            attribute def LimitRange {
                attribute nominal : Real;
                attribute lower : Real;
                attribute upper : Real;
                assert constraint wellOrdered { lower <= nominal and nominal <= upper }
            }
        }
    "#;

    fn run(user: &str) -> Vec<String> {
        let models = vec![
            parse_file("lib.sysml", LIB),
            parse_file("user.sysml", user),
        ];
        check_value_constraints(&models, &["user.sysml".to_string()])
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn metadata_value_out_of_range_flagged() {
        let msgs = run(r#"
            package U {
                part def B;
                part b : B {
                    @Fmea { severity = 12; occurrence = 3; }
                }
            }
        "#);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert!(msgs[0].contains("severity = 12"), "{}", msgs[0]);
        assert!(msgs[0].contains("inRange"));
    }

    #[test]
    fn metadata_value_in_range_clean() {
        let msgs = run(r#"
            package U {
                part def B;
                part b : B {
                    @Fmea { severity = 9; occurrence = 3; }
                }
            }
        "#);
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn typed_usage_direct_value_checked() {
        let msgs = run(r#"
            package U {
                part def B {
                    attribute s : FmeaRating = 0;
                }
            }
        "#);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert!(msgs[0].contains("`s`"));
    }

    #[test]
    fn multi_attribute_constraint_checked() {
        let msgs = run(r#"
            package U {
                part def B {
                    attribute t : LimitRange {
                        :>> nominal = 5.0;
                        :>> lower = 6.0;
                        :>> upper = 8.0;
                    }
                }
            }
        "#);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert!(msgs[0].contains("wellOrdered"), "{}", msgs[0]);
    }

    #[test]
    fn unresolved_variables_are_skipped() {
        // Only lower is given — nominal/upper unbound, so no verdict.
        let msgs = run(r#"
            package U {
                part def B {
                    attribute t : LimitRange {
                        :>> lower = 6.0;
                    }
                }
            }
        "#);
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn library_files_are_not_linted() {
        let models = vec![parse_file("lib.sysml", LIB)];
        let out = check_value_constraints(&models, &["other.sysml".to_string()]);
        assert!(out.is_empty());
    }
}
