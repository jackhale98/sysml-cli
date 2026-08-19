//! Extraction of `Uncertainty::UncertaintyAnalysis` cases from parsed
//! models: finds analysis usages whose type specializes
//! `UncertaintyAnalysis`, resolves their contributions through feature
//! chains to the dimensions that own the values, and produces the pure
//! math inputs for `sim::uncertainty`.
//!
//! Everything is name- and span-based over the extracted `Model`s — the
//! domain semantics (what a contribution is, where severity lives) come
//! from the domain library the model imports, not from code here.

use std::collections::HashSet;

use crate::model::{simple_name, Model, Span, Usage};
use crate::sim::uncertainty::{Distribution, Settings, Target, UncertainInput};

/// A fully resolved uncertainty analysis case, ready to evaluate.
#[derive(Debug, Clone)]
pub struct UncertaintyCase {
    pub name: String,
    pub file: String,
    /// The case's declared type (e.g. `ToleranceStackup`).
    pub type_name: String,
    pub inputs: Vec<UncertainInput>,
    pub target: Target,
    pub settings: Settings,
    /// Common unit of the contributions, when the dimensions declare one.
    pub unit: Option<String>,
    pub critical: bool,
}

/// True when `type_name` (a simple or qualified name) resolves to a
/// definition that is, or transitively specializes, `UncertaintyAnalysis`.
pub fn is_uncertainty_type(models: &[Model], type_name: &str) -> bool {
    let mut current = simple_name(type_name).to_string();
    let mut seen = HashSet::new();
    loop {
        if current == "UncertaintyAnalysis" {
            return true;
        }
        if !seen.insert(current.clone()) {
            return false;
        }
        let Some(def) = find_def(models, &current) else {
            return false;
        };
        let Some(ref sup) = def.super_type else {
            return false;
        };
        current = simple_name(sup).to_string();
    }
}

/// Uncertainty analysis cases as `(name, file, type_name)`, in
/// declaration order: those declared in `content`, with their types resolved against
/// `scope`. The two differ whenever the user names specific files: the
/// case is theirs, but `ToleranceStackup :> UncertaintyCase` is only
/// reachable through the libraries on the include path.
pub fn find_uncertainty_cases(content: &[Model], scope: &[Model]) -> Vec<(String, String, String)> {
    let mut cases = Vec::new();
    for m in content {
        for u in &m.usages {
            if let Some(ref t) = u.type_ref {
                if !u.name.is_empty() && is_uncertainty_type(scope, t) {
                    cases.push((u.name.clone(), m.file.clone(), simple_name(t).to_string()));
                }
            }
        }
    }
    cases
}

