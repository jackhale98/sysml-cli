/// Tree-sitter parsing and model extraction for SysML v2 files.

use tree_sitter::{Language, Node, Parser};

use crate::model::*;

extern "C" {
    fn tree_sitter_sysml() -> Language;
}

pub fn get_language() -> Language {
    unsafe { tree_sitter_sysml() }
}

/// Dump the CST for debugging.
pub fn dump_cst(source: &str) -> String {
    let mut parser = Parser::new();
    parser
        .set_language(&get_language())
        .expect("Failed to set tree-sitter language");
    let tree = parser
        .parse(source, None)
        .expect("Failed to parse source file");
    fn fmt_node(node: Node, source: &[u8], indent: usize, out: &mut String) {
        let prefix = "  ".repeat(indent);
        let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("");
        let short = if text.len() > 60 { &text[..60] } else { text };
        let short = short.replace('\n', "\\n");
        out.push_str(&format!(
            "{}{} [{}-{}] «{}»\n",
            prefix,
            node.kind(),
            node.start_position().row,
            node.end_position().row,
            short
        ));
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            fmt_node(child, source, indent + 1, out);
        }
    }
    let mut out = String::new();
    fmt_node(tree.root_node(), source.as_bytes(), 0, &mut out);
    out
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

/// Map a keyword string to DefKind.
fn def_kind_from_keyword(keyword: &str) -> Option<DefKind> {
    match keyword {
        "part" => Some(DefKind::Part),
        "port" => Some(DefKind::Port),
        "connection" => Some(DefKind::Connection),
        "interface" => Some(DefKind::Interface),
        "flow" => Some(DefKind::Flow),
        "action" => Some(DefKind::Action),
        "state" => Some(DefKind::State),
        "constraint" => Some(DefKind::Constraint),
        "calc" => Some(DefKind::Calc),
        "requirement" => Some(DefKind::Requirement),
        "use" => Some(DefKind::UseCase),       // "use case def"
        "verification" => Some(DefKind::Verification),
        "analysis" => Some(DefKind::Analysis),
        "concern" => Some(DefKind::Concern),
        "view" => Some(DefKind::View),
        "viewpoint" => Some(DefKind::Viewpoint),
        "rendering" => Some(DefKind::Rendering),
        "enum" | "enumeration" => Some(DefKind::Enum),
        "attribute" => Some(DefKind::Attribute),
        "item" => Some(DefKind::Item),
        "allocation" => Some(DefKind::Allocation),
        "occurrence" => Some(DefKind::Occurrence),
        "individual" => Some(DefKind::Occurrence),
        "metadata" => Some(DefKind::Metadata),
        // KerML
        "case" => Some(DefKind::UseCase),
        "class" => Some(DefKind::Class),
        "struct" => Some(DefKind::Struct),
        "assoc" => Some(DefKind::Assoc),
        "behavior" => Some(DefKind::Behavior),
        "datatype" => Some(DefKind::Datatype),
        "feature" => Some(DefKind::Feature),
        "function" => Some(DefKind::Function),
        "interaction" => Some(DefKind::Interaction),
        "connector" => Some(DefKind::Connector),
        "predicate" => Some(DefKind::Predicate),
        "namespace" => Some(DefKind::Namespace),
        "type" => Some(DefKind::Type),
        "classifier" => Some(DefKind::Classifier),
        "metaclass" => Some(DefKind::Metaclass),
        "expr" => Some(DefKind::Expr),
        "step" => Some(DefKind::Step),
        _ => None,
    }
}

/// Determine DefKind from a CST node.
/// Handles both the unified "definition" node (by inspecting keyword children)
/// and legacy specific nodes like "state_definition", "enumeration_definition".
fn def_kind_from_node(node: &Node, source: &[u8]) -> Option<DefKind> {
    match node.kind() {
        "definition" | "generic_definition" => {
            // Unified definition: first keyword child determines kind
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    continue;
                }
                let text = node_text(&child, source);
                if text == "def" {
                    // "def" alone = generic_definition
                    return Some(DefKind::Feature);
                }
                if let Some(k) = def_kind_from_keyword(text) {
                    return Some(k);
                }
            }
            // Fallback for generic_definition with "def" keyword
            Some(DefKind::Feature)
        }
        "state_definition" => Some(DefKind::State),
        "enumeration_definition" => Some(DefKind::Enum),
        "package_declaration" | "namespace_declaration" => Some(DefKind::Package),
        _ => None,
    }
}

