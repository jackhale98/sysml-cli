/// Tree-sitter parsing and model extraction for SysML v2 files.

use tree_sitter::{Language, Node, Parser};

use crate::model::*;

extern "C" {
    fn tree_sitter_sysml() -> Language;
}

pub(crate) fn get_language() -> Language {
    unsafe { tree_sitter_sysml() }
}

/// Parse a SysML v2 source file and extract a model.
pub fn parse_file(file_path: &str, source: &str) -> Model {
    let mut parser = Parser::new();
    parser
        .set_language(&get_language())
        .expect("Failed to set tree-sitter language");

    let tree = parser
        .parse(source, None)
        .expect("Failed to parse source file");

    let mut model = Model::new(file_path.to_string());
    let source_bytes = source.as_bytes();
    walk_node(tree.root_node(), source_bytes, &mut model, None);
    model
}

/// Extract text content of a node from source.
pub(crate) fn node_text<'a>(node: &Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("")
}

/// Get text of a named field child.
fn field_text(node: &Node, field: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .map(|n| node_text(&n, source).to_string())
}

/// Get the supertype from a specialization child node.
fn get_supertype(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "specialization" {
            if let Some(target) = child.child_by_field_name("target") {
                return Some(node_text(&target, source).to_string());
            }
        }
    }
    None
}

/// Get the type reference from a colon type relationship.
fn get_type_ref(node: &Node, source: &[u8]) -> Option<String> {
    // Try field "type" first (from hidden _colon_type_rel)
    if let Some(t) = node.child_by_field_name("type") {
        return Some(node_text(&t, source).to_string());
    }
    // Look for typed_by child node (grammar pattern: typed_by with type field)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "typed_by" {
            if let Some(t) = child.child_by_field_name("type") {
                return Some(node_text(&t, source).to_string());
            }
        }
    }
    // Fall back to looking for qualified_name after ":"
    let mut cursor2 = node.walk();
    let mut saw_colon = false;
    for child in node.children(&mut cursor2) {
        if child.kind() == ":" {
            saw_colon = true;
        } else if saw_colon && (child.kind() == "qualified_name" || child.kind() == "identifier") {
            return Some(node_text(&child, source).to_string());
        } else if saw_colon && child.kind() != ":" {
            saw_colon = false;
        }
    }
    // Also check for specialization as a type relationship
    get_supertype(node, source)
}

/// Map grammar rule name to DefKind.
fn def_kind_from_node(kind: &str) -> Option<DefKind> {
    match kind {
        "part_definition" => Some(DefKind::Part),
        "port_definition" => Some(DefKind::Port),
        "connection_definition" => Some(DefKind::Connection),
        "interface_definition" => Some(DefKind::Interface),
        "flow_definition" => Some(DefKind::Flow),
        "action_definition" => Some(DefKind::Action),
        "state_definition" => Some(DefKind::State),
        "constraint_definition" => Some(DefKind::Constraint),
        "calc_definition" => Some(DefKind::Calc),
        "requirement_definition" => Some(DefKind::Requirement),
        "use_case_definition" => Some(DefKind::UseCase),
        "verification_definition" => Some(DefKind::Verification),
        "analysis_definition" => Some(DefKind::Analysis),
        "concern_definition" => Some(DefKind::Concern),
        "view_definition" => Some(DefKind::View),
        "viewpoint_definition" => Some(DefKind::Viewpoint),
        "rendering_definition" => Some(DefKind::Rendering),
        "enum_definition" => Some(DefKind::Enum),
        "attribute_definition" => Some(DefKind::Attribute),
        "item_definition" => Some(DefKind::Item),
        "allocation_definition" => Some(DefKind::Allocation),
        "occurrence_definition" => Some(DefKind::Occurrence),
        "package_declaration" => Some(DefKind::Package),
        // KerML
        "class_definition" => Some(DefKind::Class),
        "struct_definition" => Some(DefKind::Struct),
        "assoc_definition" => Some(DefKind::Assoc),
        "behavior_definition" => Some(DefKind::Behavior),
        "datatype_definition" => Some(DefKind::Datatype),
        "feature_definition" => Some(DefKind::Feature),
        "function_definition" => Some(DefKind::Function),
        "interaction_definition" => Some(DefKind::Interaction),
        "connector_definition" => Some(DefKind::Connector),
        "predicate_definition" => Some(DefKind::Predicate),
        "namespace_definition" => Some(DefKind::Namespace),
        "type_definition" => Some(DefKind::Type),
        "classifier_definition" => Some(DefKind::Classifier),
        "metaclass_definition" => Some(DefKind::Metaclass),
        "expr_definition" => Some(DefKind::Expr),
        "step_definition" => Some(DefKind::Step),
        "metadata_definition" => Some(DefKind::Metadata),
        "annotation_definition" => Some(DefKind::Annotation),
        _ => None,
    }
}

