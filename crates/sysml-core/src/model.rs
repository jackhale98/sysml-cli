/// Model types extracted from tree-sitter parse tree.
///
/// These represent the structural elements of a SysML v2 model
/// in a form suitable for running validation checks.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
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

    /// Returns true if `other` is fully contained within this span.
    pub fn contains(&self, other: &Span) -> bool {
        self.start_byte <= other.start_byte && other.end_byte <= self.end_byte
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
    /// Visibility modifier (public/private/protected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    /// Short name alias (e.g., `<V>` in `part def Vehicle <V>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    /// Documentation comment text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Whether the definition is abstract.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_abstract: bool,
    /// Members of an enum definition.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enum_members: Vec<EnumMember>,
    /// Name of the enclosing definition, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_def: Option<String>,
    /// Byte offset of the opening `{` of the definition body.
    #[serde(skip)]
    pub body_start_byte: Option<usize>,
    /// Byte offset of the closing `}` of the definition body.
    #[serde(skip)]
    pub body_end_byte: Option<usize>,
    /// Fully qualified name (populated by qualify_model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<crate::qualified_name::QualifiedName>,
}

/// A member of an enum definition.
#[derive(Debug, Clone, Serialize)]
pub struct EnumMember {
    pub name: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

impl Visibility {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Protected => "protected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Multiplicity {
    pub lower: Option<String>,
    pub upper: Option<String>,
    pub is_ordered: bool,
    pub is_nonunique: bool,
}

impl std::fmt::Display for Multiplicity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        match (&self.lower, &self.upper) {
            (Some(lo), Some(hi)) => write!(f, "{}..{}", lo, hi)?,
            (Some(lo), None) => write!(f, "{}", lo)?,
            (None, Some(hi)) => write!(f, "{}", hi)?,
            (None, None) => write!(f, "*")?,
        }
        write!(f, "]")?;
        if self.is_ordered {
            write!(f, " ordered")?;
        }
        if self.is_nonunique {
            write!(f, " nonunique")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
    InOut,
}

impl Direction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::InOut => "inout",
        }
    }

    /// Apply conjugation: flips In↔Out, InOut stays.
    pub fn conjugated(self) -> Self {
        match self {
            Self::In => Self::Out,
            Self::Out => Self::In,
            Self::InOut => self,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub kind: String,
    pub name: String,
    pub type_ref: Option<String>,
    pub span: Span,
    /// Direction modifier (in/out/inout), if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Whether the type reference is conjugated (`~`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_conjugated: bool,
    /// Name of the enclosing definition, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_def: Option<String>,
    /// Multiplicity (e.g., [0..1], [1..*]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplicity: Option<Multiplicity>,
    /// Default value expression text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_expr: Option<String>,
    /// Short name alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    /// Redefines target name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redefinition: Option<String>,
    /// Subsets target name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsets: Option<String>,
    /// Fully qualified name (populated by qualify_model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<crate::qualified_name::QualifiedName>,
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

/// A doc comment (e.g., `doc /* description */`).
#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub text: String,
    /// Locale if specified (e.g., `doc locale "en" /* ... */`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Name of the enclosing definition, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_def: Option<String>,
    pub span: Span,
}

/// An import statement (e.g., `import Vehicles::*;`).
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub path: String,
    pub is_wildcard: bool,
    pub is_recursive: bool,
    pub span: Span,
}

/// A parsed view definition with its expose and filter clauses.
#[derive(Debug, Clone, Serialize)]
pub struct ViewDef {
    pub name: String,
    /// Exposed qualified names (e.g., "Vehicle::*", "P::**").
    pub exposes: Vec<String>,
    /// Kind filters extracted from filter statements (e.g., "part", "port").
    pub kind_filters: Vec<String>,
    /// Render clause value (e.g., "asInterconnectionDiagram", "asTableDiagram").
    pub render_as: Option<String>,
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
    pub imports: Vec<Import>,
    pub comments: Vec<Comment>,
    pub views: Vec<ViewDef>,
    /// Names resolved from imports (populated by the resolver).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resolved_imports: Vec<String>,
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
            imports: Vec::new(),
            comments: Vec::new(),
            views: Vec::new(),
            resolved_imports: Vec::new(),
        }
    }

    /// All defined names in this model.
    pub fn defined_names(&self) -> std::collections::HashSet<&str> {
        self.definitions.iter().map(|d| d.name.as_str()).collect()
    }

    /// Find a definition by name.
    pub fn find_def(&self, name: &str) -> Option<&Definition> {
        self.definitions.iter().find(|d| d.name == name)
    }

    /// Get all usages with a specific parent_def.
    pub fn usages_in_def(&self, def_name: &str) -> Vec<&Usage> {
        self.usages
            .iter()
            .filter(|u| u.parent_def.as_deref() == Some(def_name))
            .collect()
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

/// Populate `qualified_name` fields on all definitions and usages by walking
/// the `parent_def` chains. Call this after parsing to enrich the model.
pub fn qualify_model(model: &mut Model) {
    use crate::qualified_name::QualifiedName;
    use std::collections::HashMap;

    // Build a map of definition name -> parent chain for fast lookup
    let mut def_parents: HashMap<String, Option<String>> = HashMap::new();
    for d in &model.definitions {
        def_parents.insert(d.name.clone(), d.parent_def.clone());
    }

    // Reconstruct qualified name by walking parent chain
    let build_qn = |name: &str, parent: &Option<String>| -> QualifiedName {
        let mut segments = Vec::new();
        if let Some(p) = parent {
            // Walk up the parent chain
            let mut chain = vec![p.clone()];
            let mut current = p.clone();
            while let Some(Some(grandparent)) = def_parents.get(&current) {
                chain.push(grandparent.clone());
                current = grandparent.clone();
            }
            chain.reverse();
            segments.extend(chain);
        }
        segments.push(name.to_string());
        QualifiedName::new(segments)
    };

    // Apply to definitions
    for d in &mut model.definitions {
        d.qualified_name = Some(build_qn(&d.name, &d.parent_def));
    }

    // Apply to usages
    for u in &mut model.usages {
        u.qualified_name = Some(build_qn(&u.name, &u.parent_def));
    }
}
