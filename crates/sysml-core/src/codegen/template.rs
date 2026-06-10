//! SysML v2 boilerplate template generation.

use crate::model::DefKind;

/// Options for generating a SysML v2 definition template.
#[derive(Debug, Clone)]
pub struct TemplateOptions {
    pub kind: DefKind,
    pub name: String,
    pub super_type: Option<String>,
    pub is_abstract: bool,
    pub short_name: Option<String>,
    pub doc: Option<String>,
    pub members: Vec<MemberSpec>,
    /// View-specific: expose clauses (e.g., "Vehicle::*").
    pub exposes: Vec<String>,
    /// View-specific: filter kind (e.g., "part", "port").
    pub filter: Option<String>,
    /// Base indent level (number of spaces).
    pub indent: usize,
}

/// A member (usage) to include in a generated template.
#[derive(Debug, Clone)]
pub struct MemberSpec {
    pub usage_kind: String,
    pub name: String,
    pub type_ref: Option<String>,
    pub direction: Option<String>,
    pub multiplicity: Option<String>,
    /// If true, render as verbatim text (for transitions, successions, expressions).
    pub raw_line: bool,
}

/// Parse a template kind string into a `DefKind`.
pub fn parse_template_kind(s: &str) -> Option<DefKind> {
    match s.to_lowercase().replace('-', " ").as_str() {
        "part def" | "part" => Some(DefKind::Part),
        "port def" | "port" => Some(DefKind::Port),
        "connection def" | "connection" => Some(DefKind::Connection),
        "interface def" | "interface" => Some(DefKind::Interface),
        "flow def" | "flow" => Some(DefKind::Flow),
        "action def" | "action" => Some(DefKind::Action),
        "state def" | "state" => Some(DefKind::State),
        "constraint def" | "constraint" => Some(DefKind::Constraint),
        "calc def" | "calc" => Some(DefKind::Calc),
        "requirement def" | "requirement" | "req" => Some(DefKind::Requirement),
        "verification def" | "verification" | "verification-def" | "vcase" => {
            Some(DefKind::Verification)
        }
        "use case def" | "use case" | "usecase" => Some(DefKind::UseCase),
        "enum def" | "enum" => Some(DefKind::Enum),
        "attribute def" | "attribute" | "attr" => Some(DefKind::Attribute),
        "item def" | "item" => Some(DefKind::Item),
        "view def" | "view" => Some(DefKind::View),
        "viewpoint def" | "viewpoint" => Some(DefKind::Viewpoint),
        "package" | "pkg" => Some(DefKind::Package),
        "allocation def" | "allocation" => Some(DefKind::Allocation),
        _ => None,
    }
}

