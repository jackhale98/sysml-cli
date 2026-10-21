//! Golden test: every element of the OMG standard library, as published in
//! `sysml-v2-release/sysml.library.xmi.implied`, must get the `elementId`
//! that our implementation of KerML 9.1 (`path()`, `qualifiedName`,
//! `escapedName()`, UUIDv5) derives. This pins our reading of the identity
//! rules to the normative library before the semantic graph is built on them.
//!
//! What it established (all 404,686 elements, implied relationships
//! included, match):
//!
//! * Ids are derived from the model *with implied relationships*: unnamed
//!   features take their `effectiveName()` through implied redefinitions
//!   (e.g. a calculation's unnamed return parameter is named `result`).
//!   Implied relationships follow the explicit ones in `ownedRelationship`,
//!   so explicit elements keep their positions.
//! * SysML's `namingFeature()` / `effectiveName()` overrides matter:
//!   conjugated port definitions (`~Name`), perform/exhibit/include usages,
//!   constraints owned by a `RequirementConstraintMembership`, and variants.
//! * Kernel libraries use the `https://www.omg.org/spec/KerML/` base URL;
//!   systems and domain libraries use `https://www.omg.org/spec/SysML/`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use sysml_semantic::id::{self, ElementId, OwnershipTree, Spec};
use sysml_semantic::name::{self, OwningNamespace};

/// Reference properties the naming rules need to follow.
const REF_PROPERTIES: &[&str] = &[
    "redefinedFeature",
    "referencedFeature",
    "chainingFeature",
    "originalPortDefinition",
];

#[derive(Debug, Default)]
struct XNode {
    tag: String,
    xsi_type: String,
    element_id: Option<String>,
    declared_name: Option<String>,
    declared_short_name: Option<String>,
    /// Cross-references by property name (`redefinedFeature`,
    /// `referencedFeature`, ...), from attributes or `href` children.
    refs: Vec<(String, String)>,
    parent: Option<usize>,
    children: Vec<usize>,
    /// 1-based position among the parent's element children with the same tag.
    position: usize,
}

#[derive(Default)]
struct Library {
    nodes: Vec<XNode>,
    by_id: HashMap<String, usize>,
    /// (node index of the top-level package, spec, file)
    packages: Vec<(usize, Spec, PathBuf)>,
    qn_cache: RefCell<HashMap<usize, Option<String>>>,
    name_cache: RefCell<HashMap<usize, Option<String>>>,
}

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| {
            a.normalized_value(XmlVersion::default())
                .unwrap()
                .into_owned()
        })
}

impl Library {
    fn load(root: &Path) -> Self {
        let mut files = Vec::new();
        collect(root, &mut files);
        files.sort();
        let mut lib = Library::default();
        for file in files {
            lib.load_file(root, &file);
        }
        lib
    }

    fn load_file(&mut self, root: &Path, file: &Path) {
        let text = std::fs::read_to_string(file).unwrap();
        let mut reader = Reader::from_str(&text);
        let mut stack: Vec<usize> = Vec::new();
        let mut file_root = None;
        loop {
            let (start, empty) = match reader.read_event().unwrap() {
                Event::Start(e) => (e.into_owned(), false),
                Event::Empty(e) => (e.into_owned(), true),
                Event::End(_) => {
                    stack.pop();
                    continue;
                }
                Event::Eof => break,
                _ => continue,
            };
            let tag = start.name().as_ref().to_owned();
            let parent = stack.last().copied();
            if REF_PROPERTIES.contains(&tag.as_str()) {
                if let (Some(p), Some(href)) = (parent, attr(&start, "href")) {
                    let target = href.rsplit('#').next().unwrap().to_owned();
                    self.nodes[p].refs.push((tag.clone(), target));
                }
            }
            let mut node = XNode {
                tag,
                xsi_type: attr(&start, "xsi:type")
                    .map(|t| t.trim_start_matches("sysml:").to_owned())
                    .unwrap_or_default(),
                element_id: attr(&start, "elementId"),
                declared_name: attr(&start, "declaredName"),
                declared_short_name: attr(&start, "declaredShortName"),
                parent,
                ..Default::default()
            };
            for property in REF_PROPERTIES {
                if let Some(targets) = attr(&start, property) {
                    node.refs.extend(
                        targets
                            .split_whitespace()
                            .map(|t| ((*property).to_owned(), t.to_owned())),
                    );
                }
            }
            let index = self.nodes.len();
            if let Some(id) = &node.element_id {
                self.by_id.insert(id.clone(), index);
            }
            self.nodes.push(node);
            if let Some(p) = parent {
                if self.nodes[index].element_id.is_some() {
                    let tag = &self.nodes[index].tag;
                    let position = 1 + self.nodes[p]
                        .children
                        .iter()
                        .filter(|&&c| {
                            self.nodes[c].element_id.is_some() && &self.nodes[c].tag == tag
                        })
                        .count();
                    self.nodes[index].position = position;
                }
                self.nodes[p].children.push(index);
            } else {
                file_root = Some(index);
            }
            if !empty {
                stack.push(index);
            }
        }
        // root Namespace -> OwningMembership -> LibraryPackage
        let root_ns = file_root.expect("empty XMI file");
        let package = self
            .element_children(root_ns, "ownedRelationship")
            .first()
            .and_then(|&m| {
                self.element_children(m, "ownedRelatedElement")
                    .first()
                    .copied()
            })
            .expect("no top-level package");
        let rel = file.strip_prefix(root).unwrap();
        let spec = if rel.starts_with("Kernel Libraries") {
            Spec::KerML
        } else {
            Spec::SysML
        };
        self.packages.push((package, spec, rel.to_owned()));
    }

