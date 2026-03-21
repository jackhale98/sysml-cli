/// Checks for unused definitions and unresolved type references.

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, DefKind, Model};

/// Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let n = a.len();
    let m = b.len();
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut prev = (0..=m).collect::<Vec<_>>();
    let mut curr = vec![0; m + 1];

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Find the closest match for `name` among `candidates`, using Levenshtein distance.
/// Returns `None` if no candidate is within the threshold (max 3 edits, and at most
/// half the name length).
pub fn find_closest_match<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let max_dist = name.len().min(6) / 2 + 1; // allow 1 edit for short names, up to 3 for longer
    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        // Quick length filter — if lengths differ by more than max_dist, skip
        let len_diff = if candidate.len() > name.len() {
            candidate.len() - name.len()
        } else {
            name.len() - candidate.len()
        };
        if len_diff > max_dist {
            continue;
        }

        let dist = levenshtein(name, candidate);
        if dist > 0 && dist <= max_dist {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((candidate, dist));
            }
        }
    }
    best.map(|(s, _)| s)
}

/// Standard library types that are always available (not defined in user files).
const BUILTIN_TYPES: &[&str] = &[
    // ScalarValues
    "Boolean", "String", "Integer", "Real", "Natural", "Complex", "Rational",
    "ScalarValues",
    // ISQ common quantities
    "MassValue", "LengthValue", "TimeValue", "PowerValue", "ForceValue",
    "PressureValue", "TemperatureValue", "VelocityValue", "AccelerationValue",
    "AreaValue", "VolumeValue", "DensityValue", "EnergyValue", "TorqueValue",
    "AngularVelocityValue", "FrequencyValue", "ElectricCurrentValue",
    "VoltageValue", "ResistanceValue", "CapacitanceValue",
    "ISQ",
    // SI units
    "SI", "kg", "m", "s", "A", "K", "mol", "cd", "N", "Pa", "J", "W", "Hz",
    // Common base types
    "Anything", "Nothing", "Object", "Occurrence", "Item", "Part",
    "Connection", "Interface", "Port", "Flow", "Action", "State",
    "Constraint", "Requirement", "Calculation", "Analysis",
    "Verification", "UseCase", "View", "Viewpoint", "Rendering",
    "Attribute", "Enum", "Package", "Feature", "Classifier", "Type",
    // Trade studies
    "TradeStudy", "MaximizeObjective", "MinimizeObjective",
    // Verification
    "VerdictKind", "PassFail",
    // Sampling
    "SampledFunction",
];

pub struct UnusedDefCheck;

impl Check for UnusedDefCheck {
    fn name(&self) -> &'static str {
        "unused"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let referenced = model.referenced_names();
        let mut diagnostics = Vec::new();

        for def in &model.definitions {
            // Packages are structural, not referenced by name
            if def.kind == DefKind::Package {
                continue;
            }

            let name = def.name.as_str();
            if !referenced.contains(name) {
                diagnostics.push(Diagnostic::note(
                    &model.file,
                    def.span.clone(),
                    codes::UNUSED_DEF,
                    format!(
                        "{} `{}` is defined but never referenced",
                        def.kind.label(),
                        def.name,
                    ),
                ));
            }
        }

        diagnostics
    }
}

pub struct UnresolvedTypeCheck;

impl Check for UnresolvedTypeCheck {
    fn name(&self) -> &'static str {
        "unresolved"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let defined = model.defined_names();

        // Also collect usage names (parts, ports, etc. are valid targets)
        let mut known: std::collections::HashSet<&str> = defined;
        for u in &model.usages {
            known.insert(u.name.as_str());
        }
        // Add builtins
        for &b in BUILTIN_TYPES {
            known.insert(b);
        }
        // Add definitions from the embedded standard library
        for name in crate::stdlib::stdlib_definitions() {
            known.insert(name.as_str());
        }
        // Add names resolved from imports
        for name in &model.resolved_imports {
            known.insert(name.as_str());
        }

        let mut diagnostics = Vec::new();

        // Check type references
        for tr in &model.type_references {
            let name = simple_name(&tr.name);
            if !known.contains(name) && !is_qualified_stdlib(&tr.name) {
                let mut diag = Diagnostic::warning(
                    &model.file,
                    tr.span.clone(),
                    codes::UNRESOLVED_TYPE,
                    format!("type `{}` is not defined in this file", tr.name),
                );
                if let Some(closest) = find_closest_match(name, known.iter().copied()) {
                    diag = diag.with_suggestion(format!("did you mean `{}`?", closest));
                }
                diagnostics.push(diag);
            }
        }

