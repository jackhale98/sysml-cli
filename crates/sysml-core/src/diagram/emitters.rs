/// Diagram format emitters: convert DiagramGraph to text output.

use super::graph::*;

/// Output format for diagram rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramFormat {
    Mermaid,
    PlantUml,
    Dot,
    D2,
}

impl DiagramFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mermaid" | "mmd" => Some(Self::Mermaid),
            "plantuml" | "puml" => Some(Self::PlantUml),
            "dot" | "graphviz" => Some(Self::Dot),
            "d2" | "terrastruct" => Some(Self::D2),
            _ => None,
        }
    }
}

/// Render a DiagramGraph in the specified format.
pub fn render(graph: &DiagramGraph, format: DiagramFormat) -> String {
    match format {
        DiagramFormat::Mermaid => render_mermaid(graph),
        DiagramFormat::PlantUml => render_plantuml(graph),
        DiagramFormat::Dot => render_dot(graph),
        DiagramFormat::D2 => render_d2(graph),
    }
}

// ========================================================================
// Mermaid
// ========================================================================

fn render_mermaid(graph: &DiagramGraph) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", graph.title));
    out.push_str("---\n");

    match graph.kind {
        DiagramKind::Stm => render_mermaid_stm(&mut out, graph),
        DiagramKind::Act => render_mermaid_activity(&mut out, graph),
        DiagramKind::Req => render_mermaid_flowchart(&mut out, graph),
        _ => render_mermaid_class_diagram(&mut out, graph),
    }

    out
}

fn render_mermaid_class_diagram(out: &mut String, graph: &DiagramGraph) {
    out.push_str("classDiagram\n");

    // Subgraphs as namespaces
    let subgraph_nodes: std::collections::HashSet<&str> = graph
        .subgraphs
        .iter()
        .flat_map(|sg| sg.node_ids.iter().map(|s| s.as_str()))
        .collect();

    for sg in &graph.subgraphs {
        out.push_str(&format!("    namespace {} {{\n", sg.label));
        for nid in &sg.node_ids {
            if let Some(node) = graph.nodes.iter().find(|n| n.id == *nid) {
                render_mermaid_class_node(out, node, "        ");
            }
        }
        out.push_str("    }\n");
    }

    // Standalone nodes
    for node in &graph.nodes {
        if !subgraph_nodes.contains(node.id.as_str()) {
            render_mermaid_class_node(out, node, "    ");
        }
    }

    // Edges
    for edge in &graph.edges {
        let arrow = match edge.kind {
            EdgeKind::Specialization => "<|--",
            EdgeKind::Composition => "*--",
            EdgeKind::Connection => "--",
            EdgeKind::Satisfy => "..>",
            EdgeKind::Verify => "..>",
            EdgeKind::Dependency => "..>",
            EdgeKind::ConstraintBinding => "--",
            _ => "-->",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(" : {}", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "    {} {} {}{}\n",
            mermaid_id(&edge.target), arrow, mermaid_id(&edge.source), label
        ));
    }
}

fn render_mermaid_class_node(out: &mut String, node: &DiagramNode, indent: &str) {
    let id = mermaid_id(&node.id);
    out.push_str(&format!("{}class {} {{\n", indent, id));
    if let Some(ref st) = node.stereotype {
        out.push_str(&format!("{}    {}\n", indent, st));
    }
    for (key, val) in &node.attributes {
        out.push_str(&format!("{}    +{} {}\n", indent, key, val));
    }
    out.push_str(&format!("{}}}\n", indent));
}