/// Map a keyword string to usage kind.
fn usage_kind_from_keyword(keyword: &str) -> Option<&'static str> {
    match keyword {
        "part" => Some("part"),
        "port" => Some("port"),
        "attribute" => Some("attribute"),
        "item" => Some("item"),
        "occurrence" => Some("occurrence"),
        "calc" => Some("calc"),
        "view" => Some("view"),
        "viewpoint" => Some("viewpoint"),
        "rendering" => Some("rendering"),
        "concern" => Some("concern"),
        "analysis" => Some("analysis"),
        "verification" => Some("verification"),
        "enum" => Some("enum"),
        "message" => Some("message"),
        "case" => Some("use case"),
        "use" => Some("use case"),
        "classifier" => Some("classifier"),
        "metaclass" => Some("metaclass"),
        "expr" => Some("expr"),
        "step" => Some("step"),
        "snapshot" => Some("snapshot"),
        "timeslice" => Some("timeslice"),
        _ => None,
    }
}

/// Determine usage kind from a CST node.
/// Handles both the unified "usage" node and specific usage nodes.
fn usage_kind_from_node(node: &Node, source: &[u8]) -> Option<&'static str> {
    match node.kind() {
        "usage" => {
            // Unified usage: first keyword child determines kind
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    continue;
                }
                let text = node_text(&child, source);
                if let Some(k) = usage_kind_from_keyword(text) {
                    return Some(k);
                }
            }
            Some("part") // default
        }
        // Specific usage nodes that still exist
        "action_usage" => Some("action"),
        "state_usage" => Some("state"),
        "connection_usage" => Some("connection"),
        "interface_usage" => Some("interface"),
        "constraint_usage" => Some("constraint"),
        "requirement_usage" => Some("requirement"),
        "event_usage" => Some("event"),
        "allocation_usage" => Some("allocation"),
        "flow_usage" => Some("flow"),
        "metadata_usage" => Some("metadata"),
        "feature_usage" => Some("feature"),
        "binding_usage" => Some("binding"),
        "succession_usage" => Some("succession"),
        "succession_flow_usage" => Some("succession flow"),
        "constraint_expression_usage" => Some("constraint"),
        "kerml_usage" => {
            // KerML usage: check keyword child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() { continue; }
                match node_text(&child, source) {
                    "assoc" => return Some("assoc"),
                    "behavior" => return Some("behavior"),
                    "class" => return Some("class"),
                    "connector" => return Some("connector"),
                    "datatype" => return Some("datatype"),
                    "function" => return Some("function"),
                    "interaction" => return Some("interaction"),
                    "predicate" => return Some("predicate"),
                    "struct" => return Some("struct"),
                    "type" => return Some("type"),
                    "feature" => return Some("feature"),
                    _ => {}
                }
            }
            Some("feature")
        }
        _ => None,
    }
}

/// Check if a node kind is a structural element (definition, usage, or body).
fn is_structural_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "definition"
            | "state_definition"
            | "enumeration_definition"
            | "generic_definition"
            | "package_declaration"
            | "namespace_declaration"
            | "usage"
            | "action_usage"
            | "state_usage"
            | "connection_usage"
            | "interface_usage"
            | "constraint_usage"
            | "requirement_usage"
            | "event_usage"
            | "allocation_usage"
            | "flow_usage"
            | "metadata_usage"
            | "feature_usage"
            | "binding_usage"
            | "succession_usage"
            | "succession_flow_usage"
            | "constraint_expression_usage"
            | "kerml_usage"
            | "definition_body"
            | "state_body"
            | "{"
    )
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
            k if is_structural_boundary(k) => break,
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
            k if is_structural_boundary(k) => break,
            _ => {}
        }
        sibling = sib.prev_sibling();
    }
    false
}

/// Extract visibility modifier from preceding siblings of a node.
fn get_visibility(node: &Node) -> Option<Visibility> {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        match sib.kind() {
            "visibility" => {
                // The visibility node contains a child: public/private/protected
                let mut cursor = sib.walk();
                for child in sib.children(&mut cursor) {
                    match child.kind() {
                        "public" => return Some(Visibility::Public),
                        "private" => return Some(Visibility::Private),
                        "protected" => return Some(Visibility::Protected),
                        _ => {}
                    }
                }
                return None;
            }
            // Stop at structural boundaries
            k if is_structural_boundary(k) => break,
            _ => {}
        }
        sibling = sib.prev_sibling();
    }
    None
}

/// Check if a node has an abstract modifier in preceding siblings.
fn is_abstract(node: &Node) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        match sib.kind() {
            "abstract" => return true,
            k if is_structural_boundary(k) => break,
            _ => {}
        }
        sibling = sib.prev_sibling();
    }
    false
}