/// Extract and fully resolve the named case.
pub fn extract_case(models: &[Model], case_name: &str) -> Result<UncertaintyCase, String> {
    // Locate the case usage.
    let (case_model, case) = models
        .iter()
        .find_map(|m| {
            m.usages
                .iter()
                .find(|u| {
                    u.name == case_name
                        && u.type_ref
                            .as_deref()
                            .is_some_and(|t| is_uncertainty_type(models, t))
                })
                .map(|u| (m, u))
        })
        .ok_or_else(|| {
            format!(
                "no uncertainty analysis case named `{case_name}` found \
                 (is the Uncertainty library on the include path?)"
            )
        })?;
    let type_name = simple_name(case.type_ref.as_deref().unwrap_or("")).to_string();

    // Target: `:>> target { :>> nominal/lower/upper }`.
    let target_usage = child_named(case_model, case, "target")
        .ok_or_else(|| format!("case `{case_name}` has no `target` redefinition"))?;
    let target = Target {
        nominal: child_value_f64(case_model, target_usage, "nominal")
            .ok_or_else(|| format!("target of `{case_name}` has no `nominal`"))?
            .0,
        lower: child_value_f64(case_model, target_usage, "lower")
            .ok_or_else(|| format!("target of `{case_name}` has no `lower`"))?
            .0,
        upper: child_value_f64(case_model, target_usage, "upper")
            .ok_or_else(|| format!("target of `{case_name}` has no `upper`"))?
            .0,
    };

    // Settings, with UncertaintyAnalysis defaults.
    // Settings resolve case body -> type chain defaults -> built-in.
    let mut settings = Settings::default();
    if let Some(v) = setting_f64(models, case_model, case, &type_name, "sigmaLevel") {
        settings.sigma_level = v;
    }
    if let Some(v) = setting_f64(models, case_model, case, &type_name, "meanShiftK") {
        settings.mean_shift_k = v;
    }
    if let Some(v) = setting_f64(models, case_model, case, &type_name, "iterations") {
        settings.iterations = v as u64;
    }
    if let Some(v) = setting_f64(models, case_model, case, &type_name, "marginalFraction") {
        settings.marginal_fraction = v;
    }
    if let Some((v, _)) = child_value_f64(case_model, case, "seed") {
        settings.seed = Some(v as u64);
    }
    let critical = child_named(case_model, case, "critical")
        .and_then(|u| u.value_expr.as_deref())
        .map(|v| v.trim() == "true")
        .unwrap_or(false);

    // Contributions: children subsetting `contributions` or typed
    // `Contribution`, in declaration order.
    let contribs: Vec<&Usage> = children_of(case_model, case)
        .into_iter()
        .filter(|u| {
            // `:> contributions` may land in `subsets` or `type_ref`
            // depending on the CST form; typed `: Contribution` also counts.
            u.subsets
                .as_deref()
                .is_some_and(|s| simple_name(s) == "contributions")
                || u.type_ref.as_deref().is_some_and(|t| {
                    let t = simple_name(t);
                    t == "contributions" || t == "Contribution"
                })
        })
        .collect();
    if contribs.is_empty() {
        return Err(format!(
            "case `{case_name}` has no contributions \
             (subset `contributions` or declare Contribution-typed attributes)"
        ));
    }

    // The scope feature chains resolve against: the case's parent.
    let context = case.parent_def.as_deref().ok_or_else(|| {
        format!("case `{case_name}` has no enclosing part to resolve dimensions in")
    })?;

    let mut inputs = Vec::new();
    let mut units: Vec<Option<String>> = Vec::new();
    for c in &contribs {
        let dim_chain = child_named(case_model, c, "dim")
            .and_then(|u| u.value_expr.clone())
            .ok_or_else(|| format!("contribution `{}` has no `dim` binding", c.name))?;
        let sense = match child_named(case_model, c, "sense").and_then(|u| u.value_expr.as_deref())
        {
            Some(v) if v.contains("negative") => -1.0,
            _ => 1.0,
        };
        let source = child_named(case_model, c, "source")
            .and_then(|u| u.value_expr.as_deref())
            .map(|s| s.trim().trim_matches('"').to_string());

        let dim = resolve_dimension(models, case_model, context, dim_chain.trim())
            .map_err(|e| format!("contribution `{}`: {e}", c.name))?;

        units.push(dim.unit.clone());
        inputs.push(UncertainInput {
            name: c.name.clone(),
            nominal: dim.nominal,
            plus: dim.plus,
            minus: dim.minus,
            sense,
            distribution: dim.distribution,
            source,
        });
    }

    // Unit consistency: unitless everywhere is fine; identical units are
    // fine; mixed units convert into the first explicit unit.
    let common = units.iter().flatten().next().cloned();
    if let Some(ref target_unit) = common {
        for (i, u) in units.iter().enumerate() {
            if let Some(u) = u {
                if u != target_unit {
                    let f = crate::sim::units::convert(1.0, u, target_unit).map_err(|_| {
                        format!(
                            "contribution `{}` is in `{u}` but the chain is in \
                             `{target_unit}` and no conversion is known",
                            inputs[i].name
                        )
                    })?;
                    inputs[i].nominal *= f;
                    inputs[i].plus *= f;
                    inputs[i].minus *= f;
                }
            }
        }
    }

    Ok(UncertaintyCase {
        name: case_name.to_string(),
        file: case_model.file.clone(),
        type_name,
        inputs,
        target,
        settings,
        unit: common,
        critical,
    })
}

/// A resolved dimension: values harvested from the attribute the feature
/// chain points at.
struct DimValues {
    nominal: f64,
    plus: f64,
    minus: f64,
    distribution: Distribution,
    unit: Option<String>,
}