fn render_mermaid_stm(out: &mut String, graph: &DiagramGraph) {
    out.push_str("stateDiagram-v2\n");
    if graph.direction != LayoutDirection::TopBottom {
        out.push_str(&format!("    direction {}\n", graph.direction.mermaid_code()));
    }
    for node in &graph.nodes {
        let id = mermaid_id(&node.id);
        match node.kind {
            NodeKind::InitialState | NodeKind::FinalState => {}
            NodeKind::State => {
                if node.attributes.is_empty() {
                    out.push_str(&format!("    {} : {}\n", id, node.label));
                } else {
                    out.push_str(&format!("    state \"{}\" as {} {{\n", node.label, id));
                    for (k, v) in &node.attributes {
                        out.push_str(&format!("        {} / {}\n", k, v));
                    }
                    out.push_str("    }\n");
                }
            }
            _ => {}
        }
    }
    for edge in &graph.edges {
        let src = if edge.source == "__initial__" {
            "[*]".to_string()
        } else {
            mermaid_id(&edge.source)
        };
        let tgt = if edge.target == "__final__" {
            "[*]".to_string()
        } else {
            mermaid_id(&edge.target)
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(" : {}", l))
            .unwrap_or_default();
        out.push_str(&format!("    {} --> {}{}\n", src, tgt, label));
    }
}

fn render_mermaid_activity(out: &mut String, graph: &DiagramGraph) {
    out.push_str(&format!("flowchart {}\n", graph.direction.mermaid_code()));
    for node in &graph.nodes {
        let id = mermaid_id(&node.id);
        let shape = match node.kind {
            NodeKind::InitialState => format!("    {}(( ))\n", id),
            NodeKind::FinalState => format!("    {}(( \u{25CF} ))\n", id),
            NodeKind::Action => format!("    {}[\"{}\"]\n", id, node.label),
            NodeKind::Decision => format!("    {}{{\"{}\"}}\n", id, node.label),
            NodeKind::Fork | NodeKind::Join => {
                format!("    {}[\"|\"]\n", id)
            }
            _ => format!("    {}[\"{}\"]\n", id, node.label),
        };
        out.push_str(&shape);
    }
    for edge in &graph.edges {
        let label = edge
            .label
            .as_ref()
            .map(|l| format!("|{}|", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "    {} -->{} {}\n",
            mermaid_id(&edge.source), label, mermaid_id(&edge.target)
        ));
    }
}

fn render_mermaid_flowchart(out: &mut String, graph: &DiagramGraph) {
    out.push_str(&format!("flowchart {}\n", graph.direction.mermaid_code()));
    for node in &graph.nodes {
        let id = mermaid_id(&node.id);
        let shape = match node.kind {
            NodeKind::Requirement => format!("{}[\"{}\"]\n", id, node.label),
            NodeKind::Block => format!("{}([\"{}\"])\n", id, node.label),
            _ => format!("{}[\"{}\"]\n", id, node.label),
        };
        out.push_str(&format!("    {}", shape));
    }
    for edge in &graph.edges {
        let style = match edge.kind {
            EdgeKind::Satisfy | EdgeKind::Verify => "-.->",
            _ => "-->",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!("|{}|", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "    {} {}{} {}\n",
            mermaid_id(&edge.source), style, label, mermaid_id(&edge.target)
        ));
    }
}

// ========================================================================
// PlantUML
// ========================================================================

fn render_plantuml(graph: &DiagramGraph) -> String {
    let mut out = String::new();
    out.push_str("@startuml\n");
    out.push_str(&format!("title {}\n\n", graph.title));

    match graph.kind {
        DiagramKind::Stm => render_plantuml_stm(&mut out, graph),
        DiagramKind::Act => render_plantuml_activity(&mut out, graph),
        DiagramKind::Req => render_plantuml_req(&mut out, graph),
        _ => render_plantuml_class(&mut out, graph),
    }

    out.push_str("@enduml\n");
    out
}

fn render_plantuml_class(out: &mut String, graph: &DiagramGraph) {
    if graph.direction != LayoutDirection::TopBottom {
        let dir = match graph.direction {
            LayoutDirection::LeftRight => "left to right direction",
            LayoutDirection::RightLeft => "right to left direction",
            LayoutDirection::BottomTop => "bottom to top direction",
            _ => "",
        };
        if !dir.is_empty() {
            out.push_str(&format!("{}\n\n", dir));
        }
    }

    // Subgraphs as packages
    let subgraph_nodes: std::collections::HashSet<&str> = graph
        .subgraphs
        .iter()
        .flat_map(|sg| sg.node_ids.iter().map(|s| s.as_str()))
        .collect();

    for sg in &graph.subgraphs {
        out.push_str(&format!("package \"{}\" {{\n", sg.label));
        for nid in &sg.node_ids {
            if let Some(node) = graph.nodes.iter().find(|n| n.id == *nid) {
                render_plantuml_class_node(out, node, "    ");
            }
        }
        out.push_str("}\n\n");
    }

    for node in &graph.nodes {
        if !subgraph_nodes.contains(node.id.as_str()) {
            render_plantuml_class_node(out, node, "");
        }
    }

    out.push('\n');

    for edge in &graph.edges {
        let arrow = match edge.kind {
            EdgeKind::Specialization => "--|>",
            EdgeKind::Composition => "*--",
            EdgeKind::Connection => "--",
            EdgeKind::Satisfy => "..>",
            EdgeKind::Verify => "..>",
            EdgeKind::Dependency => "..>",
            _ => "-->",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(" : {}", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "{} {} {}{}\n",
            edge.source, arrow, edge.target, label
        ));
    }
}

