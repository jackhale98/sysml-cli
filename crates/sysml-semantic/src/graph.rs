//! The element graph: an arena of elements and relationships mirroring the
//! KerML/SysML abstract syntax.
//!
//! Every element — including every relationship, which in KerML is itself an
//! element — lives in one arena and is addressed by an [`ElementRef`].
//! Ownership follows the abstract syntax exactly: elements own relationships
//! (`ownedRelationship`) and relationships own elements
//! (`ownedRelatedElement`); both are kept, in order, in
//! [`Element::owned`]. Non-owning relationship ends (the type of a
//! `FeatureTyping`, the general type of a `Subclassification`, ...) are
//! [`End`]s whose [`Target`] starts as an unresolved [`Reference`] (the
//! qualified name as written) and is resolved by [`crate::resolve`].
//!
//! A model is layered: the standard library graph is built once and shared,
//! and a project graph continues its index space (see [`Graph::base`]), so
//! a project element can point at a library element directly.

use std::fmt;

use crate::metaclass::Metaclass;

/// Index of an element in a model's arena (library and project share one
/// index space; library elements come first).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementRef(pub(crate) u32);

impl ElementRef {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Index of a written reference (a qualified name in the text).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct RefId(pub(crate) u32);

impl RefId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index of a source file in a model (library files come first).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FileId(pub(crate) u32);

impl FileId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A byte range in a source file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Span {
    pub file: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn file(&self) -> FileId {
        FileId(self.file)
    }
    pub fn contains(&self, offset: usize) -> bool {
        (self.start as usize) <= offset && offset <= (self.end as usize)
    }
    pub fn len(&self) -> u32 {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// `VisibilityKind`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

impl Visibility {
    pub fn name(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }
}

/// `FeatureDirectionKind`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    In,
    Out,
    InOut,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::InOut => "inout",
        }
    }
    /// The direction as seen through a conjugation.
    pub fn conjugated(self) -> Direction {
        match self {
            Direction::In => Direction::Out,
            Direction::Out => Direction::In,
            Direction::InOut => Direction::InOut,
        }
    }
}

/// Boolean properties of an element, as set by the textual notation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Flags(u32);

macro_rules! flags {
    ($($name:ident = $bit:expr, $doc:literal;)*) => {
        impl Flags {
            $(#[doc = $doc] pub const $name: Flags = Flags(1 << $bit);)*
            const NAMES: &'static [(Flags, &'static str)] = &[$((Flags::$name, stringify!($name))),*];
        }
    };
}

flags! {
    ABSTRACT = 0, "`isAbstract`";
    VARIATION = 1, "`isVariation` (SysML)";
    DERIVED = 2, "`isDerived`";
    END = 3, "`isEnd`";
    CONSTANT = 4, "`isConstant`";
    ORDERED = 5, "`isOrdered`";
    NONUNIQUE = 6, "`isUnique = false`";
    COMPOSITE = 7, "`isComposite` written explicitly (KerML `composite`)";
    REFERENCE = 8, "`isReference` written explicitly (SysML `ref`)";
    PORTION = 9, "`isPortion`";
    VARIABLE = 10, "`isVariable` (KerML `var`)";
    INDIVIDUAL = 11, "`isIndividual` (SysML)";
    SUFFICIENT = 12, "`isSufficient` (`all`)";
    STANDARD = 13, "`isStandard` (standard library package)";
    NEGATED = 14, "`isNegated`";
    PARALLEL = 15, "`isParallel` (SysML states)";
    DEFAULT = 16, "`FeatureValue::isDefault`";
    INITIAL = 17, "`FeatureValue::isInitial`";
    RECURSIVE = 18, "`Import::isRecursive`";
    IMPORT_ALL = 19, "`Import::isImportAll`";
    IMPLIED = 20, "`Relationship::isImplied` (added by the semantic model)";
    VISIBILITY_WRITTEN = 21, "visibility was written explicitly";
    VARIANT = 22, "declared with `variant` (SysML)";
}

impl Flags {
    pub const NONE: Flags = Flags(0);

    pub fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }
    pub fn insert(&mut self, other: Flags) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Flags) {
        self.0 &= !other.0;
    }
    pub fn set(&mut self, other: Flags, on: bool) {
        if on {
            self.insert(other)
        } else {
            self.remove(other)
        }
    }
}

