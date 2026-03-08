/// Model types extracted from tree-sitter parse tree.
///
/// These represent the structural elements of a SysML v2 model
/// in a form suitable for running validation checks.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl Span {
    pub fn from_node(node: &tree_sitter::Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Self {
            start_row: start.row + 1,
            start_col: start.column + 1,
            end_row: end.row + 1,
            end_col: end.column + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefKind {
    Part,
    Port,
    Connection,
    Interface,
    Flow,
    Action,
    State,
    Constraint,
    Calc,
    Requirement,
    UseCase,
    Verification,
    Analysis,
    Concern,
    View,
    Viewpoint,
    Rendering,
    Enum,
    Attribute,
    Item,
    Allocation,
    Occurrence,
    Package,
    // KerML types
    Class,
    Struct,
    Assoc,
    Behavior,
    Datatype,
    Feature,
    Function,
    Interaction,
    Connector,
    Predicate,
    Namespace,
    Type,
    Classifier,
    Metaclass,
    Expr,
    Step,
    Metadata,
    Annotation,
}

impl DefKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Part => "part def",
            Self::Port => "port def",
            Self::Connection => "connection def",
            Self::Interface => "interface def",
            Self::Flow => "flow def",
            Self::Action => "action def",
            Self::State => "state def",
            Self::Constraint => "constraint def",
            Self::Calc => "calc def",
            Self::Requirement => "requirement def",
            Self::UseCase => "use case def",
            Self::Verification => "verification def",
            Self::Analysis => "analysis def",
            Self::Concern => "concern def",
            Self::View => "view def",
            Self::Viewpoint => "viewpoint def",
            Self::Rendering => "rendering def",
            Self::Enum => "enum def",
            Self::Attribute => "attribute def",
            Self::Item => "item def",
            Self::Allocation => "allocation def",
            Self::Occurrence => "occurrence def",
            Self::Package => "package",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Assoc => "assoc",
            Self::Behavior => "behavior",
            Self::Datatype => "datatype",
            Self::Feature => "feature",
            Self::Function => "function",
            Self::Interaction => "interaction",
            Self::Connector => "connector",
            Self::Predicate => "predicate",
            Self::Namespace => "namespace",
            Self::Type => "type",
            Self::Classifier => "classifier",
            Self::Metaclass => "metaclass",
            Self::Expr => "expr",
            Self::Step => "step",
            Self::Metadata => "metadata def",
            Self::Annotation => "annotation",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub kind: DefKind,
    pub name: String,
    pub super_type: Option<String>,
    pub span: Span,
    /// Whether the definition has a body block (`{ ... }`) vs just `;`.
    pub has_body: bool,
    /// Number of `in` parameters (relevant for constraint/calc defs).
    pub param_count: usize,
    /// Whether a constraint def contains a constraint expression.
    pub has_constraint_expr: bool,
    /// Whether a calc def contains a return statement.
    pub has_return: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub kind: String,
    pub name: String,
    pub type_ref: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub name: Option<String>,
    pub source: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flow {
    pub name: Option<String>,
    pub item_type: Option<String>,
    pub source: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Satisfaction {
    pub requirement: String,
    pub by: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Verification {
    pub requirement: String,
    pub by: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Allocation {
    pub source: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntaxError {
    pub message: String,
    pub context: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeReference {
    pub name: String,
    pub span: Span,
}

/// Complete model extracted from a SysML v2 file.
#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub file: String,
    pub definitions: Vec<Definition>,
    pub usages: Vec<Usage>,
    pub connections: Vec<Connection>,
    pub flows: Vec<Flow>,
    pub satisfactions: Vec<Satisfaction>,
    pub verifications: Vec<Verification>,
    pub allocations: Vec<Allocation>,
    pub syntax_errors: Vec<SyntaxError>,
    pub type_references: Vec<TypeReference>,
}

impl Model {
    pub fn new(file: String) -> Self {
        Self {
            file,
            definitions: Vec::new(),
            usages: Vec::new(),
            connections: Vec::new(),
            flows: Vec::new(),
            satisfactions: Vec::new(),
            verifications: Vec::new(),
            allocations: Vec::new(),
            syntax_errors: Vec::new(),
            type_references: Vec::new(),
        }
    }

    /// All defined names in this model.
    pub fn defined_names(&self) -> std::collections::HashSet<&str> {
        self.definitions.iter().map(|d| d.name.as_str()).collect()
    }

    /// All names that are referenced (used) in this model.
    pub fn referenced_names(&self) -> std::collections::HashSet<&str> {
        let mut refs = std::collections::HashSet::new();
        for u in &self.usages {
            if let Some(t) = &u.type_ref {
                refs.insert(simple_name(t));
            }
        }
        for d in &self.definitions {
            if let Some(s) = &d.super_type {
                refs.insert(simple_name(s));
            }
        }
        for c in &self.connections {
            refs.insert(simple_name(&c.source));
            refs.insert(simple_name(&c.target));
        }
        for f in &self.flows {
            refs.insert(simple_name(&f.source));
            refs.insert(simple_name(&f.target));
            if let Some(t) = &f.item_type {
                refs.insert(simple_name(t));
            }
        }
        for s in &self.satisfactions {
            refs.insert(simple_name(&s.requirement));
            if let Some(b) = &s.by {
                refs.insert(simple_name(b));
            }
        }
        for v in &self.verifications {
            refs.insert(simple_name(&v.requirement));
            refs.insert(simple_name(&v.by));
        }
        for a in &self.allocations {
            refs.insert(simple_name(&a.source));
            refs.insert(simple_name(&a.target));
        }
        for tr in &self.type_references {
            refs.insert(simple_name(&tr.name));
        }
        refs
    }
}

/// Extract the simple (unqualified) name from a potentially qualified name.
pub fn simple_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
        .rsplit('.').next().unwrap_or(name)
}