fn render_plantuml_class_node(out: &mut String, node: &DiagramNode, indent: &str) {
    let kind_kw = match node.kind {
        NodeKind::Package => "package",
        NodeKind::Constraint => "class",
        _ => "class",
    };
    let stereo = node
        .stereotype
        .as_ref()
        .map(|s| format!(" {}", s))
        .unwrap_or_default();
    out.push_str(&format!(
        "{}{} \"{}\"{} {{\n",
        indent, kind_kw, node.label, stereo
    ));
    for (key, val) in &node.attributes {
        out.push_str(&format!("{}    +{}: {}\n", indent, key, val));
    }
    out.push_str(&format!("{}}}\n", indent));
}

fn render_plantuml_stm(out: &mut String, graph: &DiagramGraph) {
    for node in &graph.nodes {
        if node.kind == NodeKind::State {
            if node.attributes.is_empty() {
                out.push_str(&format!("state \"{}\" as {}\n", node.label, node.id));
            } else {
                out.push_str(&format!("state \"{}\" as {} {{\n", node.label, node.id));
                for (k, v) in &node.attributes {
                    out.push_str(&format!("    {} : {} / {}\n", node.id, k, v));
                }
                out.push_str("}\n");
            }
        }
    }
    out.push('\n');
    for edge in &graph.edges {
        let src = if edge.source == "__initial__" {
            "[*]".to_string()
        } else {
            edge.source.clone()
        };
        let tgt = if edge.target == "__final__" {
            "[*]".to_string()
        } else {
            edge.target.clone()
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(" : {}", l))
            .unwrap_or_default();
        out.push_str(&format!("{} --> {}{}\n", src, tgt, label));
    }
}

fn render_plantuml_activity(out: &mut String, graph: &DiagramGraph) {
    out.push_str("start\n");

    // PlantUML activity diagrams use sequential syntax; we approximate from the graph
    for edge in &graph.edges {
        let target_node = graph.nodes.iter().find(|n| n.id == edge.target);
        let label = edge
            .label
            .as_ref()
            .map(|l| format!("[{}]\n", l))
            .unwrap_or_default();

        if let Some(node) = target_node {
            match node.kind {
                NodeKind::FinalState => {
                    out.push_str(&format!("{}stop\n", label));
                }
                NodeKind::Action => {
                    out.push_str(&format!("{}:{};", label, node.label));
                    out.push('\n');
                }
                NodeKind::Decision => {
                    out.push_str(&format!("{}if ({}) then (yes)\n", label, node.label));
                }
                NodeKind::Fork => {
                    out.push_str(&format!("{}fork\n", label));
                }
                NodeKind::Join => {
                    out.push_str(&format!("{}end fork\n", label));
                }
                _ => {
                    out.push_str(&format!("{}:{};", label, node.label));
                    out.push('\n');
                }
            }
        }
    }
}

