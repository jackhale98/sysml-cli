//! Format-agnostic diagram intermediate representation.
//!
//! Aligned with SysML v2 StandardViewDefinitions:
//! GeneralView, InterconnectionView, ActionFlowView, StateTransitionView,
//! SequenceView, GridView, BrowserView.

/// The kind of diagram, aligned with SysML v2 StandardViewDefinitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    /// GeneralView (gv) — general graph of definitions and relationships.
    GeneralView(GeneralViewFlavor),
    /// InterconnectionView (iv) — internal structure with ports and connections.
    InterconnectionView,
    /// ActionFlowView (afv) — action flows with control nodes.
    ActionFlowView,
    /// StateTransitionView (stv) — states and transitions.
    StateTransitionView,
    /// SequenceView (sv) — lifelines and messages.
    SequenceView,
    /// GridView (grv) — tabular/matrix presentations.
    GridView(GridViewFlavor),
    /// BrowserView (bv) — hierarchical tree/outline.
    BrowserView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralViewFlavor {
    Default,
    Parametric,
    UseCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridViewFlavor {
    Default,
    Requirements,
    Trace,
    Alloc,
}

impl DiagramKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::GeneralView(GeneralViewFlavor::Default) => "General View",
            Self::GeneralView(GeneralViewFlavor::Parametric) => "Parametric View",
            Self::GeneralView(GeneralViewFlavor::UseCase) => "Use Case View",
            Self::InterconnectionView => "Interconnection View",
            Self::ActionFlowView => "Action Flow View",
            Self::StateTransitionView => "State Transition View",
            Self::SequenceView => "Sequence View",
            Self::GridView(GridViewFlavor::Default) => "Grid View",
            Self::GridView(GridViewFlavor::Requirements) => "Requirements View",
            Self::GridView(GridViewFlavor::Trace) => "Traceability View",
            Self::GridView(GridViewFlavor::Alloc) => "Allocation View",
            Self::BrowserView => "Browser View",
        }
    }

    pub fn abbreviation(&self) -> &'static str {
        match self {
            Self::GeneralView(_) => "gv",
            Self::InterconnectionView => "iv",
            Self::ActionFlowView => "afv",
            Self::StateTransitionView => "stv",
            Self::SequenceView => "sv",
            Self::GridView(_) => "grv",
            Self::BrowserView => "bv",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            // Canonical SysML v2 standard names
            "gv" | "generalview" => Some(Self::GeneralView(GeneralViewFlavor::Default)),
            "iv" | "interconnectionview" => Some(Self::InterconnectionView),
            "afv" | "actionflowview" => Some(Self::ActionFlowView),
            "stv" | "statetransitionview" => Some(Self::StateTransitionView),
            "sv" | "sequenceview" | "sequence" => Some(Self::SequenceView),
            "grv" | "gridview" => Some(Self::GridView(GridViewFlavor::Default)),
            "bv" | "browserview" => Some(Self::BrowserView),
            // Legacy aliases (backward compatibility)
            "bdd" => Some(Self::GeneralView(GeneralViewFlavor::Default)),
            "ibd" => Some(Self::InterconnectionView),
            "stm" | "state" => Some(Self::StateTransitionView),
            "act" | "activity" => Some(Self::ActionFlowView),
            "req" | "requirements" => Some(Self::GridView(GridViewFlavor::Requirements)),
            "pkg" | "package" => Some(Self::BrowserView),
            "par" | "parametric" => Some(Self::GeneralView(GeneralViewFlavor::Parametric)),
            "trace" | "traceability" => Some(Self::GridView(GridViewFlavor::Trace)),
            "alloc" | "allocation" => Some(Self::GridView(GridViewFlavor::Alloc)),
            "ucd" | "usecase" | "use-case" => Some(Self::GeneralView(GeneralViewFlavor::UseCase)),
            _ => None,
        }
    }

    /// Parse a SysML v2 render clause (e.g., "asInterconnectionDiagram").
    pub fn from_render_clause(s: &str) -> Option<Self> {
        match s {
            "asGeneralDiagram" => Some(Self::GeneralView(GeneralViewFlavor::Default)),
            "asInterconnectionDiagram" => Some(Self::InterconnectionView),
            "asActionFlowDiagram" => Some(Self::ActionFlowView),
            "asStateTransitionDiagram" => Some(Self::StateTransitionView),
            "asSequenceDiagram" => Some(Self::SequenceView),
            "asTableDiagram" | "asGridDiagram" => Some(Self::GridView(GridViewFlavor::Default)),
            "asBrowserDiagram" | "asTreeDiagram" => Some(Self::BrowserView),
            _ => None,
        }
    }
}