/// Resolve a feature chain like `housing.depth` or `piston.od.diameter`
/// starting from `context` (the definition enclosing the analysis).
fn resolve_dimension(
    models: &[Model],
    start_model: &Model,
    context: &str,
    chain: &str,
) -> Result<DimValues, String> {
    let segments: Vec<&str> = chain.split('.').map(str::trim).collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(format!("malformed feature chain `{chain}`"));
    }

    // Scope = the usage we are currently inside (if any) plus the def
    // whose members are visible.
    let mut scope_usage: Option<(&Model, &Usage)> = None;
    let mut scope_def: String = context.to_string();

    // If the context names a usage rather than a def, hop to its type.
    if find_def(models, &scope_def).is_none() {
        if let Some((m, u)) = find_usage_anywhere(models, start_model, &scope_def) {
            scope_usage = Some((m, u));
            if let Some(ref t) = u.type_ref {
                scope_def = simple_name(t).to_string();
            }
        }
    }

    for (i, seg) in segments.iter().enumerate() {
        let found = lookup_member(models, scope_usage, &scope_def, seg).ok_or_else(|| {
            format!(
                "cannot resolve `{seg}` (segment {} of `{chain}`) in `{}`",
                i + 1,
                scope_def
            )
        })?;
        let (fm, fu) = found;
        if i + 1 == segments.len() {
            return harvest_values(models, fm, fu);
        }
        scope_usage = Some((fm, fu));
        scope_def = fu
            .type_ref
            .as_deref()
            .map(|t| simple_name(t).to_string())
            .unwrap_or_default();
    }
    unreachable!("loop returns on the last segment");
}

/// Find `name` as a member of the current scope: first among the scope
/// usage's body children, then among the scope def's members (walking the
/// specialization chain).
fn lookup_member<'a>(
    models: &'a [Model],
    scope_usage: Option<(&'a Model, &'a Usage)>,
    scope_def: &str,
    name: &str,
) -> Option<(&'a Model, &'a Usage)> {
    if let Some((m, parent)) = scope_usage {
        if let Some(u) = children_of(m, parent).into_iter().find(|u| u.name == name) {
            return Some((m, u));
        }
    }
    // Member of the def (or a supertype), in whichever model defines it.
    let mut current = scope_def.to_string();
    let mut seen = HashSet::new();
    while !current.is_empty() && seen.insert(current.clone()) {
        for m in models {
            let def = m.definitions.iter().find(|d| d.name == current);
            if let Some(def) = def {
                if let Some(u) = m.usages.iter().find(|u| {
                    u.name == name
                        && u.parent_def.as_deref() == Some(current.as_str())
                        && span_within(&u.span, &def.span)
                }) {
                    return Some((m, u));
                }
                if let Some(ref sup) = def.super_type {
                    current = simple_name(sup).to_string();
                } else {
                    current = String::new();
                }
                break;
            }
        }
        if find_def(models, &current).is_none() {
            break;
        }
    }
    None
}

/// Read nominal/plus/minus/distribution/unit from an attribute usage's
/// body redefinitions (with library defaults for the optional ones).
fn harvest_values(_models: &[Model], m: &Model, attr: &Usage) -> Result<DimValues, String> {
    let get = |name: &str| child_value_f64(m, attr, name);

    let (nominal, mut unit) = get("nominal")
        .ok_or_else(|| format!("dimension `{}` has no `nominal` value", attr.name))?;
    let (plus, u2) =
        get("plus").ok_or_else(|| format!("dimension `{}` has no `plus` value", attr.name))?;
    let (minus, u3) =
        get("minus").ok_or_else(|| format!("dimension `{}` has no `minus` value", attr.name))?;
    unit = unit.or(u2).or(u3);

    // Explicit `unit` attribute wins over unit brackets on the numbers.
    if let Some(u) = child_named(m, attr, "unit").and_then(|u| u.value_expr.as_deref()) {
        unit = Some(u.trim().trim_matches('"').to_string());
    }

    let distribution =
        match child_named(m, attr, "distribution").and_then(|u| u.value_expr.as_deref()) {
            Some(v) if v.contains("uniform") => Distribution::Uniform,
            Some(v) if v.contains("triangular") => Distribution::Triangular,
            _ => Distribution::Normal,
        };

    if plus < 0.0 || minus < 0.0 {
        return Err(format!(
            "dimension `{}` has negative bounds (plus and minus are magnitudes)",
            attr.name
        ));
    }

    Ok(DimValues {
        nominal,
        plus,
        minus,
        distribution,
        unit,
    })
}

// --- span/scope helpers -------------------------------------------------

fn span_within(inner: &Span, outer: &Span) -> bool {
    inner.start_byte >= outer.start_byte && inner.end_byte <= outer.end_byte
}