fn render_plantuml_req(out: &mut String, graph: &DiagramGraph) {
    for node in &graph.nodes {
        if node.kind == NodeKind::Requirement {
            let text = node
                .attributes
                .iter()
                .find(|(k, _)| k == "text")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            out.push_str(&format!(
                "rectangle \"{}\\n{}\" as {} <<requirement>>\n",
                node.label, text, node.id
            ));
        } else {
            let stereo = node
                .stereotype
                .as_ref()
                .map(|s| format!(" {}", s))
                .unwrap_or_default();
            out.push_str(&format!(
                "rectangle \"{}\" as {}{}\n",
                node.label, node.id, stereo
            ));
        }
    }
    out.push('\n');
    for edge in &graph.edges {
        let style = match edge.kind {
            EdgeKind::Satisfy => "..>",
            EdgeKind::Verify => "..>",
            _ => "-->",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(" : {}", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "{} {} {}{}\n",
            edge.source, style, edge.target, label
        ));
    }
}

// ========================================================================
// DOT (Graphviz)
// ========================================================================

fn render_dot(graph: &DiagramGraph) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "digraph \"{}\" {{\n",
        graph.title.replace('"', "\\\"")
    ));
    out.push_str(&format!("    rankdir={};\n", graph.direction.dot_code()));
    out.push_str(&format!(
        "    label=\"{}\";\n",
        graph.title.replace('"', "\\\"")
    ));
    out.push_str("    fontsize=14;\n\n");

    // Activity diagrams get special styling
    if graph.kind == DiagramKind::Act {
        out.push_str("    node [fontname=\"Helvetica\"];\n");
        out.push_str("    edge [fontname=\"Helvetica\"];\n\n");
    }

    // Subgraphs
    for (i, sg) in graph.subgraphs.iter().enumerate() {
        out.push_str(&format!(
            "    subgraph cluster_{} {{\n",
            i
        ));
        out.push_str(&format!(
            "        label=\"{}\";\n",
            sg.label.replace('"', "\\\"")
        ));
        out.push_str("        style=dashed;\n");
        for nid in &sg.node_ids {
            out.push_str(&format!("        \"{}\";\n", nid));
        }
        out.push_str("    }\n\n");
    }

    // Nodes
    let subgraph_nodes: std::collections::HashSet<&str> = graph
        .subgraphs
        .iter()
        .flat_map(|sg| sg.node_ids.iter().map(|s| s.as_str()))
        .collect();

    for node in &graph.nodes {
        if subgraph_nodes.contains(node.id.as_str()) {
            continue;
        }
        let (shape, extra) = match node.kind {
            NodeKind::Block => ("box", ""),
            NodeKind::Port => ("diamond", ""),
            NodeKind::State => ("ellipse", ", style=rounded"),
            NodeKind::InitialState => ("circle", ", width=0.25, fixedsize=true, style=filled, fillcolor=black, label=\"\""),
            NodeKind::FinalState => ("doublecircle", ", width=0.3, fixedsize=true, style=filled, fillcolor=black, label=\"\""),
            NodeKind::Requirement => ("note", ""),
            NodeKind::Constraint => ("parallelogram", ""),
            NodeKind::Package => ("folder", ""),
            NodeKind::Action => ("box", ", style=rounded"),
            NodeKind::Decision => ("diamond", ", width=0.5, fixedsize=true"),
            NodeKind::Fork | NodeKind::Join => ("rectangle", ", width=1.5, height=0.05, fixedsize=true, style=filled, fillcolor=black, label=\"\""),
            NodeKind::Note => ("note", ""),
        };

        let label = if extra.contains("label=\"\"") {
            String::new()
        } else {
            let mut label_parts = vec![node.label.clone()];
            if let Some(ref st) = node.stereotype {
                label_parts.insert(0, st.clone());
            }
            for (k, v) in &node.attributes {
                label_parts.push(format!("{}: {}", k, v));
            }
            label_parts.join("\\n")
        };

        if extra.contains("label=\"\"") {
            out.push_str(&format!(
                "    \"{}\" [shape={}{}];\n",
                node.id, shape, extra
            ));
        } else {
            out.push_str(&format!(
                "    \"{}\" [label=\"{}\", shape={}{}];\n",
                node.id,
                label.replace('"', "\\\""),
                shape,
                extra
            ));
        }
    }

    out.push('\n');

    // Edges
    for edge in &graph.edges {
        let style = match edge.kind {
            EdgeKind::Specialization => "style=solid, arrowhead=empty",
            EdgeKind::Composition => "style=solid, arrowhead=diamond",
            EdgeKind::Satisfy | EdgeKind::Verify | EdgeKind::Dependency => {
                "style=dashed, arrowhead=open"
            }
            _ => "style=solid",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(", label=\"{}\"", l.replace('"', "\\\"")))
            .unwrap_or_default();
        out.push_str(&format!(
            "    \"{}\" -> \"{}\" [{}{}];\n",
            edge.source, edge.target, style, label
        ));
    }

    out.push_str("}\n");
    out
}