/// Layout direction for the diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutDirection {
    #[default]
    TopBottom,
    LeftRight,
    BottomTop,
    RightLeft,
}

impl LayoutDirection {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "TB" | "TD" | "TOP-BOTTOM" | "TOP-DOWN" => Some(Self::TopBottom),
            "LR" | "LEFT-RIGHT" => Some(Self::LeftRight),
            "BT" | "BOTTOM-TOP" | "BOTTOM-UP" => Some(Self::BottomTop),
            "RL" | "RIGHT-LEFT" => Some(Self::RightLeft),
            _ => None,
        }
    }

    pub fn mermaid_code(&self) -> &'static str {
        match self {
            Self::TopBottom => "TD",
            Self::LeftRight => "LR",
            Self::BottomTop => "BT",
            Self::RightLeft => "RL",
        }
    }

    pub fn dot_code(&self) -> &'static str {
        match self {
            Self::TopBottom => "TB",
            Self::LeftRight => "LR",
            Self::BottomTop => "BT",
            Self::RightLeft => "RL",
        }
    }

    pub fn d2_code(&self) -> &'static str {
        match self {
            Self::TopBottom => "down",
            Self::LeftRight => "right",
            Self::BottomTop => "up",
            Self::RightLeft => "left",
        }
    }
}

/// A node in the diagram graph.
#[derive(Debug, Clone)]
pub struct DiagramNode {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    pub stereotype: Option<String>,
    pub attributes: Vec<(String, String)>,
    /// SysML v2 convention: definitions have square corners, usages have rounded.
    pub is_definition: bool,
}

impl DiagramNode {
    /// Create a node for a definition (square corners).
    pub fn definition(id: String, label: String, kind: NodeKind) -> Self {
        Self {
            id,
            label,
            kind,
            stereotype: None,
            attributes: Vec::new(),
            is_definition: true,
        }
    }

    /// Create a node for a usage (rounded corners).
    pub fn usage(id: String, label: String, kind: NodeKind) -> Self {
        Self {
            id,
            label,
            kind,
            stereotype: None,
            attributes: Vec::new(),
            is_definition: false,
        }
    }
}

/// Classification of diagram nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Block,
    Port,
    State,
    Action,
    Decision,
    Fork,
    Join,
    Requirement,
    Constraint,
    Package,
    InitialState,
    FinalState,
    Note,
    UseCase,
    Actor,
    Lifeline,
}

/// An edge in the diagram graph.
#[derive(Debug, Clone)]
pub struct DiagramEdge {
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub kind: EdgeKind,
}

/// Classification of diagram edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Composition,
    Specialization,
    Connection,
    Flow,
    Transition,
    Satisfy,
    Verify,
    Allocate,
    Dependency,
    Containment,
    ConstraintBinding,
    Message,
}

/// A subgraph (containment group).
#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub label: String,
    pub node_ids: Vec<String>,
}

/// The complete diagram graph.
#[derive(Debug, Clone)]
pub struct DiagramGraph {
    pub title: String,
    pub kind: DiagramKind,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub subgraphs: Vec<Subgraph>,
    pub direction: LayoutDirection,
    pub max_depth: Option<usize>,
}

impl DiagramGraph {
    pub fn new(title: String, kind: DiagramKind) -> Self {
        Self {
            title,
            kind,
            nodes: Vec::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            direction: LayoutDirection::default(),
            max_depth: None,
        }
    }

    pub fn add_node(&mut self, node: DiagramNode) {
        self.nodes.push(node);
    }

    pub fn add_edge(&mut self, edge: DiagramEdge) {
        self.edges.push(edge);
    }