/// Direct body members of a usage: same model, named parent scope, and
/// physically inside its span (the name check alone is ambiguous when
/// several usages share a name, e.g. two `diameter` attributes).
fn children_of<'a>(m: &'a Model, parent: &Usage) -> Vec<&'a Usage> {
    m.usages
        .iter()
        .filter(|u| {
            u.parent_def.as_deref() == Some(parent.name.as_str())
                && span_within(&u.span, &parent.span)
                && u.span.start_byte != parent.span.start_byte
        })
        .collect()
}

fn child_named<'a>(m: &'a Model, parent: &Usage, name: &str) -> Option<&'a Usage> {
    children_of(m, parent).into_iter().find(|u| u.name == name)
}

/// A child's numeric value, tolerating unit brackets (`50.0 [SI::mm]`).
fn child_value_f64(m: &Model, parent: &Usage, name: &str) -> Option<(f64, Option<String>)> {
    let u = child_named(m, parent, name)?;
    let expr = u.value_expr.as_deref()?;
    crate::sim::resolve::parse_value_with_unit(expr)
}

/// A setting's value for this case: the case's own body first, then the
/// defaults declared on its type and every supertype, walking up to
/// `UncertaintyAnalysis`.
///
/// The library is where a default belongs — `attribute marginalFraction :
/// Real default 0.10` in `UncertaintyAnalysis`, or a stricter value on a
/// project's own `analysis def` specializing it. Without this walk, the
/// library's `default` would be documentation and the real value would be
/// a constant in the analyzer, so editing the library would change the
/// docs and nothing else.
fn setting_f64(
    models: &[Model],
    case_model: &Model,
    case: &Usage,
    type_name: &str,
    name: &str,
) -> Option<f64> {
    if let Some((v, _)) = child_value_f64(case_model, case, name) {
        return Some(v);
    }
    let mut current = type_name.to_string();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        for m in models {
            for u in m.usages_in_def(&current) {
                if simple_name(&u.name) == name {
                    if let Some(expr) = u.value_expr.as_deref() {
                        if let Some((v, _)) = crate::sim::resolve::parse_value_with_unit(expr) {
                            return Some(v);
                        }
                    }
                }
            }
        }
        let def = find_def(models, &current)?;
        current = simple_name(def.super_type.as_deref()?).to_string();
    }
}

fn find_def<'a>(models: &'a [Model], name: &str) -> Option<&'a crate::model::Definition> {
    models
        .iter()
        .find_map(|m| m.definitions.iter().find(|d| d.name == name))
}