        // Check connection targets
        for conn in &model.connections {
            check_ref_exists(&conn.source, &conn.span, &known, &model.file, &mut diagnostics);
            check_ref_exists(&conn.target, &conn.span, &known, &model.file, &mut diagnostics);
        }

        // Check allocation targets
        for alloc in &model.allocations {
            check_ref_exists(
                &alloc.source,
                &alloc.span,
                &known,
                &model.file,
                &mut diagnostics,
            );
            check_ref_exists(
                &alloc.target,
                &alloc.span,
                &known,
                &model.file,
                &mut diagnostics,
            );
        }

        diagnostics
    }
}

fn check_ref_exists(
    name: &str,
    span: &crate::model::Span,
    known: &std::collections::HashSet<&str>,
    file: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let simple = simple_name(name);
    if !known.contains(simple) && !is_qualified_stdlib(name) {
        let mut diag = Diagnostic::warning(
            file,
            span.clone(),
            codes::UNRESOLVED_TARGET,
            format!("reference `{}` does not resolve to any definition or usage", name),
        );
        if let Some(closest) = find_closest_match(simple, known.iter().copied()) {
            diag = diag.with_suggestion(format!("did you mean `{}`?", closest));
        }
        diagnostics.push(diag);
    }
}

fn is_qualified_stdlib(name: &str) -> bool {
    name.starts_with("ISQ::")
        || name.starts_with("SI::")
        || name.starts_with("ScalarValues::")
        || name.starts_with("Quantities::")
        || name.starts_with("MeasurementReferences::")
        || name.starts_with("SampledFunctions::")
        || name.starts_with("VerificationCases::")
        || name.starts_with("TradeStudies::")
        || name.starts_with("BaseFunctions::")
        || name.starts_with("ControlFunctions::")
        || name.starts_with("DataFunctions::")
        || name.starts_with("NumericalFunctions::")
        || name.starts_with("SequenceFunctions::")
        || name.starts_with("TrigFunctions::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn levenshtein_one_insert() {
        assert_eq!(levenshtein("abc", "abcd"), 1);
    }

    #[test]
    fn levenshtein_one_delete() {
        assert_eq!(levenshtein("abcd", "abc"), 1);
    }

    #[test]
    fn levenshtein_one_substitute() {
        assert_eq!(levenshtein("abc", "axc"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn closest_match_finds_typo() {
        let candidates = vec!["Vehicle", "Engine", "Wheel", "Transmission"];
        let result = find_closest_match("Vehicl", candidates.iter().copied());
        assert_eq!(result, Some("Vehicle"));
    }

    #[test]
    fn closest_match_case_sensitive() {
        let candidates = vec!["Vehicle", "Engine"];
        // "vehicle" differs by 1 char from "Vehicle" (case)
        let result = find_closest_match("vehicle", candidates.iter().copied());
        assert_eq!(result, Some("Vehicle"));
    }

    #[test]
    fn closest_match_no_match_when_too_different() {
        let candidates = vec!["Vehicle", "Engine"];
        let result = find_closest_match("Completely", candidates.iter().copied());
        assert_eq!(result, None);
    }

    #[test]
    fn closest_match_exact_not_returned() {
        // Exact matches have distance 0 and should not be returned
        let candidates = vec!["Vehicle", "Engine"];
        let result = find_closest_match("Vehicle", candidates.iter().copied());
        assert_eq!(result, None);
    }

    #[test]
    fn closest_match_picks_closest() {
        let candidates = vec!["Engines", "Engine", "Engin"];
        let result = find_closest_match("Engne", candidates.iter().copied());
        assert_eq!(result, Some("Engine"));
    }

    #[test]
    fn unresolved_type_suggests_closest() {
        let source = r#"
            part def Vehicle;
            part def Engine;
            part car : Vehicel;
        "#;
        let model = crate::parser::parse_file("test.sysml", source);
        let check = UnresolvedTypeCheck;
        let diags = check.run(&model);
        let vehicel_diag = diags.iter().find(|d| d.message.contains("Vehicel"));
        assert!(vehicel_diag.is_some(), "should flag 'Vehicel' as unresolved");
        let suggestion = vehicel_diag.unwrap().suggestion.as_deref();
        assert_eq!(suggestion, Some("did you mean `Vehicle`?"));
    }
}