/// Map grammar rule name to usage kind string.
fn usage_kind_from_node(kind: &str) -> Option<&'static str> {
    match kind {
        "part_usage" => Some("part"),
        "port_usage" => Some("port"),
        "action_usage" => Some("action"),
        "state_usage" => Some("state"),
        "requirement_usage" => Some("requirement"),
        "constraint_usage" => Some("constraint"),
        "calc_usage" => Some("calc"),
        "connection_usage" => Some("connection"),
        "interface_usage" => Some("interface"),
        "allocation_usage" => Some("allocation"),
        "item_usage" => Some("item"),
        "ref_usage" => Some("ref"),
        "attribute_usage" => Some("attribute"),
        "feature_usage" => Some("feature"),
        "exhibit_state_usage" => Some("exhibit state"),
        "occurrence_usage" => Some("occurrence"),
        "event_usage" => Some("event"),
        "rendering_usage" => Some("rendering"),
        "view_usage" => Some("view"),
        "viewpoint_usage" => Some("viewpoint"),
        "concern_usage" => Some("concern"),
        "analysis_usage" => Some("analysis"),
        "verification_usage" => Some("verification"),
        "use_case_usage" => Some("use case"),
        "metadata_usage" => Some("metadata"),
        "metaclass_usage" => Some("metaclass"),
        "expr_usage" => Some("expr"),
        "step_usage" => Some("step"),
        "binding_usage" => Some("binding"),
        "succession_usage" => Some("succession"),
        "succession_flow_usage" => Some("succession flow"),
        _ => None,
    }
}

/// Extract direction modifier from preceding siblings.
fn get_direction(node: &Node) -> Option<Direction> {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        match sib.kind() {
            "in" => return Some(Direction::In),
            "out" => return Some(Direction::Out),
            "inout" => return Some(Direction::InOut),
            // Stop at other structural nodes (don't cross declaration boundaries)
            k if def_kind_from_node(k).is_some()
                || usage_kind_from_node(k).is_some()
                || k == "definition_body"
                || k == "state_body"
                || k == "{" => break,
            _ => {}
        }
        sibling = sib.prev_sibling();
    }
    None
}

/// Check if a usage node's type reference has conjugation (`~` prefix).
fn is_conjugated_type(node: &Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "typed_by" {
            // Check for ~ child node
            let mut tc = child.walk();
            for tc_child in child.children(&mut tc) {
                if tc_child.kind() == "~" {
                    return true;
                }
            }
            // Fallback: check text starts with ~
            let text = node_text(&child, source);
            if text.starts_with('~') {
                return true;
            }
        }
    }
    // Also check for "conjugate" modifier in preceding siblings
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        match sib.kind() {
            "conjugate" => return true,
            k if def_kind_from_node(k).is_some()
                || usage_kind_from_node(k).is_some()
                || k == "definition_body"
                || k == "{" => break,
            _ => {}
        }
        sibling = sib.prev_sibling();
    }
    false
}

/// Recursively walk the parse tree and extract model elements.
fn walk_node(
    node: Node,
    source: &[u8],
    model: &mut Model,
    enclosing_verification: Option<&str>,
) {
    walk_node_scoped(node, source, model, enclosing_verification, None);
}