    fn element_children(&self, node: usize, tag: &str) -> Vec<usize> {
        self.nodes[node]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].tag == tag && self.nodes[c].element_id.is_some())
            .collect()
    }

    fn is_membership(&self, node: usize) -> bool {
        let t = &self.nodes[node].xsi_type;
        t.ends_with("Membership") || t == "FeatureValue"
    }

    fn is_owning_membership(&self, node: usize) -> bool {
        self.is_membership(node) && self.nodes[node].xsi_type != "Membership"
    }

    /// `owningNamespace`: the owner of the membership that owns `node`.
    fn owning_namespace(&self, node: usize) -> Option<usize> {
        let rel = self.owning_relationship(node)?;
        if !self.is_membership(rel) {
            return None;
        }
        self.owning_related_element(rel)
    }

    /// `name` = `effectiveName()`, following `namingFeature()` for unnamed features.
    fn name(&self, node: usize) -> Option<String> {
        if let Some(cached) = self.name_cache.borrow().get(&node) {
            return cached.clone();
        }
        let n = &self.nodes[node];
        let result = if n.xsi_type == "ConjugatedPortDefinition" {
            // SysML 8.3.12.2: '~' + originalPortDefinition.name
            self.owned_relationships_of(node, "PortConjugation")
                .find_map(|r| self.reference(r, "originalPortDefinition"))
                .and_then(|original| self.name(original))
                .map(|original| format!("~{original}"))
        } else if n.declared_name.is_some() || n.declared_short_name.is_some() {
            n.declared_name.clone()
        } else {
            self.naming_feature(node).and_then(|f| self.name(f))
        };
        self.name_cache.borrow_mut().insert(node, result.clone());
        result
    }

    fn short_name(&self, node: usize) -> Option<String> {
        let n = &self.nodes[node];
        if n.declared_name.is_some() || n.declared_short_name.is_some() {
            n.declared_short_name.clone()
        } else {
            self.naming_feature(node).and_then(|f| self.short_name(f))
        }
    }

    fn owned_relationships_of<'a>(
        &'a self,
        node: usize,
        kind: &'a str,
    ) -> impl Iterator<Item = usize> + 'a {
        self.element_children(node, "ownedRelationship")
            .into_iter()
            .filter(move |&r| self.nodes[r].xsi_type == kind)
    }

    fn reference(&self, node: usize, property: &str) -> Option<usize> {
        self.nodes[node]
            .refs
            .iter()
            .find(|(p, _)| p == property)
            .and_then(|(_, id)| self.by_id.get(id).copied())
    }

    /// `ownedReferenceSubsetting.referencedFeature`.
    fn referenced_feature(&self, node: usize) -> Option<usize> {
        self.owned_relationships_of(node, "ReferenceSubsetting")
            .next()
            .and_then(|r| self.reference(r, "referencedFeature"))
    }

    /// `featureTarget`: the last chaining feature of a feature chain, else itself.
    fn feature_target(&self, feature: usize) -> usize {
        self.owned_relationships_of(feature, "FeatureChaining")
            .last()
            .and_then(|r| self.reference(r, "chainingFeature"))
            .unwrap_or(feature)
    }

    /// `namingFeature()` (KerML 8.3.3.3.4) with the SysML overrides.
    fn naming_feature(&self, node: usize) -> Option<usize> {
        let kind = self.nodes[node].xsi_type.as_str();
        let owning_membership = self
            .owning_relationship(node)
            .map(|r| self.nodes[r].xsi_type.as_str());
        // Usage (SysML 8.3.6.4): a variant is named by its referenced feature.
        if owning_membership == Some("VariantMembership") {
            return self.referenced_feature(node);
        }
        // PerformActionUsage (and ExhibitStateUsage, IncludeUseCaseUsage):
        // named by the performedAction when that is not itself.
        if matches!(
            kind,
            "PerformActionUsage" | "ExhibitStateUsage" | "IncludeUseCaseUsage"
        ) {
            if let Some(performed) = self.referenced_feature(node) {
                return Some(performed);
            }
        }
        // ConstraintUsage (and specializations) owned by a
        // RequirementConstraintMembership: the referenced feature's target.
        if owning_membership == Some("RequirementConstraintMembership") {
            if let Some(referenced) = self.referenced_feature(node) {
                return Some(self.feature_target(referenced));
            }
        }
        // Feature (KerML): the first owned redefinition's redefined feature.
        self.owned_relationships_of(node, "Redefinition")
            .next()
            .and_then(|r| self.reference(r, "redefinedFeature"))
    }

    fn escaped_name(&self, node: usize) -> Option<String> {
        self.name(node)
            .or_else(|| self.short_name(node))
            .map(|n| name::escape_name(&n).into_owned())
    }

    fn owned_members(&self, namespace: usize) -> Vec<usize> {
        self.element_children(namespace, "ownedRelationship")
            .into_iter()
            .filter(|&r| self.is_owning_membership(r))
            .filter_map(|r| {
                self.element_children(r, "ownedRelatedElement")
                    .first()
                    .copied()
            })
            .collect()
    }
}