/// Extract short_name from a definition node.
fn get_short_name(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "short_name" {
            // short_name contains: < quoted_name >
            let mut sc = child.walk();
            for sc_child in child.children(&mut sc) {
                if sc_child.kind() == "quoted_name" {
                    return Some(node_text(&sc_child, source).to_string());
                }
            }
        }
    }
    None
}

/// Extract the first doc comment from a definition's body.
fn get_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "definition_body" || child.kind() == "enumeration_body" {
            let mut bc = child.walk();
            for body_child in child.children(&mut bc) {
                if body_child.kind() == "doc_comment" {
                    return extract_doc_text(&body_child, source);
                }
            }
        }
    }
    None
}

/// Extract text from a doc_comment node, stripping /* */ delimiters.
fn extract_doc_text(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block_comment" {
            let raw = node_text(&child, source);
            // Strip /* */ and trim
            let text = raw
                .strip_prefix("/*")
                .unwrap_or(raw)
                .strip_suffix("*/")
                .unwrap_or(raw)
                .trim()
                .to_string();
            return Some(text);
        }
    }
    None
}

/// Extract multiplicity from a usage node.
fn get_multiplicity(node: &Node, source: &[u8]) -> Option<Multiplicity> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "multiplicity" {
            return Some(parse_multiplicity(&child, source));
        }
    }
    None
}

/// Parse a multiplicity node into a Multiplicity struct.
fn parse_multiplicity(node: &Node, source: &[u8]) -> Multiplicity {
    let mut lower = None;
    let mut upper = None;
    let mut is_ordered = false;
    let mut is_nonunique = false;
    let mut saw_dotdot = false;
    let mut first_expr = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "[" | "]" => {}
            "*" => {
                if saw_dotdot {
                    upper = None; // * means unbounded
                } else {
                    // Standalone * — unbounded
                    return Multiplicity {
                        lower: None,
                        upper: None,
                        is_ordered: false,
                        is_nonunique: false,
                    };
                }
            }
            ".." => {
                saw_dotdot = true;
                // The first expression becomes the lower bound
                lower = first_expr.take();
            }
            "ordered" => is_ordered = true,
            "nonunique" => is_nonunique = true,
            _ => {
                let text = node_text(&child, source).to_string();
                if saw_dotdot {
                    upper = Some(text);
                } else {
                    first_expr = Some(text);
                }
            }
        }
    }

    // If no ".." was seen, the single expression is a lower bound (exact count)
    if !saw_dotdot {
        lower = first_expr;
    }

    Multiplicity {
        lower,
        upper,
        is_ordered,
        is_nonunique,
    }
}