/// Generate SysML v2 text for a definition template.
pub fn generate_template(opts: &TemplateOptions) -> String {
    let indent = " ".repeat(opts.indent);
    let inner_indent = " ".repeat(opts.indent + 4);
    let mut out = String::new();

    // Abstract modifier
    if opts.is_abstract {
        out.push_str(&format!("{}abstract ", indent));
    } else {
        out.push_str(&indent);
    }

    // Definition keyword
    out.push_str(opts.kind.label());

    // Short name (comes before name in SysML v2)
    if let Some(ref sn) = opts.short_name {
        out.push_str(&format!(" <{}>", sn));
    }

    // Name
    out.push_str(&format!(" {}", opts.name));

    // Supertype
    if let Some(ref st) = opts.super_type {
        out.push_str(&format!(" :> {}", st));
    }

    // Body
    let has_body = opts.doc.is_some()
        || !opts.members.is_empty()
        || !opts.exposes.is_empty()
        || opts.filter.is_some();
    if has_body {
        out.push_str(" {\n");

        // Doc comment (for verification defs, render as objective block)
        if let Some(ref doc) = opts.doc {
            if opts.kind == DefKind::Verification {
                out.push_str(&format!("{}objective {{\n", inner_indent));
                out.push_str(&format!("{}    doc /* {} */\n", inner_indent, doc));
                out.push_str(&format!("{}}}\n", inner_indent));
            } else {
                out.push_str(&format!("{}doc /* {} */\n", inner_indent, doc));
            }
        }

        // View-specific: expose clauses
        for expose in &opts.exposes {
            out.push_str(&format!("{}expose {};\n", inner_indent, expose));
        }

        // View-specific: filter
        if let Some(ref f) = opts.filter {
            out.push_str(&format!("{}filter @type istype {};\n", inner_indent, f));
        }

        // Members — raw lines, enum members, or structured usages
        let is_enum = opts.kind == DefKind::Enum;
        for member in &opts.members {
            out.push_str(&inner_indent);

            if member.raw_line {
                // Raw line: verbatim text (transitions, successions, expressions)
                let text = member.name.trim_end();
                out.push_str(text);
                if !text.ends_with(';') {
                    out.push(';');
                }
                out.push('\n');
            } else if is_enum {
                // Enum members: just `enum <name>;`
                out.push_str(&format!("enum {};\n", member.name));
            } else {
                // Direction
                if let Some(ref dir) = member.direction {
                    out.push_str(&format!("{} ", dir));
                }

                // Usage kind and name
                out.push_str(&format!("{} {}", member.usage_kind, member.name));

                // Type reference
                if let Some(ref t) = member.type_ref {
                    out.push_str(&format!(" : {}", t));
                }

                // Multiplicity
                if let Some(ref m) = member.multiplicity {
                    out.push_str(&format!(" [{}]", m));
                }

                out.push_str(";\n");
            }
        }

        out.push_str(&format!("{}}}\n", indent));
    } else {
        out.push_str(";\n");
    }

    out
}

/// Parse generated SysML text through tree-sitter and return any syntax errors.
pub fn validate_generated(text: &str) -> Result<(), Vec<String>> {
    let model = crate::parser::parse_file("<generated>", text);
    if model.syntax_errors.is_empty() {
        Ok(())
    } else {
        Err(model
            .syntax_errors
            .iter()
            .map(|e| format!("line {}: {} ({})", e.span.start_row, e.message, e.context))
            .collect())
    }
}

/// Parse a member spec string like "part engine:Engine" or "in port fuelIn:FuelPort[2]".
///
/// Supported formats:
///   - `part engine:Engine`
///   - `in port fuelIn:FuelPort`
///   - `part wheels:Wheel[4]`
///   - `attribute name:String[0..1]`
pub fn parse_member_spec(s: &str) -> Option<MemberSpec> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Detect raw-line patterns: transitions, successions, entry/exit, accept/send
    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    let first_word = first_token.trim_end_matches(';');
    if matches!(
        first_word,
        "transition" | "entry" | "exit" | "first" | "accept" | "send"
    ) {
        return Some(MemberSpec {
            usage_kind: String::new(),
            name: trimmed.to_string(),
            type_ref: None,
            direction: None,
            multiplicity: None,
            raw_line: true,
        });
    }

    // Detect constraint expressions: "constraint <expr with operators>"
    if first_word == "constraint" {
        let rest = trimmed.strip_prefix("constraint").unwrap().trim();
        // If no colon (not a typed usage) and has operator chars, treat as expression
        if !rest.contains(':')
            && (rest.contains(">=")
                || rest.contains("<=")
                || rest.contains("==")
                || rest.contains(" and ")
                || rest.contains(" or "))
        {
            return Some(MemberSpec {
                usage_kind: String::new(),
                name: rest.to_string(),
                type_ref: None,
                direction: None,
                multiplicity: None,
                raw_line: true,
            });
        }
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();

    let mut idx = 0;
    let direction = match parts.get(idx) {
        Some(&"in") | Some(&"out") | Some(&"inout") => {
            let d = parts[idx].to_string();
            idx += 1;
            Some(d)
        }
        _ => None,
    };

    let usage_kind = parts.get(idx)?.to_string();
    idx += 1;

    let name_and_type = parts.get(idx)?.to_string();
    let (name, type_ref, multiplicity) = parse_name_type_mult(&name_and_type);

    Some(MemberSpec {
        usage_kind,
        name,
        type_ref,
        direction,
        multiplicity,
        raw_line: false,
    })
}

