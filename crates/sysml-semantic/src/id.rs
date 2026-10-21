//! Element identity (KerML 1.0, 9.1 and 8.3.2.1).
//!
//! Standard library elements have normative, name-based (version 5, SHA-1)
//! UUIDs:
//!
//! * a top-level library package's id is `uuid5(NameSpace_URL, base + name)`,
//!   where `base` is `https://www.omg.org/spec/KerML/` or
//!   `https://www.omg.org/spec/SysML/` (see [`Spec`]);
//! * every element contained in it has `uuid5(package id, path())`.
//!
//! The spec allows tools to use the same scheme for user models provided
//! each top-level element gets a unique URL; [`ElementId::top_level`] does
//! that from a project URI.
//!
//! [`path`] implements `Element::path()`, `Relationship::path()` and
//! `OwningMembership::path()` over any [`OwnershipTree`].

use std::fmt;

use uuid::Uuid;

/// The specification family that owns a standard library package, which
/// determines the URL its id is derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Spec {
    /// Kernel libraries (`https://www.omg.org/spec/KerML/<package>`).
    KerML,
    /// Systems and domain libraries (`https://www.omg.org/spec/SysML/<package>`).
    SysML,
}

impl Spec {
    /// URL prefix that, with the package name appended, names a library package.
    pub fn base_url(self) -> &'static str {
        match self {
            Spec::KerML => "https://www.omg.org/spec/KerML/",
            Spec::SysML => "https://www.omg.org/spec/SysML/",
        }
    }
}

/// An element's `elementId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(Uuid);

impl ElementId {
    /// Id of a top-level standard library package.
    pub fn library_package(spec: Spec, package_name: &str) -> Self {
        Self::top_level(spec.base_url(), package_name)
    }

    /// Id of a top-level element named `name` in a model identified by
    /// `base_uri` (which should end with `/`): `uuid5(NameSpace_URL, base_uri + name)`.
    pub fn top_level(base_uri: &str, name: &str) -> Self {
        let url = format!("{base_uri}{name}");
        ElementId(Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes()))
    }

    /// Id of an element contained in the top-level element `namespace`,
    /// given its [`path`].
    pub fn contained(namespace: ElementId, path: &str) -> Self {
        ElementId(Uuid::new_v5(&namespace.0, path.as_bytes()))
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn parse(text: &str) -> Option<Self> {
        Uuid::parse_str(text).ok().map(ElementId)
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// The containment structure `path()` is defined over.
///
/// Elements own relationships (`ownedRelationship`, ordered) and
/// relationships own elements (`ownedRelatedElement`, ordered). Indexes are
/// 1-based positions in those ordered lists, as in the spec.
pub trait OwnershipTree {
    type Node: Copy;

    /// The element's `owningRelationship`, if any.
    fn owning_relationship(&self, node: Self::Node) -> Option<Self::Node>;
    /// 1-based position of `node` in its owning relationship's `ownedRelatedElement`.
    fn owned_related_element_index(&self, node: Self::Node) -> usize;
    /// For a relationship: its `owningRelatedElement`, if any.
    fn owning_related_element(&self, node: Self::Node) -> Option<Self::Node>;
    /// For a relationship: 1-based position in its owner's `ownedRelationship`.
    fn owned_relationship_index(&self, node: Self::Node) -> usize;
    fn is_relationship(&self, node: Self::Node) -> bool;
    /// For an `OwningMembership` (or any specialization): its `ownedMemberElement`.
    /// `None` for every other node.
    fn owned_member_element(&self, node: Self::Node) -> Option<Self::Node>;
    /// The element's derived `qualifiedName`.
    fn qualified_name(&self, node: Self::Node) -> Option<String>;
}

/// `path()` of `node`.
pub fn path<T: OwnershipTree>(tree: &T, node: T::Node) -> String {
    // OwningMembership::path() (8.3.2.4.8).
    if let Some(member) = tree.owned_member_element(node) {
        if let Some(qn) = tree.qualified_name(member) {
            return format!("{qn}/owningMembership");
        }
    }
    // Relationship::path() (8.3.2.1.3).
    if tree.is_relationship(node) && tree.owning_relationship(node).is_none() {
        if let Some(owner) = tree.owning_related_element(node) {
            return format!(
                "{}/{}",
                path(tree, owner),
                tree.owned_relationship_index(node)
            );
        }
    }
    // Element::path() (8.3.2.1.2).
    if let Some(qn) = tree.qualified_name(node) {
        return qn;
    }
    match tree.owning_relationship(node) {
        Some(rel) => format!(
            "{}/{}",
            path(tree, rel),
            tree.owned_related_element_index(node)
        ),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_package_ids_match_the_release() {
        // Values from sysml.library.xmi in the OMG SysML v2 release.
        let parts = ElementId::library_package(Spec::SysML, "Parts");
        assert_eq!(parts.to_string(), "6890ef7c-3613-5738-88c2-483f356d3ee5");
        assert_eq!(
            ElementId::contained(parts, "Parts/owningMembership").to_string(),
            "620595d0-953a-58eb-a0e6-1ec5ced81c7a"
        );
        assert_eq!(
            ElementId::contained(parts, "Parts/1/1").to_string(),
            "2b574b4a-b51c-560b-bf14-33cf613dc44c"
        );
    }

    #[test]
    fn parse_round_trips() {
        let id = ElementId::library_package(Spec::KerML, "Base");
        assert_eq!(ElementId::parse(&id.to_string()), Some(id));
        assert_eq!(ElementId::parse("not a uuid"), None);
    }
}