/// Extract value expression from a usage node.
fn get_value_expr(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "value_assignment" {
            // Extract just the expression part (skip = or :=)
            let mut vc = child.walk();
            for vc_child in child.children(&mut vc) {
                match vc_child.kind() {
                    "=" | ":=" | "default" => {}
                    _ => {
                        return Some(node_text(&vc_child, source).to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract redefines target from a usage node.
/// Handles both old "redefines_keyword" and new "keyword_type_relationship" + "redefinition" nodes.
fn get_redefinition(node: &Node, source: &[u8]) -> Option<String> {
    fn search_children(node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "redefines_keyword" | "redefinition" => {
                    // Contains: :>> target or redefines target
                    if let Some(t) = child.child_by_field_name("target") {
                        return Some(node_text(&t, source).to_string());
                    }
                    let mut rc = child.walk();
                    for rc_child in child.children(&mut rc) {
                        if rc_child.kind() == "qualified_name" || rc_child.kind() == "identifier" {
                            return Some(node_text(&rc_child, source).to_string());
                        }
                    }
                }
                "keyword_type_relationship" => {
                    // Check if the keyword is "redefines"
                    let mut kc = child.walk();
                    let mut is_redefines = false;
                    for kc_child in child.children(&mut kc) {
                        if !kc_child.is_named() && node_text(&kc_child, source) == "redefines" {
                            is_redefines = true;
                        }
                        if is_redefines {
                            if let Some(t) = kc_child.child_by_field_name("target") {
                                return Some(node_text(&t, source).to_string());
                            }
                            if kc_child.kind() == "qualified_name" || kc_child.kind() == "identifier" || kc_child.kind() == "feature_chain" {
                                return Some(node_text(&kc_child, source).to_string());
                            }
                        }
                    }
                }
                _ => {
                    if let Some(r) = search_children(&child, source) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }
    search_children(node, source)
}

/// Extract subsets target from a usage node.
/// Handles both old "subsets_keyword" and new "keyword_type_relationship" nodes.
fn get_subsets(node: &Node, source: &[u8]) -> Option<String> {
    fn search_children(node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "subsets_keyword" => {
                    let mut sc = child.walk();
                    for sc_child in child.children(&mut sc) {
                        if sc_child.kind() == "qualified_name" || sc_child.kind() == "identifier" {
                            return Some(node_text(&sc_child, source).to_string());
                        }
                    }
                }
                "keyword_type_relationship" => {
                    let mut kc = child.walk();
                    let mut is_subsets = false;
                    for kc_child in child.children(&mut kc) {
                        if !kc_child.is_named() && node_text(&kc_child, source) == "subsets" {
                            is_subsets = true;
                        }
                        if is_subsets {
                            if let Some(t) = kc_child.child_by_field_name("target") {
                                return Some(node_text(&t, source).to_string());
                            }
                            if kc_child.kind() == "qualified_name" || kc_child.kind() == "identifier" || kc_child.kind() == "feature_chain" {
                                return Some(node_text(&kc_child, source).to_string());
                            }
                        }
                    }
                }
                _ => {
                    if let Some(r) = search_children(&child, source) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }
    search_children(node, source)
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
        "definition" | "state_definition" | "enumeration_definition"
        | "generic_definition" | "package_declaration" | "namespace_declaration" => {
            let Some(def_kind) = def_kind_from_node(&node, source) else {
                // Fall through to default recursion
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    walk_node_scoped(child, source, model, enclosing_verification, parent_def_name);
                }
                return;
            };
            if let Some(name) = field_text(&node, "name", source) {
                let super_type = get_supertype(&node, source);

                // Extract body structure for constraint/calc defs
                let (has_body, param_count, has_constraint_expr, has_return) =
                    if def_kind == DefKind::Constraint || def_kind == DefKind::Calc {
                        inspect_def_body(&node, def_kind)
                    } else {
                        (true, 0, false, false)
                    };

                // Extract enriched fields
                let visibility = get_visibility(&node);
                let is_abstract_val = is_abstract(&node);
                let short_name = get_short_name(&node, source);
                let doc = get_doc_comment(&node, source);
                let (body_start_byte, body_end_byte) = get_body_braces(&node);

                model.definitions.push(Definition {
                    kind: def_kind,
                    name: name.clone(),
                    super_type,
                    span: Span::from_node(&node),
                    has_body,
                    param_count,
                    has_constraint_expr,
                    has_return,
                    visibility,
                    short_name,
                    doc: doc.clone(),
                    is_abstract: is_abstract_val,
                    enum_members: Vec::new(),
                    parent_def: parent_def_name.map(|s| s.to_string()),
                    body_start_byte,
                    body_end_byte,
                    qualified_name: None,
                });

                // Extract enum members for enum definitions
                if def_kind == DefKind::Enum {
                    let def_idx = model.definitions.len() - 1;
                    let mut cursor_body = node.walk();
                    for child in node.children(&mut cursor_body) {
                        if child.kind() == "definition_body" || child.kind() == "enumeration_body" {
                            let mut bc = child.walk();
                            for body_child in child.children(&mut bc) {
                                if body_child.kind() == "enum_usage" || body_child.kind() == "enum_member" {
                                    if let Some(member_name) = field_text(&body_child, "name", source) {
                                        let member_doc = get_doc_comment(&body_child, source);
                                        model.definitions[def_idx].enum_members.push(EnumMember {
                                            name: member_name,
                                            doc: member_doc,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // Collect doc comments as model-level comments
                if let Some(text) = doc {
                    model.comments.push(Comment {
                        text,
                        locale: None,
                        parent_def: Some(name.clone()),
                        span: Span::from_node(&node),
                    });
                }

                // Extract view definition body (expose/filter)
                if def_kind == DefKind::View {
                    let view = extract_view_body(&node, source, &name);
                    model.views.push(view);
                }

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
        "usage" | "action_usage" | "state_usage" | "connection_usage"
        | "interface_usage" | "constraint_usage" | "requirement_usage"
        | "event_usage" | "allocation_usage" | "flow_usage" | "metadata_usage"
        | "feature_usage" | "binding_usage" | "succession_usage"
        | "succession_flow_usage" | "constraint_expression_usage" | "kerml_usage" => {
            let usage_kind = usage_kind_from_node(&node, source).unwrap_or("feature");
            let redefinition = get_redefinition(&node, source);
            let subsets = get_subsets(&node, source);
            // Fall back to redefines/subsets target as usage name
            // (e.g. `part redefines foo { ... }` → name is "foo")
            let name = field_text(&node, "name", source)
                .or_else(|| redefinition.as_ref().map(|r| {
                    // Strip qualified prefix: "vehicle_C1::rearAxle" → "rearAxle"
                    r.rsplit("::").next().unwrap_or(r).to_string()
                }))
                .or_else(|| subsets.as_ref().map(|s| {
                    s.rsplit("::").next().unwrap_or(s).to_string()
                }));
            if let Some(name) = name {
                let type_ref = get_type_ref(&node, source);
                if let Some(ref t) = type_ref {
                    model.type_references.push(TypeReference {
                        name: t.clone(),
                        span: Span::from_node(&node),
                    });
                }
                let direction = get_direction(&node);
                let conjugated = is_conjugated_type(&node, source);
                let multiplicity = get_multiplicity(&node, source);
                let value_expr = get_value_expr(&node, source);
                let short_name = get_short_name(&node, source);
                model.usages.push(Usage {
                    kind: usage_kind.to_string(),
                    name: name.clone(),
                    type_ref,
                    span: Span::from_node(&node),
                    direction,
                    is_conjugated: conjugated,
                    parent_def: parent_def_name.map(|s| s.to_string()),
                    multiplicity,
                    value_expr,
                    short_name,
                    redefinition,
                    subsets,
                    qualified_name: None,
                });

                // Connection and interface usages can have connect clauses
                if kind == "connection_usage" || kind == "interface_usage" {
                    extract_connect_clause(&node, source, model);
                }

                // If this usage has a body, recurse with this usage as the
                // parent scope so nested usages get the correct parent_def.
                let has_body = node.children(&mut node.walk())
                    .any(|c| c.kind().ends_with("_body") || c.kind() == "{");
                if has_body {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        walk_node_scoped(child, source, model,
                            enclosing_verification, Some(&name));
                    }
                    return; // Already recursed with updated scope
                }
            } else {
                // Connection/interface usages without names
                if kind == "connection_usage" || kind == "interface_usage" {
                    extract_connect_clause(&node, source, model);
                }
                // Unnamed constraint_usage (e.g., require constraint { expr })
                // Extract the body expression text and store as a usage
                if kind == "constraint_usage" {
                    let body_text = extract_constraint_body_text(&node, source);
                    if let Some(expr_text) = body_text {
                        model.usages.push(Usage {
                            kind: "constraint".to_string(),
                            name: String::new(),
                            type_ref: None,
                            span: Span::from_node(&node),
                            direction: None,
                            is_conjugated: false,
                            parent_def: parent_def_name.map(|s| s.to_string()),
                            multiplicity: None,
                            value_expr: Some(expr_text),
                            short_name: None,
                            redefinition: None,
                            subsets: None,
                            qualified_name: None,
                        });
                    }
                }
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

        // --- Transition statement: `transition [name] first Source [accept ...] [if guard] [do effect] then Target;` ---
        "transition_statement" => {
            let name = field_text(&node, "name", source);
            let mut source_state: Option<String> = None;
            let mut target_state: Option<String> = None;
            let mut saw_first = false;
            let mut saw_then = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "first" => { saw_first = true; }
                    "then" => { saw_then = true; }
                    "qualified_name" | "identifier" | "feature_chain" => {
                        let text = node_text(&child, source).to_string();
                        if saw_then {
                            target_state = Some(text);
                        } else if saw_first && source_state.is_none() {
                            source_state = Some(text);
                        }
                    }
                    _ => {}
                }
            }
            if target_state.is_some() {
                model.usages.push(Usage {
                    kind: "transition".to_string(),
                    name: name.unwrap_or_default(),
                    type_ref: target_state,
                    span: Span::from_node(&node),
                    direction: None,
                    is_conjugated: false,
                    parent_def: parent_def_name.map(|s| s.to_string()),
                    multiplicity: None,
                    value_expr: source_state, // source state stored here
                    short_name: None,
                    redefinition: None,
                    subsets: None,
                    qualified_name: None,
                });
            }
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

        // --- Control flow nodes (fork, join, merge, decide) ---
        "fork_node" | "join_node" | "merge_node" | "decide_node" | "control_node" => {
            // For unified control_node, determine kind from first keyword child
            let ctrl_kind = if kind == "control_node" {
                let mut cursor = node.walk();
                let mut found = "control_node".to_string();
                for child in node.children(&mut cursor) {
                    match node_text(&child, source) {
                        "fork" => { found = "fork_node".to_string(); break; }
                        "join" => { found = "join_node".to_string(); break; }
                        "merge" => { found = "merge_node".to_string(); break; }
                        "decide" => { found = "decide_node".to_string(); break; }
                        _ => {}
                    }
                }
                found
            } else {
                kind.to_string()
            };
            if let Some(name) = field_text(&node, "name", source) {
                model.usages.push(Usage {
                    kind: ctrl_kind,
                    name,
                    type_ref: None,
                    span: Span::from_node(&node),
                    direction: None,
                    is_conjugated: false,
                    parent_def: parent_def_name.map(|s| s.to_string()),
                    multiplicity: None,
                    value_expr: None,
                    short_name: None,
                    redefinition: None,
                    subsets: None,
                    qualified_name: None,
                });
            }
        }

        // --- Dependency statement ---
        "dependency_statement" => {
            let name = field_text(&node, "name", source);
            let mut from_refs = Vec::new();
            let mut to_refs = Vec::new();
            let mut after_from = false;
            let mut after_to = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "from" => { after_from = true; after_to = false; }
                    "to" => { after_to = true; after_from = false; }
                    "qualified_name" | "feature_chain" | "identifier" => {
                        let text = node_text(&child, source).to_string();
                        if after_to {
                            to_refs.push(text);
                        } else if after_from {
                            from_refs.push(text);
                        } else if name.is_none() {
                            // Unnamed: first ref before "to" is the source
                            from_refs.push(text);
                        }
                    }
                    _ => {}
                }
            }
            for from in &from_refs {
                for to in &to_refs {
                    model.connections.push(Connection {
                        name: name.clone(),
                        source: from.clone(),
                        target: to.clone(),
                        span: Span::from_node(&node),
                    });
                }
            }
        }

        // --- Connect statement (standalone) ---
        "connect_statement" => {
            extract_connect_clause(&node, source, model);
            // Also try direct "ref to ref" pattern
            let mut refs = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "qualified_name" | "feature_chain" | "identifier") {
                    refs.push(node_text(&child, source).to_string());
                }
            }
            if refs.len() >= 2 {
                model.connections.push(Connection {
                    name: None,
                    source: refs[0].clone(),
                    target: refs[1].clone(),
                    span: Span::from_node(&node),
                });
            }
        }

        // --- Message statement ---
        "message_statement" => {
            let name = field_text(&node, "name", source);
            let mut from_ref = None;
            let mut to_ref = None;
            let mut after_from = false;
            let mut after_to = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "from" => { after_from = true; }
                    "to" => { after_to = true; after_from = false; }
                    "qualified_name" | "feature_chain" | "identifier" => {
                        let text = node_text(&child, source).to_string();
                        if after_to && to_ref.is_none() {
                            to_ref = Some(text);
                        } else if after_from && from_ref.is_none() {
                            from_ref = Some(text);
                        }
                    }
                    _ => {}
                }
            }
            if let (Some(src), Some(tgt)) = (from_ref, to_ref) {
                model.flows.push(Flow {
                    name,
                    item_type: None,
                    source: src,
                    target: tgt,
                    span: Span::from_node(&node),
                });
            }
        }

        // --- Verify statement ---
        "verify_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "qualified_name" | "identifier") {
                    let req_name = node_text(&child, source).to_string();
                    if let Some(ver_name) = enclosing_verification.or(parent_def_name) {
                        model.verifications.push(Verification {
                            requirement: req_name,
                            by: ver_name.to_string(),
                            span: Span::from_node(&node),
                        });
                    }
                    break;
                }
            }
        }

        // --- Metadata annotation (@Risk { ... }) ---
        "metadata_annotation" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "qualified_name" | "identifier" | "feature_chain") {
                    let meta_name = node_text(&child, source).to_string();
                    model.usages.push(Usage {
                        kind: "metadata".to_string(),
                        name: meta_name,
                        type_ref: None,
                        span: Span::from_node(&node),
                        direction: None,
                        is_conjugated: false,
                        parent_def: parent_def_name.map(|s| s.to_string()),
                        multiplicity: None,
                        value_expr: None,
                        short_name: None,
                        redefinition: None,
                        subsets: None,
                        qualified_name: None,
                    });
                    break;
                }
            }
        }

        // --- Assert statement ---
        "assert_statement" => {
            // assert not constraint X; or assert constraint X;
            // Recurse to find nested constraint_usage
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_node_scoped(child, source, model, enclosing_verification, parent_def_name);
            }
            return;
        }

        // --- Accept action (standalone accept outside transition) ---
        "accept_action" => {
            // Accept actions reference types/events
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "qualified_name" | "identifier") {
                    let name = node_text(&child, source).to_string();
                    model.type_references.push(TypeReference {
                        name,
                        span: Span::from_node(&node),
                    });
                    break;
                }
            }
        }

        // --- Subject, objective, actor declarations (analysis/use-case bodies) ---
        "subject_declaration" | "objective_declaration" | "actor_declaration"
        | "stakeholder_declaration" | "frame_statement" => {
            let decl_kind = match kind {
                "subject_declaration" => "subject",
                "objective_declaration" => "objective",
                "actor_declaration" => "actor",
                "stakeholder_declaration" => "stakeholder",
                "frame_statement" => "frame",
                _ => "unknown",
            };
            let name = field_text(&node, "name", source).unwrap_or_default();
            let type_ref = get_type_ref(&node, source);
            let value_expr = get_value_expr(&node, source);
            model.usages.push(Usage {
                kind: decl_kind.to_string(),
                name,
                type_ref,
                span: Span::from_node(&node),
                direction: None,
                is_conjugated: false,
                parent_def: parent_def_name.map(|s| s.to_string()),
                multiplicity: get_multiplicity(&node, source),
                value_expr,
                short_name: None,
                redefinition: None,
                subsets: None,
                qualified_name: None,
            });
            // Recurse into body for nested elements
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_node_scoped(child, source, model, enclosing_verification, parent_def_name);
            }
            return;
        }

        // --- Return statement ---
        "return_statement" => {
            // Extract return name and type
            let name = field_text(&node, "name", source).unwrap_or_else(|| "return".to_string());
            let type_ref = get_type_ref(&node, source);
            let value_expr = get_value_expr(&node, source);
            model.usages.push(Usage {
                kind: "return".to_string(),
                name,
                type_ref,
                span: Span::from_node(&node),
                direction: None,
                is_conjugated: false,
                parent_def: parent_def_name.map(|s| s.to_string()),
                multiplicity: None,
                value_expr,
                short_name: None,
                redefinition: None,
                subsets: None,
                qualified_name: None,
            });
        }

        // --- Succession statement: `first X then Y` ---
        "succession_statement" => {
            let mut names: Vec<String> = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "qualified_name" || child.kind() == "identifier" {
                    names.push(node_text(&child, source).to_string());
                }
            }
            if names.len() >= 2 {
                // Create a succession usage with source→target in name/type_ref
                model.usages.push(Usage {
                    kind: "succession".to_string(),
                    name: names[0].clone(),
                    type_ref: Some(names[1].clone()),
                    span: Span::from_node(&node),
                    direction: None,
                    is_conjugated: false,
                    parent_def: parent_def_name.map(|s| s.to_string()),
                    multiplicity: None,
                    value_expr: None,
                    short_name: None,
                    redefinition: None,
                    subsets: None,
                    qualified_name: None,
                });
            }
            // Check for then_succession children (fork branches: `then X;`)
            let mut cursor2 = node.walk();
            for child in node.children(&mut cursor2) {
                if child.kind() == "then_succession" {
                    let mut cursor3 = child.walk();
                    for grandchild in child.children(&mut cursor3) {
                        if grandchild.kind() == "qualified_name" || grandchild.kind() == "identifier" {
                            let target = node_text(&grandchild, source).to_string();
                            model.usages.push(Usage {
                                kind: "then_succession".to_string(),
                                name: target,
                                type_ref: None,
                                span: Span::from_node(&child),
                                direction: None,
                                is_conjugated: false,
                                parent_def: parent_def_name.map(|s| s.to_string()),
                                multiplicity: None,
                                value_expr: None,
                                short_name: None,
                                redefinition: None,
                                subsets: None,
                                qualified_name: None,
                            });
                            break;
                        }
                    }
                }
            }
            return; // Already handled children
        }

        // --- Standalone then_succession (outside succession_statement) ---
        "then_succession" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "qualified_name" || child.kind() == "identifier" {
                    let target = node_text(&child, source).to_string();
                    model.usages.push(Usage {
                        kind: "then_succession".to_string(),
                        name: target,
                        type_ref: None,
                        span: Span::from_node(&node),
                        direction: None,
                        is_conjugated: false,
                        parent_def: parent_def_name.map(|s| s.to_string()),
                        multiplicity: None,
                        value_expr: None,
                        short_name: None,
                        redefinition: None,
                        subsets: None,
                        qualified_name: None,
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
///
/// Handles two forms:
///   connect a.x to b.y              → source="a.x", target="b.y"
///   connect ep1 ::> a.x to ep2 ::> b.y  → source="a.x", target="b.y"
///
/// When an endpoint has a binding (`::>`), the binding's feature chain is
/// the actual reference; the preceding qualified_name is just a local alias.
fn extract_connect_clause(node: &Node, source: &[u8], model: &mut Model) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "connect_clause" {
            // Collect children into a vec for lookahead
            let children: Vec<_> = {
                let mut cc_cursor = child.walk();
                child.children(&mut cc_cursor).collect()
            };
            let mut refs = Vec::new();
            let mut i = 0;
            while i < children.len() {
                let cc_child = &children[i];
                match cc_child.kind() {
                    "qualified_name" | "feature_chain" | "identifier" => {
                        // Check if next sibling is a binding (::> ref)
                        if i + 1 < children.len() && children[i + 1].kind() == "binding" {
                            // Use the binding's feature chain / qualified_name
                            let binding = &children[i + 1];
                            let mut b_cursor = binding.walk();
                            for b_child in binding.children(&mut b_cursor) {
                                if b_child.kind() == "feature_chain"
                                    || b_child.kind() == "qualified_name"
                                {
                                    refs.push(node_text(&b_child, source).to_string());
                                    break;
                                }
                            }
                            i += 2; // skip both the name and the binding
                        } else {
                            refs.push(node_text(cc_child, source).to_string());
                            i += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
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
/// Extract the byte positions of the opening `{` and closing `}` of a definition body.
fn get_body_braces(node: &Node) -> (Option<usize>, Option<usize>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "definition_body" || child.kind() == "state_body" || child.kind() == "enumeration_body" {
            let mut body_cursor = child.walk();
            let mut open = None;
            let mut close = None;
            for body_child in child.children(&mut body_cursor) {
                if body_child.kind() == "{" {
                    open = Some(body_child.start_byte());
                } else if body_child.kind() == "}" {
                    close = Some(body_child.start_byte());
                }
            }
            return (open, close);
        }
    }
    (None, None)
}

/// Extract expose and filter clauses from a view definition body.
fn extract_view_body(node: &Node, source: &[u8], name: &str) -> ViewDef {
    let mut exposes = Vec::new();
    let mut kind_filters = Vec::new();
    let mut render_as = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_view_body_node(&child, source, &mut exposes, &mut kind_filters, &mut render_as);
    }

    ViewDef {
        name: name.to_string(),
        exposes,
        kind_filters,
        render_as,
        span: Span::from_node(node),
    }
}

fn visit_view_body_node(
    node: &Node,
    source: &[u8],
    exposes: &mut Vec<String>,
    kind_filters: &mut Vec<String>,
    render_as: &mut Option<String>,
) {
    match node.kind() {
        "expose_statement" => {
            // Collect the full text of the expose, extracting the qualified name
            let text = node_text(node, source).trim().to_string();
            // Parse out the qualified name from "expose QualifiedName::*;"
            let stripped = text
                .strip_prefix("expose ")
                .unwrap_or(&text)
                .trim_end_matches(';')
                .trim();
            if !stripped.is_empty() {
                exposes.push(stripped.to_string());
            }
        }
        "filter_statement" => {
            // Try to extract kind filters from filter statements
            // Look for patterns like: filter @SysML::Metadata::KindFilter {kind = part;}
            // or simpler usage-kind patterns
            let text = node_text(node, source).trim().to_string();
            // Extract kind= value if present
            if let Some(pos) = text.find("kind") {
                let after = &text[pos + 4..];
                let after = after.trim_start_matches(|c: char| c == ' ' || c == '=');
                let kind_val: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !kind_val.is_empty() {
                    kind_filters.push(kind_val);
                }
            }
        }
        _ => {
            // Check for render clause: "render asInterconnectionDiagram;"
            let text = node_text(node, source).trim().to_string();
            if text.starts_with("render ") {
                let val = text
                    .strip_prefix("render ")
                    .unwrap_or("")
                    .trim_end_matches(';')
                    .trim();
                if !val.is_empty() {
                    *render_as = Some(val.to_string());
                }
            }
            // Recurse into child nodes
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                visit_view_body_node(&child, source, exposes, kind_filters, render_as);
            }
        }
    }
}

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

/// Extract expression text from a constraint_body or definition_body.
fn extract_constraint_body_text(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "constraint_body" || child.kind() == "definition_body" {
            let mut bc = child.walk();
            for body_child in child.children(&mut bc) {
                match body_child.kind() {
                    "expression_statement" | "result_expression" => {
                        let text = node_text(&body_child, source).trim_end_matches(';').trim();
                        if !text.is_empty() {
                            return Some(text.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
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