/// Parse `name[:Type[mult]]` into (name, Option<type>, Option<multiplicity>).
fn parse_name_type_mult(s: &str) -> (String, Option<String>, Option<String>) {
    if let Some((name, rest)) = s.split_once(':') {
        // rest could be "Type[mult]" or "Type"
        if let Some((type_ref, mult)) = rest.split_once('[') {
            let mult = mult.trim_end_matches(']');
            (
                name.to_string(),
                Some(type_ref.to_string()),
                Some(mult.to_string()),
            )
        } else {
            (name.to_string(), Some(rest.to_string()), None)
        }
    } else if let Some((name, mult)) = s.split_once('[') {
        // name[mult] without type
        let mult = mult.trim_end_matches(']');
        (name.to_string(), None, Some(mult.to_string()))
    } else {
        (s.to_string(), None, None)
    }
}

/// Generate a connection usage with connect binding.
///
/// Returns text like:
/// ```text
/// connection tempConn : SensorConnection
///     connect tempSensor.dataOut to controller.tempIn;
/// ```
pub fn generate_connection_usage(
    name: &str,
    type_ref: Option<&str>,
    connect_endpoints: &str,
    indent: usize,
) -> String {
    let ind = " ".repeat(indent);
    let inner = " ".repeat(indent + 4);
    let type_part = type_ref.map(|t| format!(" : {}", t)).unwrap_or_default();
    format!(
        "{}connection {}{}\n{}connect {};\n",
        ind, name, type_part, inner, connect_endpoints,
    )
}

/// Generate a satisfy or verify relationship statement.
///
/// Returns text like:
/// ```text
/// satisfy requirement TemperatureAccuracy by WeatherStationUnit;
/// ```
pub fn generate_relationship(rel_kind: &str, requirement: &str, by: &str, indent: usize) -> String {
    let ind = " ".repeat(indent);
    format!(
        "{}{} requirement {} by {};\n",
        ind, rel_kind, requirement, by
    )
}