    pub fn add_subgraph(&mut self, subgraph: Subgraph) {
        self.subgraphs.push(subgraph);
    }

    pub fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n.id == id)
    }

    /// Remove nodes and edges that don't match the allowed set of names.
    pub fn filter_by_names(&mut self, allowed: &std::collections::HashSet<&str>) {
        self.nodes.retain(|n| {
            matches!(
                n.kind,
                NodeKind::InitialState
                    | NodeKind::FinalState
                    | NodeKind::Fork
                    | NodeKind::Join
                    | NodeKind::Decision
            ) || allowed.contains(n.id.as_str())
        });
        let remaining: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.id.as_str()).collect();
        self.edges.retain(|e| {
            remaining.contains(e.source.as_str()) || remaining.contains(e.target.as_str())
        });
        self.subgraphs.iter_mut().for_each(|sg| {
            sg.node_ids.retain(|id| remaining.contains(id.as_str()));
        });
        self.subgraphs.retain(|sg| !sg.node_ids.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_parse() {
        assert_eq!(
            DiagramKind::from_str("gv"),
            Some(DiagramKind::GeneralView(GeneralViewFlavor::Default))
        );
        assert_eq!(
            DiagramKind::from_str("iv"),
            Some(DiagramKind::InterconnectionView)
        );
        assert_eq!(
            DiagramKind::from_str("afv"),
            Some(DiagramKind::ActionFlowView)
        );
        assert_eq!(
            DiagramKind::from_str("stv"),
            Some(DiagramKind::StateTransitionView)
        );
        assert_eq!(DiagramKind::from_str("sv"), Some(DiagramKind::SequenceView));
        assert_eq!(
            DiagramKind::from_str("grv"),
            Some(DiagramKind::GridView(GridViewFlavor::Default))
        );
        assert_eq!(DiagramKind::from_str("bv"), Some(DiagramKind::BrowserView));
    }

    #[test]
    fn legacy_aliases_parse() {
        assert_eq!(
            DiagramKind::from_str("bdd"),
            Some(DiagramKind::GeneralView(GeneralViewFlavor::Default))
        );
        assert_eq!(
            DiagramKind::from_str("ibd"),
            Some(DiagramKind::InterconnectionView)
        );
        assert_eq!(
            DiagramKind::from_str("stm"),
            Some(DiagramKind::StateTransitionView)
        );
        assert_eq!(
            DiagramKind::from_str("act"),
            Some(DiagramKind::ActionFlowView)
        );
        assert_eq!(
            DiagramKind::from_str("req"),
            Some(DiagramKind::GridView(GridViewFlavor::Requirements))
        );
        assert_eq!(DiagramKind::from_str("pkg"), Some(DiagramKind::BrowserView));
        assert_eq!(
            DiagramKind::from_str("par"),
            Some(DiagramKind::GeneralView(GeneralViewFlavor::Parametric))
        );
        assert_eq!(
            DiagramKind::from_str("trace"),
            Some(DiagramKind::GridView(GridViewFlavor::Trace))
        );
        assert_eq!(
            DiagramKind::from_str("alloc"),
            Some(DiagramKind::GridView(GridViewFlavor::Alloc))
        );
        assert_eq!(
            DiagramKind::from_str("ucd"),
            Some(DiagramKind::GeneralView(GeneralViewFlavor::UseCase))
        );
    }

    #[test]
    fn render_clause_parsing() {
        assert_eq!(
            DiagramKind::from_render_clause("asInterconnectionDiagram"),
            Some(DiagramKind::InterconnectionView)
        );
        assert_eq!(
            DiagramKind::from_render_clause("asSequenceDiagram"),
            Some(DiagramKind::SequenceView)
        );
        assert_eq!(DiagramKind::from_render_clause("unknown"), None);
    }

    #[test]
    fn abbreviations() {
        assert_eq!(
            DiagramKind::GeneralView(GeneralViewFlavor::Default).abbreviation(),
            "gv"
        );
        assert_eq!(DiagramKind::InterconnectionView.abbreviation(), "iv");
        assert_eq!(DiagramKind::SequenceView.abbreviation(), "sv");
    }
}
