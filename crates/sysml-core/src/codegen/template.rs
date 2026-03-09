/// SysML v2 boilerplate template generation.

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
    let has_body = opts.doc.is_some() || !opts.members.is_empty()
        || !opts.exposes.is_empty() || opts.filter.is_some();
    if has_body {
        out.push_str(" {\n");

        // Doc comment
        if let Some(ref doc) = opts.doc {
            out.push_str(&format!("{}doc /* {} */\n", inner_indent, doc));
        }

        // View-specific: expose clauses
        for expose in &opts.exposes {
            out.push_str(&format!("{}expose {};\n", inner_indent, expose));
        }

        // View-specific: filter
        if let Some(ref f) = opts.filter {
            out.push_str(&format!("{}filter @type istype {};\n", inner_indent, f));
        }

        // Members
        for member in &opts.members {
            out.push_str(&inner_indent);

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

        out.push_str(&format!("{}}}\n", indent));
    } else {
        out.push_str(";\n");
    }

    out
}

/// Parse a member spec string like "part engine:Engine" or "in port fuelIn:FuelPort".
pub fn parse_member_spec(s: &str) -> Option<MemberSpec> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

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
    let (name, type_ref) = if let Some((n, t)) = name_and_type.split_once(':') {
        (n.to_string(), Some(t.to_string()))
    } else {
        (name_and_type, None)
    };

    Some(MemberSpec {
        usage_kind,
        name,
        type_ref,
        direction,
        multiplicity: None,
    })
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
            members: vec![
                MemberSpec {
                    usage_kind: "part".to_string(),
                    name: "engine".to_string(),
                    type_ref: Some("Engine".to_string()),
                    direction: None,
                    multiplicity: None,
                },
            ],
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
            members: vec![
                MemberSpec {
                    usage_kind: "item".to_string(),
                    name: "fuel".to_string(),
                    type_ref: Some("FuelType".to_string()),
                    direction: Some("in".to_string()),
                    multiplicity: None,
                },
            ],
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
        assert_eq!(parse_template_kind("requirement"), Some(DefKind::Requirement));
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

        let m2 = parse_member_spec("in port fuelIn:FuelPort").unwrap();
        assert_eq!(m2.direction.as_deref(), Some("in"));
        assert_eq!(m2.usage_kind, "port");
        assert_eq!(m2.name, "fuelIn");
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