/// Generate an import statement.
///
/// Returns text like:
/// ```text
/// import WeatherStation::*;
/// ```
pub fn generate_import(import_path: &str, indent: usize) -> String {
    let ind = " ".repeat(indent);
    format!("{}import {};\n", ind, import_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_simple_part_def() {
        let opts = TemplateOptions {
            kind: DefKind::Part,
            name: "Vehicle".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: Vec::new(),
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert_eq!(result.trim(), "part def Vehicle;");
    }

    #[test]
    fn generate_with_supertype_and_doc() {
        let opts = TemplateOptions {
            kind: DefKind::Part,
            name: "Vehicle".to_string(),
            super_type: Some("Base".to_string()),
            is_abstract: false,
            short_name: None,
            doc: Some("A vehicle definition".to_string()),
            members: Vec::new(),
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("part def Vehicle :> Base {"));
        assert!(result.contains("doc /* A vehicle definition */"));
    }

    #[test]
    fn generate_abstract_with_members() {
        let opts = TemplateOptions {
            kind: DefKind::Part,
            name: "Vehicle".to_string(),
            super_type: None,
            is_abstract: true,
            short_name: Some("V".to_string()),
            doc: None,
            members: vec![MemberSpec {
                usage_kind: "part".to_string(),
                name: "engine".to_string(),
                type_ref: Some("Engine".to_string()),
                direction: None,
                multiplicity: None,
                raw_line: false,
            }],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("abstract part def <V> Vehicle {"));
        assert!(result.contains("part engine : Engine;"));
    }

    #[test]
    fn generate_port_def_with_direction() {
        let opts = TemplateOptions {
            kind: DefKind::Port,
            name: "FuelPort".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![MemberSpec {
                usage_kind: "item".to_string(),
                name: "fuel".to_string(),
                type_ref: Some("FuelType".to_string()),
                direction: Some("in".to_string()),
                multiplicity: None,
                raw_line: false,
            }],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("port def FuelPort {"));
        assert!(result.contains("in item fuel : FuelType;"));
    }

    #[test]
    fn generate_indented() {
        let opts = TemplateOptions {
            kind: DefKind::Part,
            name: "Engine".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: Vec::new(),
            exposes: Vec::new(),
            filter: None,
            indent: 4,
        };
        let result = generate_template(&opts);
        assert!(result.starts_with("    part def Engine;"));
    }

    #[test]
    fn parse_template_kind_variants() {
        assert_eq!(parse_template_kind("part-def"), Some(DefKind::Part));
        assert_eq!(parse_template_kind("port def"), Some(DefKind::Port));
        assert_eq!(parse_template_kind("action-def"), Some(DefKind::Action));
        assert_eq!(parse_template_kind("state"), Some(DefKind::State));
        assert_eq!(
            parse_template_kind("requirement"),
            Some(DefKind::Requirement)
        );
        assert_eq!(parse_template_kind("package"), Some(DefKind::Package));
        assert_eq!(parse_template_kind("nonsense"), None);
    }

    #[test]
    fn parse_member_spec_variants() {
        let m = parse_member_spec("part engine:Engine").unwrap();
        assert_eq!(m.usage_kind, "part");
        assert_eq!(m.name, "engine");
        assert_eq!(m.type_ref.as_deref(), Some("Engine"));
        assert!(m.direction.is_none());
        assert!(m.multiplicity.is_none());

        let m2 = parse_member_spec("in port fuelIn:FuelPort").unwrap();
        assert_eq!(m2.direction.as_deref(), Some("in"));
        assert_eq!(m2.usage_kind, "port");
        assert_eq!(m2.name, "fuelIn");
    }

    #[test]
    fn parse_member_spec_with_multiplicity() {
        let m = parse_member_spec("part wheels:Wheel[4]").unwrap();
        assert_eq!(m.name, "wheels");
        assert_eq!(m.type_ref.as_deref(), Some("Wheel"));
        assert_eq!(m.multiplicity.as_deref(), Some("4"));

        let m2 = parse_member_spec("attribute sensors:Sensor[1..*]").unwrap();
        assert_eq!(m2.name, "sensors");
        assert_eq!(m2.type_ref.as_deref(), Some("Sensor"));
        assert_eq!(m2.multiplicity.as_deref(), Some("1..*"));

        let m3 = parse_member_spec("part items[0..1]").unwrap();
        assert_eq!(m3.name, "items");
        assert!(m3.type_ref.is_none());
        assert_eq!(m3.multiplicity.as_deref(), Some("0..1"));
    }

    #[test]
    fn generate_member_with_multiplicity() {
        let opts = TemplateOptions {
            kind: DefKind::Part,
            name: "Vehicle".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![MemberSpec {
                usage_kind: "part".to_string(),
                name: "wheels".to_string(),
                type_ref: Some("Wheel".to_string()),
                direction: None,
                multiplicity: Some("4".to_string()),
                raw_line: false,
            }],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("part wheels : Wheel [4];"));
    }

    #[test]
    fn generate_enum_with_members() {
        let opts = TemplateOptions {
            kind: DefKind::Enum,
            name: "Color".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![
                MemberSpec {
                    usage_kind: "enum".to_string(),
                    name: "red".to_string(),
                    type_ref: None,
                    direction: None,
                    multiplicity: None,
                    raw_line: false,
                },
                MemberSpec {
                    usage_kind: "enum".to_string(),
                    name: "green".to_string(),
                    type_ref: None,
                    direction: None,
                    multiplicity: None,
                    raw_line: false,
                },
                MemberSpec {
                    usage_kind: "enum".to_string(),
                    name: "blue".to_string(),
                    type_ref: None,
                    direction: None,
                    multiplicity: None,
                    raw_line: false,
                },
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("enum def Color {"));
        assert!(result.contains("enum red;"));
        assert!(result.contains("enum green;"));
        assert!(result.contains("enum blue;"));
        // Should NOT contain usage-style formatting
        assert!(!result.contains("enum enum"));
    }

    #[test]
    fn generate_connection_usage_with_type() {
        let result = generate_connection_usage(
            "tempConn",
            Some("SensorConnection"),
            "tempSensor.dataOut to controller.tempIn",
            0,
        );
        assert!(result.contains("connection tempConn : SensorConnection"));
        assert!(result.contains("connect tempSensor.dataOut to controller.tempIn;"));
    }

    #[test]
    fn generate_connection_usage_without_type() {
        let result = generate_connection_usage(
            "displayConn",
            None,
            "controller.displayOut to display.dataIn",
            0,
        );
        assert!(result.contains("connection displayConn\n"));
        assert!(result.contains("connect controller.displayOut to display.dataIn;"));
        assert!(!result.contains(":"));
    }

    #[test]
    fn generate_connection_usage_indented() {
        let result = generate_connection_usage("conn1", Some("C"), "a.x to b.y", 8);
        assert!(result.starts_with("        connection conn1 : C\n"));
        assert!(result.contains("            connect a.x to b.y;"));
    }

    #[test]
    fn generate_satisfy_relationship() {
        let result = generate_relationship("satisfy", "TempAccuracy", "WeatherStation", 0);
        assert_eq!(
            result,
            "satisfy requirement TempAccuracy by WeatherStation;\n"
        );
    }

    #[test]
    fn generate_verify_relationship() {
        let result = generate_relationship("verify", "TempAccuracy", "TestTempAccuracy", 4);
        assert_eq!(
            result,
            "    verify requirement TempAccuracy by TestTempAccuracy;\n"
        );
    }

    #[test]
    fn generate_import_statement() {
        let result = generate_import("WeatherStation::*", 0);
        assert_eq!(result, "import WeatherStation::*;\n");
    }

    #[test]
    fn generate_import_statement_indented() {
        let result = generate_import("Vehicles::Engine", 4);
        assert_eq!(result, "    import Vehicles::Engine;\n");
    }

    #[test]
    fn validate_generated_valid() {
        let valid = "part def Vehicle;\n";
        assert!(validate_generated(valid).is_ok());
    }

    #[test]
    fn validate_generated_with_body() {
        let valid = "part def Vehicle {\n    part engine : Engine;\n}\n";
        assert!(validate_generated(valid).is_ok());
    }

    #[test]
    fn parse_member_spec_transition() {
        let m = parse_member_spec("transition first idle accept start then running").unwrap();
        assert!(m.raw_line);
        assert_eq!(m.name, "transition first idle accept start then running");
    }

    #[test]
    fn parse_member_spec_entry() {
        let m = parse_member_spec("entry; then idle;").unwrap();
        assert!(m.raw_line);
        assert_eq!(m.name, "entry; then idle;");
    }

    #[test]
    fn parse_member_spec_first_succession() {
        let m = parse_member_spec("first readTemp then processData").unwrap();
        assert!(m.raw_line);
        assert_eq!(m.name, "first readTemp then processData");
    }

    #[test]
    fn parse_member_spec_constraint_expr() {
        let m = parse_member_spec("constraint temp >= -40 and temp <= 60").unwrap();
        assert!(m.raw_line);
        assert_eq!(m.name, "temp >= -40 and temp <= 60");
    }

    #[test]
    fn parse_member_spec_constraint_usage_not_raw() {
        let m = parse_member_spec("constraint tempCheck:TempConstraint").unwrap();
        assert!(!m.raw_line);
        assert_eq!(m.usage_kind, "constraint");
        assert_eq!(m.name, "tempCheck");
        assert_eq!(m.type_ref.as_deref(), Some("TempConstraint"));
    }

    #[test]
    fn parse_member_spec_return() {
        let m = parse_member_spec("return hours:Real").unwrap();
        assert!(!m.raw_line);
        assert_eq!(m.usage_kind, "return");
        assert_eq!(m.name, "hours");
        assert_eq!(m.type_ref.as_deref(), Some("Real"));
    }

    #[test]
    fn generate_state_def_with_transitions() {
        let opts = TemplateOptions {
            kind: DefKind::State,
            name: "StationStates".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: Some("Operating states".to_string()),
            members: vec![
                parse_member_spec("entry; then off;").unwrap(),
                parse_member_spec("state off").unwrap(),
                parse_member_spec("state monitoring").unwrap(),
                parse_member_spec("transition first off accept powerOn then monitoring").unwrap(),
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("state def StationStates {"));
        assert!(result.contains("entry; then off;"));
        assert!(result.contains("state off;"));
        assert!(result.contains("state monitoring;"));
        assert!(result.contains("transition first off accept powerOn then monitoring;"));
    }

    #[test]
    fn generate_action_def_with_steps() {
        let opts = TemplateOptions {
            kind: DefKind::Action,
            name: "ReadSensors".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![
                parse_member_spec("action readTemp").unwrap(),
                parse_member_spec("action processData").unwrap(),
                parse_member_spec("first readTemp then processData").unwrap(),
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("action def ReadSensors {"));
        assert!(result.contains("action readTemp;"));
        assert!(result.contains("first readTemp then processData;"));
    }

    #[test]
    fn generate_calc_def_with_return() {
        let opts = TemplateOptions {
            kind: DefKind::Calc,
            name: "BatteryRuntime".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![
                parse_member_spec("in attribute capacity:Real").unwrap(),
                parse_member_spec("in attribute consumption:Real").unwrap(),
                parse_member_spec("return hours:Real").unwrap(),
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("calc def BatteryRuntime {"));
        assert!(result.contains("in attribute capacity : Real;"));
        assert!(result.contains("return hours : Real;"));
    }

    #[test]
    fn generate_constraint_def_with_expression() {
        let opts = TemplateOptions {
            kind: DefKind::Constraint,
            name: "TempLimit".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: vec![
                parse_member_spec("in attribute temp:Real").unwrap(),
                parse_member_spec("constraint temp >= -40 and temp <= 60").unwrap(),
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("constraint def TempLimit {"));
        assert!(result.contains("in attribute temp : Real;"));
        assert!(result.contains("temp >= -40 and temp <= 60;"));
    }

    #[test]
    fn generate_verification_def() {
        let opts = TemplateOptions {
            kind: DefKind::Verification,
            name: "TestTempAccuracy".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: Some("Verify temperature accuracy".to_string()),
            members: vec![
                parse_member_spec("subject testSubject").unwrap(),
                parse_member_spec("requirement tempReq:TemperatureAccuracy").unwrap(),
            ],
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("verification def TestTempAccuracy {"));
        assert!(result.contains("objective {"));
        assert!(result.contains("doc /* Verify temperature accuracy */"));
        assert!(result.contains("subject testSubject;"));
        assert!(result.contains("requirement tempReq : TemperatureAccuracy;"));
        // Should NOT have a top-level doc comment
        let lines: Vec<&str> = result.lines().collect();
        assert!(!lines[1].trim().starts_with("doc"));
    }

    #[test]
    fn parse_template_kind_verification() {
        assert_eq!(
            parse_template_kind("verification"),
            Some(DefKind::Verification)
        );
        assert_eq!(
            parse_template_kind("verification-def"),
            Some(DefKind::Verification)
        );
    }

    #[test]
    fn generate_view_def_with_expose_and_filter() {
        let opts = TemplateOptions {
            kind: DefKind::View,
            name: "PartsView".to_string(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc: None,
            members: Vec::new(),
            exposes: vec!["Vehicle::*".to_string()],
            filter: Some("PartDef".to_string()),
            indent: 0,
        };
        let result = generate_template(&opts);
        assert!(result.contains("view def PartsView {"));
        assert!(result.contains("expose Vehicle::*;"));
        assert!(result.contains("filter @type istype PartDef;"));
    }
}
