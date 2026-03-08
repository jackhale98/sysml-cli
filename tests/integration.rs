/// Integration tests for sysml-lint.

use sysml2_cli::checks;
use sysml2_cli::diagnostic::Severity;
use sysml2_cli::parser as sysml_parser;

fn lint(source: &str) -> Vec<sysml2_cli::diagnostic::Diagnostic> {
    let model = sysml_parser::parse_file("test.sysml", source);
    let checks = checks::all_checks();
    let mut diagnostics = Vec::new();
    for check in &checks {
        diagnostics.extend(check.run(&model));
    }
    diagnostics
}

fn lint_with(source: &str, check_name: &str) -> Vec<sysml2_cli::diagnostic::Diagnostic> {
    let model = sysml_parser::parse_file("test.sysml", source);
    let checks = checks::all_checks();
    let check = checks.iter().find(|c| c.name() == check_name).unwrap();
    check.run(&model)
}

#[test]
fn clean_model_no_errors() {
    let source = r#"
        package CleanModel {
            part def Vehicle;
            part vehicle : Vehicle;
        }
    "#;
    let diags = lint(source);
    let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
    assert_eq!(errors, 0, "Clean model should have no errors");
}

#[test]
fn syntax_error_detected() {
    let source = r#"
        part def Vehicle {{{
    "#;
    let diags = lint_with(source, "syntax");
    assert!(!diags.is_empty(), "Garbled syntax should produce syntax error");
    assert!(diags.iter().all(|d| d.severity == Severity::Error));
}

#[test]
fn duplicate_definitions() {
    let source = r#"
        part def Widget;
        part def Widget;
    "#;
    let diags = lint_with(source, "duplicates");
    assert_eq!(diags.len(), 1, "Should detect one duplicate");
    assert!(diags[0].message.contains("duplicate"));
}

#[test]
fn unused_definition() {
    let source = r#"
        part def Foo;
        part def Bar;
    "#;
    let diags = lint_with(source, "unused");
    assert_eq!(diags.len(), 2, "Both definitions are unused");
}

#[test]
fn used_definition_not_flagged() {
    let source = r#"
        part def Engine;
        part def Vehicle {
            part engine : Engine;
        }
    "#;
    let diags = lint_with(source, "unused");
    let engine_unused = diags.iter().any(|d| d.message.contains("Engine"));
    assert!(
        !engine_unused,
        "Engine is used via typing, should not be flagged: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn unsatisfied_requirement() {
    let source = r#"
        requirement def MassReq {
            doc /* mass under 2000 kg */
        }
    "#;
    let diags = lint_with(source, "unsatisfied");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("MassReq"));
}

#[test]
fn satisfied_requirement_ok() {
    let source = r#"
        requirement def MassReq {
            doc /* mass under 2000 kg */
        }
        part def Vehicle {
            satisfy MassReq;
        }
    "#;
    let diags = lint_with(source, "unsatisfied");
    assert!(diags.is_empty(), "Satisfied requirement should not be flagged");
}

#[test]
fn unverified_requirement() {
    let source = r#"
        requirement def SpeedReq {
            doc /* top speed > 100 km/h */
        }
    "#;
    let diags = lint_with(source, "unverified");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("SpeedReq"));
}

#[test]
fn diagnostic_sorting() {
    let diags = lint(r#"
        part def A;
        part def B;
        part def C;
    "#);
    // Should be sorted by line
    for pair in diags.windows(2) {
        assert!(
            pair[0].span.start_row <= pair[1].span.start_row,
            "Diagnostics should be sorted by line"
        );
    }
}

#[test]
fn json_output_valid() {
    let diags = lint("part def Unused;");
    let json = sysml2_cli::output::format_json(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should be valid JSON");
    assert!(parsed.is_array());
}

#[test]
fn text_output_format() {
    let diags = lint("part def Unused;");
    let text = sysml2_cli::output::format_text(&diags);
    // Should contain file:line:col format
    assert!(text.contains("test.sysml:"));
}

// --- Constraint checks ---

#[test]
fn empty_constraint_flagged() {
    let source = r#"
        constraint def BadConstraint {
            in massActual : Real;
        }
    "#;
    let diags = lint_with(source, "constraints");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("BadConstraint"));
    assert!(diags[0].message.contains("no constraint expression"));
}

#[test]
fn constraint_with_expression_ok() {
    let source = r#"
        constraint def MassConstraint {
            in massActual : Real;
            in massLimit : Real;
            massActual <= massLimit;
        }
    "#;
    let diags = lint_with(source, "constraints");
    assert!(diags.is_empty(), "Constraint with expression should not be flagged");
}

#[test]
fn constraint_semicolon_only_ok() {
    // Forward-declared constraint with no body should not be flagged
    let source = "constraint def Forward;";
    let diags = lint_with(source, "constraints");
    assert!(diags.is_empty(), "Semicolon-only constraint should not be flagged");
}

// --- Calculation checks ---

#[test]
fn calc_no_return_flagged() {
    let source = r#"
        calc def BadCalc {
            in x : Real;
        }
    "#;
    let diags = lint_with(source, "calculations");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("BadCalc"));
    assert!(diags[0].message.contains("no return statement"));
}

#[test]
fn calc_with_return_ok() {
    let source = r#"
        calc def GoodCalc {
            in x : Real;
            return result : Real;
        }
    "#;
    let diags = lint_with(source, "calculations");
    assert!(diags.is_empty(), "Calc with return should not be flagged");
}

#[test]
fn calc_semicolon_only_ok() {
    let source = "calc def Forward;";
    let diags = lint_with(source, "calculations");
    assert!(diags.is_empty(), "Semicolon-only calc should not be flagged");
}
