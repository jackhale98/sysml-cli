use std::process::ExitCode;

pub(crate) fn run(
    kind: &str,
    name: &str,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    doc: Option<&str>,
    members: &[String],
    exposes: &[String],
    filter: Option<&str>,
) -> ExitCode {
    use sysml_core::codegen::template::*;

    let def_kind = match parse_template_kind(kind) {
        Some(k) => k,
        None => {
            eprintln!("error: unknown element kind `{}`", kind);
            eprintln!("  available: part-def, port-def, action-def, state-def, constraint-def,");
            eprintln!("            calc-def, requirement, enum-def, attribute-def, item-def,");
            eprintln!("            view-def, viewpoint-def, package, use-case, connection-def,");
            eprintln!("            flow-def, interface-def, allocation-def");
            return ExitCode::from(1);
        }
    };

    let parsed_members: Vec<MemberSpec> = members
        .iter()
        .filter_map(|s| parse_member_spec(s))
        .collect();

    let opts = TemplateOptions {
        kind: def_kind,
        name: name.to_string(),
        super_type: extends.map(|s| s.to_string()),
        is_abstract,
        short_name: short_name.map(|s| s.to_string()),
        doc: doc.map(|s| s.to_string()),
        members: parsed_members,
        exposes: exposes.to_vec(),
        filter: filter.map(|s| s.to_string()),
        indent: 0,
    };

    let generated = generate_template(&opts);
    print!("{}", generated);

    ExitCode::SUCCESS
}