/// Walk with parent definition scope tracking.
fn walk_node_scoped(
    node: Node,
    source: &[u8],
    model: &mut Model,
    enclosing_verification: Option<&str>,
    parent_def_name: Option<&str>,
) {
    let kind = node.kind();

    match kind {
        // --- Syntax errors ---
        "ERROR" | "MISSING" => {
            let context = node_text(&node, source);
            let context_trimmed = if context.len() > 60 {
                format!("{}...", &context[..60])
            } else {
                context.to_string()
            };
            model.syntax_errors.push(SyntaxError {
                message: if kind == "MISSING" {
                    "Missing expected syntax element".to_string()
                } else {
                    "Syntax error".to_string()
                },
                context: context_trimmed,
                span: Span::from_node(&node),
            });
        }

        // --- Definitions ---
        k if def_kind_from_node(k).is_some() => {
            let def_kind = def_kind_from_node(k).unwrap();
            if let Some(name) = field_text(&node, "name", source) {
                let super_type = get_supertype(&node, source);

                // Extract body structure for constraint/calc defs
                let (has_body, param_count, has_constraint_expr, has_return) =
                    if def_kind == DefKind::Constraint || def_kind == DefKind::Calc {
                        inspect_def_body(&node, def_kind)
                    } else {
                        (true, 0, false, false)
                    };

                model.definitions.push(Definition {
                    kind: def_kind,
                    name: name.clone(),
                    super_type,
                    span: Span::from_node(&node),
                    has_body,
                    param_count,
                    has_constraint_expr,
                    has_return,
                });

                // Recurse into definition body with scope tracking
                let ev = if def_kind == DefKind::Verification {
                    Some(name.as_str())
                } else {
                    enclosing_verification
                };
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    walk_node_scoped(child, source, model, ev, Some(&name));
                }
                return; // Already recursed with scope
            }
        }

        // --- Usages ---
        k if usage_kind_from_node(k).is_some() => {
            let usage_kind = usage_kind_from_node(k).unwrap();
            if let Some(name) = field_text(&node, "name", source) {
                let type_ref = get_type_ref(&node, source);
                if let Some(ref t) = type_ref {
                    model.type_references.push(TypeReference {
                        name: t.clone(),
                        span: Span::from_node(&node),
                    });
                }
                let direction = get_direction(&node);
                let conjugated = is_conjugated_type(&node, source);
                model.usages.push(Usage {
                    kind: usage_kind.to_string(),
                    name,
                    type_ref,
                    span: Span::from_node(&node),
                    direction,
                    is_conjugated: conjugated,
                    parent_def: parent_def_name.map(|s| s.to_string()),
                });
            }

            // Connection usages have connect clauses
            if k == "connection_usage" {
                extract_connect_clause(&node, source, model);
            }
        }

        // --- Satisfy statement ---
        "satisfy_statement" => {
            extract_satisfaction(&node, source, model);
        }

        // --- Allocate statement ---
        "allocate_statement" => {
            extract_allocation(&node, source, model);
        }

        // --- Flow statement ---
        "flow_statement" => {
            extract_flow(&node, source, model);
        }

        // --- Import statement ---
        "import_statement" => {
            let full_text = node_text(&node, source).to_string();
            let is_wildcard = full_text.contains("::*");
            let is_recursive = full_text.contains("::**");
            // Extract the qualified_name child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "qualified_name" || child.kind() == "identifier" {
                    let path = node_text(&child, source).to_string();
                    model.imports.push(Import {
                        path,
                        is_wildcard,
                        is_recursive,
                        span: Span::from_node(&node),
                    });
                    break;
                }
            }
        }

        // --- Require statement (verify inside verification) ---
        "require_statement" => {
            if let Some(ver_name) = enclosing_verification {
                // Inside a verification block, require_statement references a requirement
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "qualified_name" || child.kind() == "identifier" {
                        let req_name = node_text(&child, source).to_string();
                        model.verifications.push(Verification {
                            requirement: req_name,
                            by: ver_name.to_string(),
                            span: Span::from_node(&node),
                        });
                        break;
                    }
                }
            }
        }

        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node_scoped(child, source, model, enclosing_verification, parent_def_name);
    }
}

/// Extract connection endpoints from a connect_clause.
fn extract_connect_clause(node: &Node, source: &[u8], model: &mut Model) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "connect_clause" {
            let mut refs = Vec::new();
            let mut cc_cursor = child.walk();
            for cc_child in child.children(&mut cc_cursor) {
                match cc_child.kind() {
                    "qualified_name" | "feature_chain" | "identifier" => {
                        refs.push(node_text(&cc_child, source).to_string());
                    }
                    _ => {}
                }
            }
            if refs.len() >= 2 {
                model.connections.push(Connection {
                    name: field_text(node, "name", source),
                    source: refs[0].clone(),
                    target: refs[1].clone(),
                    span: Span::from_node(&child),
                });
            }
        }
    }
}