impl OwnershipTree for Library {
    type Node = usize;

    fn owning_relationship(&self, node: usize) -> Option<usize> {
        (self.nodes[node].tag == "ownedRelatedElement")
            .then_some(self.nodes[node].parent)
            .flatten()
    }

    fn owned_related_element_index(&self, node: usize) -> usize {
        self.nodes[node].position
    }

    fn owning_related_element(&self, node: usize) -> Option<usize> {
        (self.nodes[node].tag == "ownedRelationship")
            .then_some(self.nodes[node].parent)
            .flatten()
    }

    fn owned_relationship_index(&self, node: usize) -> usize {
        self.nodes[node].position
    }

    fn is_relationship(&self, node: usize) -> bool {
        self.nodes[node].tag == "ownedRelationship"
    }

    fn owned_member_element(&self, node: usize) -> Option<usize> {
        if !self.is_owning_membership(node) {
            return None;
        }
        self.element_children(node, "ownedRelatedElement")
            .first()
            .copied()
    }

    fn qualified_name(&self, node: usize) -> Option<String> {
        if let Some(cached) = self.qn_cache.borrow().get(&node) {
            return cached.clone();
        }
        let result = match self.owning_namespace(node) {
            None => None,
            Some(ns) => {
                let owner = if self.owning_relationship(ns).is_none() {
                    OwningNamespace::Root
                } else {
                    OwningNamespace::Nested(None)
                };
                let ns_qn = match owner {
                    OwningNamespace::Root => None,
                    _ => self.qualified_name(ns),
                };
                let owner = match owner {
                    OwningNamespace::Root => OwningNamespace::Root,
                    _ => OwningNamespace::Nested(ns_qn.as_deref()),
                };
                let first = match self.name(node) {
                    None => true,
                    Some(n) => {
                        self.owned_members(ns)
                            .into_iter()
                            .find(|&m| self.name(m).as_deref() == Some(n.as_str()))
                            == Some(node)
                    }
                };
                name::qualified_name(owner, self.escaped_name(node).as_deref(), first)
            }
        };
        self.qn_cache.borrow_mut().insert(node, result.clone());
        result
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("sysmlx" | "kermlx")
        ) {
            out.push(path);
        }
    }
}

#[test]
fn every_library_element_id_matches_the_release() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sysml-v2-release/sysml.library.xmi.implied");
    if !root.is_dir() {
        eprintln!(
            "skipping: {} not found (init the sysml-v2-release submodule)",
            root.display()
        );
        return;
    }
    let lib = Library::load(&root);
    let mut checked = 0usize;
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for &(package, spec, ref file) in &lib.packages {
        let package_name = lib.nodes[package].declared_name.clone().unwrap();
        let package_id = ElementId::library_package(spec, &package_name);
        // Every element below the root namespace: the package and its contents.
        let mut stack = vec![lib.nodes[package].parent.unwrap()];
        while let Some(node) = stack.pop() {
            stack.extend(lib.nodes[node].children.iter().copied());
            let Some(actual) = lib.nodes[node].element_id.as_deref() else {
                continue;
            };
            let (expected, path) = if node == package {
                (package_id, format!("<package {package_name}>"))
            } else {
                let path = id::path(&lib, node);
                (ElementId::contained(package_id, &path), path)
            };
            checked += 1;
            if expected.to_string() != actual {
                let entry = failures
                    .entry(lib.nodes[node].xsi_type.clone())
                    .or_default();
                if entry.len() < 3 {
                    entry.push(format!(
                        "{}: path {path:?} (expected {actual})",
                        file.display()
                    ));
                } else {
                    entry.push(String::new());
                }
            }
        }
    }

    let failed: usize = failures.values().map(Vec::len).sum();
    if failed > 0 {
        let mut report = format!("{failed} of {checked} library element ids differ:\n");
        for (kind, examples) in &failures {
            report.push_str(&format!("  {kind} ({}):\n", examples.len()));
            for example in examples.iter().filter(|e| !e.is_empty()) {
                report.push_str(&format!("    {example}\n"));
            }
        }
        panic!("{report}");
    }
    assert!(checked > 100_000, "only {checked} elements checked");
    eprintln!("verified {checked} library element ids");
}