// ========================================================================
// D2
// ========================================================================

fn render_d2(graph: &DiagramGraph) -> String {
    let mut out = String::new();

    // Title and direction
    out.push_str(&format!("title: |\n  {}\n|\n\n", graph.title));
    out.push_str(&format!("direction: {}\n\n", graph.direction.d2_code()));

    // Subgraphs as containers
    let subgraph_nodes: std::collections::HashSet<&str> = graph
        .subgraphs
        .iter()
        .flat_map(|sg| sg.node_ids.iter().map(|s| s.as_str()))
        .collect();

    for sg in &graph.subgraphs {
        out.push_str(&format!("{}: {{\n", d2_id(&sg.id)));
        out.push_str(&format!("  label: \"{}\"\n", sg.label));
        for nid in &sg.node_ids {
            if let Some(node) = graph.nodes.iter().find(|n| n.id == *nid) {
                render_d2_node(&mut out, node, "  ");
            }
        }
        out.push_str("}\n\n");
    }

    // Standalone nodes
    for node in &graph.nodes {
        if !subgraph_nodes.contains(node.id.as_str()) {
            render_d2_node(&mut out, node, "");
        }
    }
    out.push('\n');

    // Edges
    for edge in &graph.edges {
        let src = d2_id(&edge.source);
        let tgt = d2_id(&edge.target);
        let arrow = match edge.kind {
            EdgeKind::Specialization => "<-",
            EdgeKind::Composition => "<-",
            EdgeKind::Satisfy | EdgeKind::Verify | EdgeKind::Dependency => "->",
            EdgeKind::Transition | EdgeKind::Flow => "->",
            _ => "->",
        };
        let label = edge
            .label
            .as_ref()
            .map(|l| format!(": {}", l))
            .unwrap_or_default();
        let style = match edge.kind {
            EdgeKind::Satisfy | EdgeKind::Verify | EdgeKind::Dependency => " {\n    style.stroke-dash: 3\n  }",
            _ => "",
        };
        out.push_str(&format!("{} {} {} {}{}\n", src, arrow, tgt, label, style));
    }

    out
}

fn render_d2_node(out: &mut String, node: &DiagramNode, indent: &str) {
    let id = d2_id(&node.id);
    let shape = match node.kind {
        NodeKind::Block | NodeKind::Action => "rectangle",
        NodeKind::Port => "diamond",
        NodeKind::State => "oval",
        NodeKind::InitialState => "circle",
        NodeKind::FinalState => "circle",
        NodeKind::Requirement | NodeKind::Note => "page",
        NodeKind::Constraint => "parallelogram",
        NodeKind::Package => "package",
        NodeKind::Decision => "diamond",
        NodeKind::Fork | NodeKind::Join => "rectangle",
    };

    if node.attributes.is_empty() && node.stereotype.is_none() {
        if node.label.is_empty() {
            out.push_str(&format!("{}{}: {{ shape: {} }}\n", indent, id, shape));
        } else {
            out.push_str(&format!("{}{}: {} {{ shape: {} }}\n", indent, id, node.label, shape));
        }
    } else {
        let stereo = node
            .stereotype
            .as_ref()
            .map(|s| format!("\\n{}", s))
            .unwrap_or_default();
        let attrs: Vec<String> = node
            .attributes
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        let attr_str = if attrs.is_empty() {
            String::new()
        } else {
            format!("\\n{}", attrs.join("\\n"))
        };
        out.push_str(&format!(
            "{}{}: |md\n{}  {}{}{}\n{}| {{ shape: {} }}\n",
            indent, id, indent, node.label, stereo, attr_str, indent, shape
        ));
    }
}