impl std::ops::BitOr for Flags {
    type Output = Flags;
    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

impl fmt::Debug for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Flags::NAMES
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, name)| *name)
            .collect();
        write!(f, "{}", names.join("|"))
    }
}

/// `PortionKind`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PortionKind {
    Snapshot,
    Timeslice,
}

/// Metaclass-specific attribute values.
#[derive(Clone, PartialEq, Debug, Default)]
pub enum Value {
    #[default]
    None,
    /// `LiteralBoolean::value`.
    Bool(bool),
    /// `LiteralInteger::value` (decimal text, unbounded).
    Integer(Box<str>),
    /// `LiteralRational::value` (as written).
    Rational(Box<str>),
    /// `LiteralString::value` (unescaped).
    String(Box<str>),
    /// `OperatorExpression::operator`.
    Operator(Box<str>),
    /// `Comment` / `Documentation` / `TextualRepresentation`: the processed
    /// body, plus `locale` or `language`.
    Body {
        body: Box<str>,
        locale: Option<Box<str>>,
        language: Option<Box<str>>,
    },
    /// The `kind` of a `RequirementConstraintMembership`
    /// (`assumption`/`requirement`), `StateSubactionMembership`
    /// (`entry`/`do`/`exit`), `TransitionFeatureMembership`
    /// (`trigger`/`guard`/`effect`), `TriggerInvocationExpression`
    /// (`when`/`at`/`after`) or `RequirementVerificationMembership`.
    Kind(&'static str),
    /// `portionKind` of a SysML occurrence usage.
    Portion(PortionKind),
}

/// The role of a non-owning relationship end.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// The `source` side when it is not the owning element (standalone
    /// relationships: `specific`, `typedFeature`, `conjugatedType`,
    /// `client`, `featureOfType`, ...).
    Source,
    /// The `target` side: `general`, `type`, `superclassifier`,
    /// `subsettedFeature`, `redefinedFeature`, `memberElement`,
    /// `importedMembership`, `importedNamespace`, `chainingFeature`,
    /// `annotatedElement`, `supplier`, ...
    Target,
}

/// What a relationship end points at.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Target {
    /// A qualified name written in the text, resolved on demand.
    Name(RefId),
    /// An element owned by the relationship itself (a feature chain
    /// written as the target, `ownedRelatedElement`).
    Owned(ElementRef),
    /// A direct reference (implied relationships).
    Element(ElementRef),
}

/// A non-owning end of a relationship.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct End {
    pub role: Role,
    pub target: Target,
}

/// A qualified name as written in the text.
#[derive(Clone, PartialEq, Debug)]
pub struct Reference {
    /// Segment names, unescaped.
    pub segments: Vec<Box<str>>,
    /// Byte range of each segment (including quotes).
    pub segment_spans: Vec<(u32, u32)>,
    /// Written with the global scope qualifier `$::`.
    pub global: bool,
    /// The whole name.
    pub span: Span,
    /// The relationship (or expression membership) this name is a target of.
    pub owner: ElementRef,
}

impl Reference {
    /// The name as written, with `::` separators.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if self.global {
            out.push_str("$::");
        }
        for (i, s) in self.segments.iter().enumerate() {
            if i > 0 {
                out.push_str("::");
            }
            out.push_str(&crate::name::escape_name(s));
        }
        out
    }
}

/// One element of the abstract syntax (relationships included).
#[derive(Clone, Debug)]
pub struct Element {
    pub kind: Metaclass,
    /// `owningRelationship` (for a non-relationship element) or
    /// `owningRelatedElement` (for a relationship).
    pub owner: Option<ElementRef>,
    /// `ownedRelationship` (for a non-relationship element) or
    /// `ownedRelatedElement` (for a relationship), in order. Implied
    /// relationships follow the explicit ones.
    pub owned: Vec<ElementRef>,
    /// `declaredName` (`memberName` for an alias `Membership`).
    pub name: Option<Box<str>>,
    /// `declaredShortName` (`memberShortName` for an alias `Membership`).
    pub short_name: Option<Box<str>>,
    pub flags: Flags,
    /// For memberships and imports.
    pub visibility: Visibility,
    pub direction: Option<Direction>,
    /// Non-owning relationship ends.
    pub ends: Vec<End>,
    pub value: Value,
    /// The source text the element was lowered from. Implied elements
    /// carry the span of the element that implies them.
    pub span: Span,
    /// The span of the declared name (or short name), if any.
    pub name_span: Option<(u32, u32)>,
}

