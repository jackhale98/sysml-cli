/// Integration tests for sysml-core.

use sysml_core::checks;
use sysml_core::diagnostic::Severity;
use sysml_core::model::{DefKind, Visibility};
use sysml_core::parser as sysml_parser;

fn lint(source: &str) -> Vec<sysml_core::diagnostic::Diagnostic> {
    let model = sysml_parser::parse_file("test.sysml", source);
    let checks = checks::all_checks();
    let mut diagnostics = Vec::new();
    for check in &checks {
        diagnostics.extend(check.run(&model));
    }
    diagnostics
}

fn lint_with(source: &str, check_name: &str) -> Vec<sysml_core::diagnostic::Diagnostic> {
    let model = sysml_parser::parse_file("test.sysml", source);
    let checks = checks::all_checks();
    let check = checks.iter().find(|c| c.name() == check_name).unwrap();
    check.run(&model)
}

fn parse(source: &str) -> sysml_core::model::Model {
    sysml_parser::parse_file("test.sysml", source)
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

// Output format tests moved to sysml-cli crate

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

// ========================================================================
// Phase 1: Model enrichment — parser extraction tests
// ========================================================================

// --- Visibility ---

#[test]
fn parse_visibility_on_definition() {
    let model = parse(r#"
        private part def Engine;
        public part def Vehicle;
        protected port def DataPort;
    "#);
    let engine = model.find_def("Engine").unwrap();
    assert_eq!(engine.visibility, Some(Visibility::Private));
    let vehicle = model.find_def("Vehicle").unwrap();
    assert_eq!(vehicle.visibility, Some(Visibility::Public));
    let port = model.find_def("DataPort").unwrap();
    assert_eq!(port.visibility, Some(Visibility::Protected));
}

#[test]
fn parse_no_visibility_is_none() {
    let model = parse("part def Plain;");
    let plain = model.find_def("Plain").unwrap();
    assert_eq!(plain.visibility, None);
}

// --- Abstract modifier ---

#[test]
fn parse_abstract_definition() {
    let model = parse(r#"
        abstract part def Vehicle;
        part def Car;
    "#);
    let vehicle = model.find_def("Vehicle").unwrap();
    assert!(vehicle.is_abstract, "Vehicle should be abstract");
    let car = model.find_def("Car").unwrap();
    assert!(!car.is_abstract, "Car should not be abstract");
}

#[test]
fn parse_abstract_with_visibility() {
    let model = parse("public abstract part def Base;");
    let base = model.find_def("Base").unwrap();
    assert!(base.is_abstract);
    assert_eq!(base.visibility, Some(Visibility::Public));
}

// --- Short name ---

#[test]
fn parse_short_name_on_definition() {
    // SysML v2 syntax: short name comes before declared name
    let model = parse("part def <V> Vehicle;");
    let v = model.find_def("Vehicle").unwrap();
    assert_eq!(v.short_name.as_deref(), Some("V"));
}

#[test]
fn parse_no_short_name_is_none() {
    let model = parse("part def Vehicle;");
    let v = model.find_def("Vehicle").unwrap();
    assert_eq!(v.short_name, None);
}

// --- Doc comments ---

#[test]
fn parse_doc_comment_on_definition() {
    let model = parse(r#"
        part def Vehicle {
            doc /* The main vehicle definition */
        }
    "#);
    let v = model.find_def("Vehicle").unwrap();
    assert!(
        v.doc.as_ref().map_or(false, |d| d.contains("main vehicle")),
        "Doc should contain 'main vehicle', got: {:?}",
        v.doc
    );
}

#[test]
fn parse_doc_comments_collected() {
    let model = parse(r#"
        part def Vehicle {
            doc /* The main vehicle definition */
        }
    "#);
    assert!(!model.comments.is_empty(), "Should collect doc comments");
    assert!(model.comments[0].text.contains("main vehicle"));
    assert_eq!(model.comments[0].parent_def.as_deref(), Some("Vehicle"));
}

// --- Parent definition tracking ---

#[test]
fn parse_nested_definition_has_parent() {
    let model = parse(r#"
        part def Vehicle {
            part def Engine;
        }
    "#);
    let engine = model.find_def("Engine").unwrap();
    assert_eq!(engine.parent_def.as_deref(), Some("Vehicle"));
}

#[test]
fn parse_top_level_definition_no_parent() {
    let model = parse("part def Vehicle;");
    let v = model.find_def("Vehicle").unwrap();
    assert_eq!(v.parent_def, None);
}

// --- Multiplicity on usages ---

#[test]
fn parse_multiplicity_range() {
    let model = parse(r#"
        part def Vehicle {
            part wheels : Wheel [4];
        }
    "#);
    let wheels = model.usages.iter().find(|u| u.name == "wheels").unwrap();
    let mult = wheels.multiplicity.as_ref().expect("Should have multiplicity");
    assert_eq!(mult.lower.as_deref(), Some("4"));
}

#[test]
fn parse_multiplicity_range_bounds() {
    let model = parse(r#"
        part def Vehicle {
            part passengers : Person [0..5];
        }
    "#);
    let passengers = model.usages.iter().find(|u| u.name == "passengers").unwrap();
    let mult = passengers.multiplicity.as_ref().expect("Should have multiplicity");
    assert_eq!(mult.lower.as_deref(), Some("0"));
    assert_eq!(mult.upper.as_deref(), Some("5"));
}

#[test]
fn parse_multiplicity_star() {
    let model = parse(r#"
        part def Fleet {
            part vehicles : Vehicle [*];
        }
    "#);
    let vehicles = model.usages.iter().find(|u| u.name == "vehicles").unwrap();
    let mult = vehicles.multiplicity.as_ref().expect("Should have multiplicity");
    assert_eq!(mult.lower, None);
    assert_eq!(mult.upper, None); // * means unbounded
}

#[test]
fn parse_no_multiplicity_is_none() {
    let model = parse(r#"
        part def Vehicle {
            part engine : Engine;
        }
    "#);
    let engine = model.usages.iter().find(|u| u.name == "engine").unwrap();
    assert_eq!(engine.multiplicity, None);
}

// --- Value expressions ---

#[test]
fn parse_value_assignment() {
    let model = parse(r#"
        part def Vehicle {
            attribute mass : Real = 1500.0;
        }
    "#);
    let mass = model.usages.iter().find(|u| u.name == "mass").unwrap();
    assert!(
        mass.value_expr.as_ref().map_or(false, |v| v.contains("1500")),
        "Should extract value expression, got: {:?}",
        mass.value_expr
    );
}

#[test]
fn parse_no_value_is_none() {
    let model = parse(r#"
        part def Vehicle {
            attribute mass : Real;
        }
    "#);
    let mass = model.usages.iter().find(|u| u.name == "mass").unwrap();
    assert_eq!(mass.value_expr, None);
}

// --- Redefines ---

#[test]
fn parse_redefines_keyword() {
    let model = parse(r#"
        part def Car :> Vehicle {
            part engine : V8Engine redefines engine;
        }
    "#);
    let engine = model.usages.iter().find(|u| u.name == "engine").unwrap();
    assert!(
        engine.redefinition.is_some(),
        "Should extract redefines relationship"
    );
}

// --- Subsets ---

#[test]
fn parse_subsets_keyword() {
    let model = parse(r#"
        part def Vehicle {
            part primaryEngine : Engine subsets engines;
        }
    "#);
    let pe = model.usages.iter().find(|u| u.name == "primaryEngine").unwrap();
    assert!(
        pe.subsets.is_some(),
        "Should extract subsets relationship"
    );
}

// Short name on usages: the grammar only supports short_name on definitions,
// not on most usage types. Skipping usage short_name test for now.

#[test]
fn parse_enum_members() {
    let model = parse(r#"
        enum def Color {
            enum red;
            enum green;
            enum blue;
        }
    "#);
    let color = model.find_def("Color").expect("Color should be found");
    assert_eq!(color.kind, DefKind::Enum);
    assert!(color.enum_members.len() >= 2,
        "Expected at least 2 enum members, got {}: {:?}",
        color.enum_members.len(),
        color.enum_members.iter().map(|m| &m.name).collect::<Vec<_>>());
}
