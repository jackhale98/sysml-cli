/// Check for multiplicity constraint violations.
///
/// Validates that multiplicity bounds are well-formed:
/// - Lower bound <= upper bound (when both specified)
/// - Bounds are non-negative
/// - Upper bound is not zero (empty collections are suspicious)

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::Model;

pub struct MultiplicityCheck;

impl Check for MultiplicityCheck {
    fn name(&self) -> &'static str {
        "multiplicity"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for usage in &model.usages {
            if let Some(ref mult) = usage.multiplicity {
                let lower: Option<i64> = mult.lower.as_ref().and_then(|s| s.parse().ok());
                let upper: Option<i64> = mult.upper.as_ref().and_then(|s| s.parse().ok());

                // Check lower > upper
                if let (Some(lo), Some(hi)) = (lower, upper) {
                    if lo > hi {
                        diagnostics.push(Diagnostic::warning(
                            &model.file,
                            usage.span.clone(),
                            codes::MULTIPLICITY_VIOLATION,
                            format!(
                                "multiplicity lower bound ({}) exceeds upper bound ({}) on `{}`",
                                lo, hi, usage.name
                            ),
                        ));
                    }
                }

                // Check for zero upper bound (likely error)
                if let Some(0) = upper {
                    diagnostics.push(Diagnostic::warning(
                        &model.file,
                        usage.span.clone(),
                        codes::MULTIPLICITY_VIOLATION,
                        format!(
                            "multiplicity upper bound is 0 on `{}` — element can never exist",
                            usage.name
                        ),
                    ));
                }

                // Check negative bounds
                if let Some(lo) = lower {
                    if lo < 0 {
                        diagnostics.push(Diagnostic::warning(
                            &model.file,
                            usage.span.clone(),
                            codes::MULTIPLICITY_VIOLATION,
                            format!(
                                "negative multiplicity lower bound ({}) on `{}`",
                                lo, usage.name
                            ),
                        ));
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn valid_multiplicity_no_warnings() {
        let source = r#"
            part def Vehicle {
                part wheels : Wheel [4];
                part passengers : Human [0..*];
                part engine : Engine [1];
            }
        "#;
        let model = parse_file("test.sysml", source);
        let check = MultiplicityCheck;
        let diags = check.run(&model);
        assert!(diags.is_empty(), "valid multiplicities should produce no warnings: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>());
    }

    #[test]
    fn lower_exceeds_upper() {
        let source = "part def A { part b : B [5..2]; }\n";
        let model = parse_file("test.sysml", source);
        let check = MultiplicityCheck;
        let diags = check.run(&model);
        assert!(
            diags.iter().any(|d| d.code == codes::MULTIPLICITY_VIOLATION && d.message.contains("exceeds")),
            "should warn about lower > upper"
        );
    }

    #[test]
    fn zero_upper_bound() {
        let source = "part def A { part b : B [0..0]; }\n";
        let model = parse_file("test.sysml", source);
        let check = MultiplicityCheck;
        let diags = check.run(&model);
        assert!(
            diags.iter().any(|d| d.message.contains("upper bound is 0")),
            "should warn about zero upper bound"
        );
    }
}