impl Element {
    pub fn new(kind: Metaclass, span: Span) -> Self {
        Element {
            kind,
            owner: None,
            owned: Vec::new(),
            name: None,
            short_name: None,
            flags: Flags::NONE,
            visibility: Visibility::Public,
            direction: None,
            ends: Vec::new(),
            value: Value::None,
            span,
            name_span: None,
        }
    }

    pub fn is_implied(&self) -> bool {
        self.flags.contains(Flags::IMPLIED)
    }

    /// The first end with the given role.
    pub fn end(&self, role: Role) -> Option<Target> {
        self.ends.iter().find(|e| e.role == role).map(|e| e.target)
    }
}

/// A source file of a graph.
#[derive(Clone, Debug)]
pub struct File {
    pub path: Box<str>,
    /// The root `Namespace` lowered from the file.
    pub root: ElementRef,
    /// Whether the file is KerML (`.kerml`) rather than SysML.
    pub kerml: bool,
}

/// One layer of elements: either the standard library or a project.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// Index of the first element of this layer in the model's index space.
    pub(crate) base: u32,
    /// Index of the first reference of this layer.
    pub(crate) ref_base: u32,
    /// Index of the first file of this layer.
    pub(crate) file_base: u32,
    pub(crate) elements: Vec<Element>,
    pub(crate) refs: Vec<Reference>,
    pub(crate) files: Vec<File>,
}

impl Graph {
    /// An empty layer continuing after `below` (or starting at zero).
    pub fn above(below: Option<&Graph>) -> Graph {
        match below {
            None => Graph::default(),
            Some(g) => Graph {
                base: g.base + g.elements.len() as u32,
                ref_base: g.ref_base + g.refs.len() as u32,
                file_base: g.file_base + g.files.len() as u32,
                ..Graph::default()
            },
        }
    }

    pub fn contains(&self, e: ElementRef) -> bool {
        e.0 >= self.base && ((e.0 - self.base) as usize) < self.elements.len()
    }

    pub fn contains_ref(&self, r: RefId) -> bool {
        r.0 >= self.ref_base && ((r.0 - self.ref_base) as usize) < self.refs.len()
    }

    pub fn get(&self, e: ElementRef) -> &Element {
        &self.elements[(e.0 - self.base) as usize]
    }

    pub fn get_mut(&mut self, e: ElementRef) -> &mut Element {
        &mut self.elements[(e.0 - self.base) as usize]
    }

    pub fn reference(&self, r: RefId) -> &Reference {
        &self.refs[(r.0 - self.ref_base) as usize]
    }

    pub fn file(&self, f: FileId) -> &File {
        &self.files[(f.0 - self.file_base) as usize]
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// All element refs of this layer.
    pub fn refs_iter(&self) -> impl Iterator<Item = ElementRef> {
        (self.base..self.base + self.elements.len() as u32).map(ElementRef)
    }

    /// All reference ids of this layer.
    pub fn ref_ids(&self) -> impl Iterator<Item = RefId> {
        (self.ref_base..self.ref_base + self.refs.len() as u32).map(RefId)
    }

    /// All file ids of this layer.
    pub fn file_ids(&self) -> impl Iterator<Item = FileId> {
        (self.file_base..self.file_base + self.files.len() as u32).map(FileId)
    }

    /// Add an element (unowned).
    pub(crate) fn push(&mut self, element: Element) -> ElementRef {
        let r = ElementRef(self.base + self.elements.len() as u32);
        self.elements.push(element);
        r
    }

    /// Append `child` to `parent`'s owned list.
    pub(crate) fn attach(&mut self, parent: ElementRef, child: ElementRef) {
        self.get_mut(child).owner = Some(parent);
        self.get_mut(parent).owned.push(child);
    }

    pub(crate) fn add_ref(&mut self, reference: Reference) -> RefId {
        let r = RefId(self.ref_base + self.refs.len() as u32);
        self.refs.push(reference);
        r
    }

    pub(crate) fn add_file(&mut self, file: File) -> FileId {
        let f = FileId(self.file_base + self.files.len() as u32);
        self.files.push(file);
        f
    }

    /// The next file id this graph would assign.
    pub(crate) fn next_file_id(&self) -> FileId {
        FileId(self.file_base + self.files.len() as u32)
    }
}