/// Find a usage by name, preferring the model the analysis lives in.
fn find_usage_anywhere<'a>(
    models: &'a [Model],
    prefer: &'a Model,
    name: &str,
) -> Option<(&'a Model, &'a Usage)> {
    if let Some(u) = prefer.usages.iter().find(|u| u.name == name) {
        return Some((prefer, u));
    }
    models
        .iter()
        .find_map(|m| m.usages.iter().find(|u| u.name == name).map(|u| (m, u)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;
    use crate::sim::uncertainty::{monte_carlo, rss, worst_case, PassFail};

    const LIB: &str = r#"
        package Uncertainty {
            enum def Distribution { enum normal; enum uniform; enum triangular; }
            attribute def UncertainValue {
                attribute nominal : Real;
                attribute plus : Real;
                attribute minus : Real;
                attribute distribution : Distribution default Distribution::normal;
            }
            attribute def LimitRange {
                attribute nominal : Real;
                attribute lower : Real;
                attribute upper : Real;
            }
            analysis def UncertaintyAnalysis {
                attribute target : LimitRange;
                attribute sigmaLevel : Real default 6.0;
                attribute meanShiftK : Real default 0.0;
                attribute iterations : Integer default 10000;
                attribute seed : Integer[0..1];
            }
        }
        package Tolerancing {
            attribute def TolerancedDimension :> UncertainValue {
                attribute unit : String default "mm";
            }
            enum def Sense { enum positive; enum negative; }
            attribute def Contribution {
                attribute dim : TolerancedDimension;
                attribute sense : Sense default Sense::positive;
                attribute source : String[0..1];
            }
            analysis def ToleranceStackup :> UncertaintyAnalysis {
                attribute contributions : Contribution[1..*] ordered;
                attribute critical : Boolean default false;
            }
        }
    "#;

    const MODEL: &str = r#"
        package EnclosureGapExample {
            private import Uncertainty::*;
            private import Tolerancing::*;

            part def Housing {
                attribute depth : TolerancedDimension {
                    :>> nominal = 50.0;
                    :>> plus = 0.1;
                    :>> minus = 0.1;
                }
            }
            part def Cover {
                attribute height : TolerancedDimension {
                    :>> nominal = 45.0;
                    :>> plus = 0.08;
                    :>> minus = 0.08;
                }
            }
            part def Gasket {
                attribute thickness : TolerancedDimension {
                    :>> nominal = 2.0;
                    :>> plus = 0.15;
                    :>> minus = 0.10;
                    :>> distribution = Distribution::uniform;
                }
            }

            part def Enclosure {
                part housing : Housing;
                part cover : Cover;
                part gasket : Gasket;

                analysis gapAnalysis : ToleranceStackup {
                    attribute :>> target {
                        :>> nominal = 3.0;
                        :>> lower = 2.5;
                        :>> upper = 3.5;
                    }
                    attribute :>> critical = true;
                    attribute housingDepth :> contributions {
                        :>> dim = housing.depth;
                        :>> sense = Sense::positive;
                        :>> source = "DWG-001 Rev A";
                    }
                    attribute coverHeight :> contributions {
                        :>> dim = cover.height;
                        :>> sense = Sense::negative;
                    }
                    attribute gasketThickness :> contributions {
                        :>> dim = gasket.thickness;
                        :>> sense = Sense::negative;
                    }
                }
            }
        }
    "#;

    fn models() -> Vec<Model> {
        vec![
            parse_file("lib.sysml", LIB),
            parse_file("model.sysml", MODEL),
        ]
    }

    #[test]
    fn recognizes_uncertainty_types_transitively() {
        let ms = models();
        assert!(is_uncertainty_type(&ms, "UncertaintyAnalysis"));
        assert!(is_uncertainty_type(&ms, "ToleranceStackup"));
        assert!(is_uncertainty_type(&ms, "Tolerancing::ToleranceStackup"));
        assert!(!is_uncertainty_type(&ms, "Contribution"));
        assert!(!is_uncertainty_type(&ms, "NoSuchThing"));
    }

    #[test]
    fn finds_cases() {
        let ms = models();
        let cases = find_uncertainty_cases(&ms, &ms);
        assert_eq!(cases.len(), 1, "{cases:?}");
        assert_eq!(cases[0].0, "gapAnalysis");
        assert_eq!(cases[0].2, "ToleranceStackup");
    }

    #[test]
    fn extracts_full_case() {
        let ms = models();
        let case = extract_case(&ms, "gapAnalysis").expect("extraction");
        assert_eq!(case.type_name, "ToleranceStackup");
        assert!(case.critical);
        assert_eq!(case.inputs.len(), 3);

        let h = &case.inputs[0];
        assert_eq!(h.name, "housingDepth");
        assert!((h.nominal - 50.0).abs() < 1e-9);
        assert!((h.plus - 0.1).abs() < 1e-9);
        assert_eq!(h.sense, 1.0);
        assert_eq!(h.distribution, Distribution::Normal);
        assert_eq!(h.source.as_deref(), Some("DWG-001 Rev A"));

        let g = &case.inputs[2];
        assert_eq!(g.sense, -1.0);
        assert_eq!(g.distribution, Distribution::Uniform);
        assert!((g.plus - 0.15).abs() < 1e-9);
        assert!((g.minus - 0.10).abs() < 1e-9);

        assert!((case.target.nominal - 3.0).abs() < 1e-9);
        assert!((case.target.lower - 2.5).abs() < 1e-9);
        assert!((case.target.upper - 3.5).abs() < 1e-9);
    }

    #[test]
    fn extracted_case_evaluates_end_to_end() {
        let ms = models();
        let case = extract_case(&ms, "gapAnalysis").expect("extraction");

        let wc = worst_case(&case.inputs, &case.target, &case.settings);
        assert!((wc.min - 2.67).abs() < 1e-9, "min = {}", wc.min);
        assert!((wc.max - 3.28).abs() < 1e-9, "max = {}", wc.max);
        assert_eq!(wc.result, PassFail::Pass);

        let r = rss(&case.inputs, &case.target, &case.settings);
        assert!((r.mean - 3.0).abs() < 1e-9);
        assert!((r.cp - 2.7940).abs() < 1e-3);

        let mc = monte_carlo(&case.inputs, &case.target, &case.settings, 42);
        assert_eq!(mc.seed, 42);
        assert!(mc.yield_percent > 99.9);
    }

    #[test]
    fn missing_target_is_an_error() {
        let bad = r#"
            package P {
                analysis def UncertaintyAnalysis;
                analysis def Stackup :> UncertaintyAnalysis;
                part def Asm {
                    analysis gap : Stackup {
                        attribute c1 : Contribution;
                    }
                }
            }
        "#;
        let ms = vec![parse_file("bad.sysml", bad)];
        let err = extract_case(&ms, "gap").unwrap_err();
        assert!(err.contains("target"), "unexpected error: {err}");
    }

    #[test]
    fn unresolvable_chain_is_an_error() {
        let bad = r#"
            package P {
                analysis def UncertaintyAnalysis;
                analysis def Stackup :> UncertaintyAnalysis;
                part def Asm {
                    analysis gap : Stackup {
                        attribute :>> target {
                            :>> nominal = 1.0;
                            :>> lower = 0.5;
                            :>> upper = 1.5;
                        }
                        attribute c1 : Contribution {
                            :>> dim = nowhere.nothing;
                        }
                    }
                }
            }
        "#;
        let ms = vec![parse_file("bad.sysml", bad)];
        let err = extract_case(&ms, "gap").unwrap_err();
        assert!(
            err.contains("nowhere") || err.contains("cannot resolve"),
            "{err}"
        );
    }

    #[test]
    fn usage_level_redefinition_overrides_definition() {
        // Values live on the part def (the drawing) by default; a part
        // usage may redefine a dimension for its context (selective
        // assembly, variant fit). Usage values win over def values.
        let src = r#"
            package P {
                analysis def UncertaintyAnalysis;
                analysis def Stackup :> UncertaintyAnalysis;
                part def Pin {
                    attribute od {
                        :>> nominal = 10.0;
                        :>> plus = 0.1;
                        :>> minus = 0.1;
                    }
                }
                part def Asm {
                    part pin : Pin {
                        attribute :>> od {
                            :>> nominal = 10.2;
                            :>> plus = 0.02;
                            :>> minus = 0.02;
                        }
                    }
                    analysis gap : Stackup {
                        attribute :>> target {
                            :>> nominal = 10.2;
                            :>> lower = 10.0;
                            :>> upper = 10.4;
                        }
                        attribute c1 : Contribution {
                            :>> dim = pin.od;
                        }
                    }
                }
            }
        "#;
        let ms = vec![parse_file("o.sysml", src)];
        let case = extract_case(&ms, "gap").expect("extraction");
        assert!(
            (case.inputs[0].nominal - 10.2).abs() < 1e-9,
            "usage-level value must win: {}",
            case.inputs[0].nominal
        );
        assert!((case.inputs[0].plus - 0.02).abs() < 1e-9);
    }

    #[test]
    fn settings_overrides_from_model() {
        let src = r#"
            package P {
                analysis def UncertaintyAnalysis;
                analysis def Stackup :> UncertaintyAnalysis;
                part def D {
                    attribute x {
                        :>> nominal = 1.0;
                        :>> plus = 0.1;
                        :>> minus = 0.1;
                    }
                }
                part def Asm {
                    part d : D;
                    analysis gap : Stackup {
                        attribute :>> target {
                            :>> nominal = 1.0;
                            :>> lower = 0.5;
                            :>> upper = 1.5;
                        }
                        attribute :>> sigmaLevel = 4.5;
                        attribute :>> meanShiftK = 1.5;
                        attribute :>> iterations = 5000;
                        attribute c1 : Contribution {
                            :>> dim = d.x;
                        }
                    }
                }
            }
        "#;
        let ms = vec![parse_file("s.sysml", src)];
        let case = extract_case(&ms, "gap").expect("extraction");
        assert!((case.settings.sigma_level - 4.5).abs() < 1e-9);
        assert!((case.settings.mean_shift_k - 1.5).abs() < 1e-9);
        assert_eq!(case.settings.iterations, 5000);
        assert_eq!(case.inputs[0].sense, 1.0, "sense defaults to positive");
    }
}