/// Extract satisfy relationship.
fn extract_satisfaction(node: &Node, source: &[u8], model: &mut Model) {
    let mut refs = Vec::new();
    let mut cursor = node.walk();
    let mut after_by = false;
    let mut by_ref = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "qualified_name" | "feature_chain" | "identifier" => {
                if after_by {
                    by_ref = Some(node_text(&child, source).to_string());
                } else {
                    refs.push(node_text(&child, source).to_string());
                }
            }
            "by" => after_by = true,
            _ => {}
        }
    }
    for req in refs {
        model.satisfactions.push(Satisfaction {
            requirement: req,
            by: by_ref.clone(),
            span: Span::from_node(node),
        });
    }
}

/// Extract allocation relationship.
fn extract_allocation(node: &Node, source: &[u8], model: &mut Model) {
    let mut refs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "qualified_name" | "feature_chain" | "identifier"
        ) {
            refs.push(node_text(&child, source).to_string());
        }
    }
    if refs.len() >= 2 {
        model.allocations.push(Allocation {
            source: refs[0].clone(),
            target: refs[1].clone(),
            span: Span::from_node(node),
        });
    }
}

/// Inspect a constraint or calc definition body for structural elements.
/// Returns (has_body, param_count, has_constraint_expr, has_return).
fn inspect_def_body(node: &Node, kind: DefKind) -> (bool, usize, bool, bool) {
    let mut has_body = false;
    let mut param_count = 0;
    let mut has_constraint_expr = false;
    let mut has_return = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "definition_body" {
            has_body = true;
            inspect_body_children(&child, kind, &mut param_count, &mut has_constraint_expr, &mut has_return);
        }
    }

    (has_body, param_count, has_constraint_expr, has_return)
}

/// Walk children of a definition_body to count params, expressions, returns.
fn inspect_body_children(
    body: &Node,
    kind: DefKind,
    param_count: &mut usize,
    has_constraint_expr: &mut bool,
    has_return: &mut bool,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            // `in` parameters appear as feature_usage with "in" modifier
            "feature_usage" => {
                let mut fc = child.walk();
                for fc_child in child.children(&mut fc) {
                    if fc_child.kind() == "in" {
                        *param_count += 1;
                        break;
                    }
                }
            }
            // Constraint expressions
            "expression_statement" => {
                if kind == DefKind::Constraint {
                    *has_constraint_expr = true;
                }
            }
            "result_expression" => {
                if kind == DefKind::Constraint {
                    *has_constraint_expr = true;
                }
            }
            // Constraint usages inside a body also count as expressions
            "constraint_usage" => {
                if kind == DefKind::Constraint || kind == DefKind::Calc {
                    *has_constraint_expr = true;
                }
            }
            // Return statements in calc defs
            "return_statement" => {
                *has_return = true;
            }
            _ => {}
        }
    }
}

/// Extract flow relationship.
fn extract_flow(node: &Node, source: &[u8], model: &mut Model) {
    let mut item_type = None;
    let mut from_ref = None;
    let mut to_ref = None;
    let mut cursor = node.walk();
    let mut after_of = false;
    let mut after_from = false;
    let mut after_to = false;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "of" => after_of = true,
            "from" => after_from = true,
            "to" => after_to = true,
            "qualified_name" | "feature_chain" | "identifier" => {
                let text = node_text(&child, source).to_string();
                if after_to {
                    to_ref = Some(text);
                    after_to = false;
                } else if after_from {
                    from_ref = Some(text);
                    after_from = false;
                } else if after_of {
                    item_type = Some(text);
                    after_of = false;
                }
            }
            _ => {}
        }
    }
    if let (Some(src), Some(tgt)) = (from_ref, to_ref) {
        model.flows.push(Flow {
            name: field_text(node, "name", source),
            item_type,
            source: src,
            target: tgt,
            span: Span::from_node(node),
        });
    }
}