/// Make a Mermaid-safe identifier (replace special chars).
fn mermaid_id(id: &str) -> String {
    if id == "__initial__" {
        "initial".to_string()
    } else if id == "__final__" {
        "final_state".to_string()
    } else {
        id.replace(':', "_").replace(' ', "_").replace('.', "_")
    }
}

/// Make a D2-safe identifier (replace special chars).
fn d2_id(id: &str) -> String {
    if id == "__initial__" {
        "initial".to_string()
    } else if id == "__final__" {
        "final".to_string()
    } else {
        id.replace(' ', "_").replace('.', "_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::builders;
    use crate::parser::parse_file;

    #[test]
    fn mermaid_bdd_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
        "#,
        );
        let graph = builders::build_bdd(&model, None);
        let output = render(&graph, DiagramFormat::Mermaid);
        assert!(output.contains("classDiagram"));
        assert!(output.contains("Vehicle"));
        assert!(output.contains("Engine"));
    }

    #[test]
    fn plantuml_bdd_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
        "#,
        );
        let graph = builders::build_bdd(&model, None);
        let output = render(&graph, DiagramFormat::PlantUml);
        assert!(output.contains("@startuml"));
        assert!(output.contains("@enduml"));
        assert!(output.contains("Vehicle"));
    }

    #[test]
    fn dot_bdd_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
        "#,
        );
        let graph = builders::build_bdd(&model, None);
        let output = render(&graph, DiagramFormat::Dot);
        assert!(output.contains("digraph"));
        assert!(output.contains("Vehicle"));
        assert!(output.contains("Engine"));
    }

    #[test]
    fn mermaid_stm_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            state def EngineStates {
                state off;
                state running;
            }
        "#,
        );
        let graph = builders::build_stm(&model, None);
        let output = render(&graph, DiagramFormat::Mermaid);
        assert!(output.contains("stateDiagram-v2"));
        assert!(output.contains("off"));
        assert!(output.contains("running"));
    }

    #[test]
    fn plantuml_req_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            requirement def MassReq {
                doc /* mass under 2000 kg */
            }
            part def Vehicle {
                satisfy MassReq;
            }
        "#,
        );
        let graph = builders::build_req(&model);
        let output = render(&graph, DiagramFormat::PlantUml);
        assert!(output.contains("MassReq"));
        assert!(output.contains("<<requirement>>"));
    }

    #[test]
    fn format_from_str() {
        assert_eq!(DiagramFormat::from_str("mermaid"), Some(DiagramFormat::Mermaid));
        assert_eq!(DiagramFormat::from_str("puml"), Some(DiagramFormat::PlantUml));
        assert_eq!(DiagramFormat::from_str("dot"), Some(DiagramFormat::Dot));
        assert_eq!(DiagramFormat::from_str("d2"), Some(DiagramFormat::D2));
        assert_eq!(DiagramFormat::from_str("unknown"), None);
    }

    #[test]
    fn d2_bdd_output() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
        "#,
        );
        let graph = builders::build_bdd(&model, None);
        let output = render(&graph, DiagramFormat::D2);
        assert!(output.contains("Vehicle"));
        assert!(output.contains("Engine"));
        assert!(output.contains("direction:"));
    }

    #[test]
    fn dot_uses_direction() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
        "#,
        );
        let mut graph = builders::build_bdd(&model, None);
        graph.direction = LayoutDirection::LeftRight;
        let output = render(&graph, DiagramFormat::Dot);
        assert!(output.contains("rankdir=LR"));
    }

    #[test]
    fn mermaid_stm_direction() {
        let model = parse_file(
            "test.sysml",
            r#"
            state def S {
                state a;
            }
        "#,
        );
        let mut graph = builders::build_stm(&model, None);
        graph.direction = LayoutDirection::LeftRight;
        let output = render(&graph, DiagramFormat::Mermaid);
        assert!(output.contains("direction LR"));
    }

    #[test]
    fn mermaid_id_sanitizes() {
        assert_eq!(super::mermaid_id("Foo::Bar"), "Foo__Bar");
        assert_eq!(super::mermaid_id("hello world"), "hello_world");
        assert_eq!(super::mermaid_id("a.b.c"), "a_b_c");
        assert_eq!(super::mermaid_id("__initial__"), "initial");
        assert_eq!(super::mermaid_id("__final__"), "final_state");
        assert_eq!(super::mermaid_id("simple"), "simple");
    }

    #[test]
    fn mermaid_req_block_node_syntax() {
        // Regression: Block node had newline inside parens: "id([\"label\"]\n)"
        // Fixed to: "id([\"label\"])\n"
        let graph = DiagramGraph {
            title: "Test".to_string(),
            kind: DiagramKind::Req,
            nodes: vec![
                DiagramNode {
                    id: "block1".to_string(),
                    label: "MyBlock".to_string(),
                    kind: NodeKind::Block,
                    stereotype: None,
                    attributes: vec![],
                },
            ],
            edges: vec![],
            subgraphs: vec![],
            direction: LayoutDirection::TopBottom,
            max_depth: None,
        };
        let output = render(&graph, DiagramFormat::Mermaid);
        // Should NOT contain "]\n)" (broken syntax)
        assert!(!output.contains("]\n)"), "Block node should not have newline inside parens");
        // Should contain "])\n" (correct syntax)
        assert!(output.contains("])\n"), "Block node should have closing paren before newline");
    }

    #[test]
    fn mermaid_act_final_state_no_triple_paren() {
        // Regression: FinalState used ((( ))) which is invalid mermaid
        // Fixed to (( ● )) using unicode BLACK CIRCLE
        let graph = DiagramGraph {
            title: "Test".to_string(),
            kind: DiagramKind::Act,
            nodes: vec![
                DiagramNode {
                    id: "end1".to_string(),
                    label: "".to_string(),
                    kind: NodeKind::FinalState,
                    stereotype: None,
                    attributes: vec![],
                },
            ],
            edges: vec![],
            subgraphs: vec![],
            direction: LayoutDirection::TopBottom,
            max_depth: None,
        };
        let output = render(&graph, DiagramFormat::Mermaid);
        // Should NOT contain triple parens
        assert!(!output.contains("((("), "FinalState should not use triple parens");
        // Should contain double parens with filled circle
        assert!(output.contains("(("), "FinalState should use double parens");
    }

    #[test]
    fn mermaid_flowchart_uses_sanitized_ids() {
        // Verify that IDs with colons get sanitized in mermaid output
        let graph = DiagramGraph {
            title: "Test".to_string(),
            kind: DiagramKind::Req,
            nodes: vec![
                DiagramNode {
                    id: "Pkg::Req1".to_string(),
                    label: "Requirement 1".to_string(),
                    kind: NodeKind::Requirement,
                    stereotype: None,
                    attributes: vec![],
                },
            ],
            edges: vec![],
            subgraphs: vec![],
            direction: LayoutDirection::TopBottom,
            max_depth: None,
        };
        let output = render(&graph, DiagramFormat::Mermaid);
        // Should use sanitized ID (no colons)
        assert!(output.contains("Pkg__Req1"), "IDs with colons should be sanitized");
        assert!(!output.contains("Pkg::Req1"), "Raw colons should not appear in mermaid IDs");
    }

    #[test]
    fn rich_stm_with_transitions() {
        use crate::sim::state_machine::*;
        use crate::model::Span;

        let sm = StateMachineModel {
            name: "TrafficLight".to_string(),
            states: vec![
                StateNode {
                    name: "Red".to_string(),
                    entry_action: Some(ActionRef::Named("stopTraffic".to_string())),
                    do_action: None,
                    exit_action: None,
                    span: Span::default(),
                },
                StateNode {
                    name: "Green".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: Span::default(),
                },
            ],
            transitions: vec![Transition {
                name: None,
                source: "Red".to_string(),
                target: "Green".to_string(),
                trigger: Some(Trigger::Signal("timer".to_string())),
                guard: None,
                effect: Some(ActionRef::Named("startGo".to_string())),
                span: Span::default(),
            }],
            entry_state: Some("Red".to_string()),
            span: Span::default(),
        };
        let graph = builders::build_stm_from_state_machine(&sm);
        let output = render(&graph, DiagramFormat::Mermaid);
        assert!(output.contains("timer"));
        assert!(output.contains("/ startGo"));
        assert!(output.contains("Red"));
        assert!(output.contains("Green"));
    }
}
