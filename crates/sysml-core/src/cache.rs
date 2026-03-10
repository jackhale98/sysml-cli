/// Performance index cache for SysML models.
///
/// This is never the source of truth -- always rebuildable from model files
/// and records.  Designed to be gitignored (`.sysml/cache.db`).
///
/// The current implementation uses in-memory data structures.  A future
/// version will swap in an SQLite backend via `rusqlite` without changing
/// the public API.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A model element (definition or usage) stored in the cache.
#[derive(Debug, Clone, Serialize)]
pub struct CacheNode {
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub parent: Option<String>,
}

/// A relationship between two model elements.
#[derive(Debug, Clone, Serialize)]
pub struct CacheEdge {
    pub source: String,
    pub target: String,
    /// Relationship kind: `"specializes"`, `"satisfies"`, `"verifies"`,
    /// `"allocates"`, `"connects"`, `"flows"`, etc.
    pub kind: String,
}

/// A TOML record (review, decision, change request, ...).
#[derive(Debug, Clone, Serialize)]
pub struct CacheRecord {
    pub id: String,
    pub tool: String,
    pub record_type: String,
    pub created: String,
    pub author: String,
    pub file: String,
}

/// A reference edge linking a record to a model element.
#[derive(Debug, Clone, Serialize)]
pub struct CacheRefEdge {
    pub record_id: String,
    pub qualified_name: String,
    pub ref_kind: String,
}

/// Summary statistics returned by [`Cache::stats`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CacheStats {
    pub nodes: usize,
    pub edges: usize,
    pub records: usize,
    pub ref_edges: usize,
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// In-memory model cache.
///
/// All query methods return borrowed references so callers do not need to
/// clone unless they want ownership.
#[derive(Debug, Default)]
pub struct Cache {
    nodes: Vec<CacheNode>,
    edges: Vec<CacheEdge>,
    records: Vec<CacheRecord>,
    ref_edges: Vec<CacheRefEdge>,
    git_head: Option<String>,
}

impl Cache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    // -- mutators -----------------------------------------------------------

    /// Insert a model node.
    pub fn add_node(&mut self, node: CacheNode) {
        self.nodes.push(node);
    }

    /// Insert a relationship edge.
    pub fn add_edge(&mut self, edge: CacheEdge) {
        self.edges.push(edge);
    }

    /// Insert a record (from a TOML file).
    pub fn add_record(&mut self, record: CacheRecord) {
        self.records.push(record);
    }

    /// Insert a reference edge linking a record to a model element.
    pub fn add_ref_edge(&mut self, ref_edge: CacheRefEdge) {
        self.ref_edges.push(ref_edge);
    }

    // -- node queries -------------------------------------------------------

    /// Return all nodes whose `kind` matches exactly.
    pub fn find_nodes_by_kind<'a>(&'a self, kind: &str) -> Vec<&'a CacheNode> {
        self.nodes.iter().filter(|n| n.kind == kind).collect()
    }

    /// Return the first node whose qualified name matches exactly.
    pub fn find_node<'a>(&'a self, qualified_name: &str) -> Option<&'a CacheNode> {
        self.nodes.iter().find(|n| n.qualified_name == qualified_name)
    }

    // -- edge queries -------------------------------------------------------

    /// Return all edges originating from `source`.
    pub fn find_edges_from<'a>(&'a self, source: &str) -> Vec<&'a CacheEdge> {
        self.edges.iter().filter(|e| e.source == source).collect()
    }

    /// Return all edges pointing to `target`.
    pub fn find_edges_to<'a>(&'a self, target: &str) -> Vec<&'a CacheEdge> {
        self.edges.iter().filter(|e| e.target == target).collect()
    }

    // -- record queries -----------------------------------------------------

    /// Return all records produced by a given tool.
    pub fn find_records_by_tool<'a>(&'a self, tool: &str) -> Vec<&'a CacheRecord> {
        self.records.iter().filter(|r| r.tool == tool).collect()
    }

    /// Return all reference edges for a given record id.
    pub fn find_refs_for_record<'a>(&'a self, record_id: &str) -> Vec<&'a CacheRefEdge> {
        self.ref_edges
            .iter()
            .filter(|re| re.record_id == record_id)
            .collect()
    }

    /// Return all records that reference a particular model element (by
    /// qualified name), joined through `ref_edges`.
    pub fn find_records_referencing<'a>(&'a self, qualified_name: &str) -> Vec<&'a CacheRecord> {
        let record_ids: Vec<&str> = self
            .ref_edges
            .iter()
            .filter(|re| re.qualified_name == qualified_name)
            .map(|re| re.record_id.as_str())
            .collect();

        self.records
            .iter()
            .filter(|r| record_ids.contains(&r.id.as_str()))
            .collect()
    }

    // -- bulk access (for SQLite transfer) ------------------------------------

    /// Return all nodes in the cache.
    pub fn all_nodes(&self) -> &[CacheNode] {
        &self.nodes
    }

    /// Return all edges in the cache.
    pub fn all_edges(&self) -> &[CacheEdge] {
        &self.edges
    }

    /// Return all records in the cache.
    pub fn all_records(&self) -> &[CacheRecord] {
        &self.records
    }

    /// Return all reference edges in the cache.
    pub fn all_ref_edges(&self) -> &[CacheRefEdge] {
        &self.ref_edges
    }

    // -- stats & git --------------------------------------------------------

    /// Return summary counts of cached data.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            nodes: self.nodes.len(),
            edges: self.edges.len(),
            records: self.records.len(),
            ref_edges: self.ref_edges.len(),
        }
    }

    /// Store the current git HEAD hash so we can detect staleness.
    pub fn set_git_head(&mut self, hash: &str) {
        self.git_head = Some(hash.to_string());
    }

    /// Return the stored git HEAD hash, if any.
    pub fn git_head(&self) -> Option<&str> {
        self.git_head.as_deref()
    }

    /// Returns `true` when the stored HEAD differs from `current_head`,
    /// meaning the cache may be out of date.
    pub fn is_stale(&self, current_head: &str) -> bool {
        match &self.git_head {
            Some(stored) => stored != current_head,
            None => true,
        }
    }

    /// Drop all cached data (nodes, edges, records, ref edges, and git HEAD).
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.records.clear();
        self.ref_edges.clear();
        self.git_head = None;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cache() -> Cache {
        let mut cache = Cache::new();

        cache.add_node(CacheNode {
            qualified_name: "Vehicle".into(),
            kind: "part def".into(),
            file: "vehicle.sysml".into(),
            line: 1,
            parent: None,
        });
        cache.add_node(CacheNode {
            qualified_name: "Vehicle::engine".into(),
            kind: "part".into(),
            file: "vehicle.sysml".into(),
            line: 5,
            parent: Some("Vehicle".into()),
        });
        cache.add_node(CacheNode {
            qualified_name: "Engine".into(),
            kind: "part def".into(),
            file: "vehicle.sysml".into(),
            line: 10,
            parent: None,
        });
        cache.add_node(CacheNode {
            qualified_name: "MassReq".into(),
            kind: "requirement def".into(),
            file: "reqs.sysml".into(),
            line: 1,
            parent: None,
        });

        cache.add_edge(CacheEdge {
            source: "Engine".into(),
            target: "PowerSource".into(),
            kind: "specializes".into(),
        });
        cache.add_edge(CacheEdge {
            source: "Vehicle".into(),
            target: "MassReq".into(),
            kind: "satisfies".into(),
        });

        cache.add_record(CacheRecord {
            id: "rev-001".into(),
            tool: "review".into(),
            record_type: "design-review".into(),
            created: "2025-01-15".into(),
            author: "alice".into(),
            file: "records/rev-001.toml".into(),
        });
        cache.add_record(CacheRecord {
            id: "dec-001".into(),
            tool: "decision".into(),
            record_type: "architecture-decision".into(),
            created: "2025-01-20".into(),
            author: "bob".into(),
            file: "records/dec-001.toml".into(),
        });

        cache.add_ref_edge(CacheRefEdge {
            record_id: "rev-001".into(),
            qualified_name: "Vehicle".into(),
            ref_kind: "reviews".into(),
        });
        cache.add_ref_edge(CacheRefEdge {
            record_id: "rev-001".into(),
            qualified_name: "Engine".into(),
            ref_kind: "reviews".into(),
        });
        cache.add_ref_edge(CacheRefEdge {
            record_id: "dec-001".into(),
            qualified_name: "Vehicle".into(),
            ref_kind: "decides".into(),
        });

        cache
    }

    #[test]
    fn new_cache_is_empty() {
        let cache = Cache::new();
        assert_eq!(
            cache.stats(),
            CacheStats {
                nodes: 0,
                edges: 0,
                records: 0,
                ref_edges: 0,
            }
        );
    }

    #[test]
    fn stats_reflect_inserted_data() {
        let cache = sample_cache();
        assert_eq!(
            cache.stats(),
            CacheStats {
                nodes: 4,
                edges: 2,
                records: 2,
                ref_edges: 3,
            }
        );
    }

    #[test]
    fn find_node_by_qualified_name() {
        let cache = sample_cache();
        let node = cache.find_node("Vehicle::engine");
        assert!(node.is_some());
        let node = node.unwrap();
        assert_eq!(node.kind, "part");
        assert_eq!(node.parent.as_deref(), Some("Vehicle"));
    }

    #[test]
    fn find_node_returns_none_for_missing() {
        let cache = sample_cache();
        assert!(cache.find_node("DoesNotExist").is_none());
    }

    #[test]
    fn find_nodes_by_kind() {
        let cache = sample_cache();
        let part_defs = cache.find_nodes_by_kind("part def");
        assert_eq!(part_defs.len(), 2);
        let names: Vec<&str> = part_defs.iter().map(|n| n.qualified_name.as_str()).collect();
        assert!(names.contains(&"Vehicle"));
        assert!(names.contains(&"Engine"));
    }

    #[test]
    fn find_edges_from_source() {
        let cache = sample_cache();
        let edges = cache.find_edges_from("Engine");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "PowerSource");
        assert_eq!(edges[0].kind, "specializes");
    }

    #[test]
    fn find_edges_to_target() {
        let cache = sample_cache();
        let edges = cache.find_edges_to("MassReq");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "Vehicle");
    }

    #[test]
    fn find_edges_empty_when_no_match() {
        let cache = sample_cache();
        assert!(cache.find_edges_from("NoSuchNode").is_empty());
        assert!(cache.find_edges_to("NoSuchNode").is_empty());
    }

    #[test]
    fn find_records_by_tool() {
        let cache = sample_cache();
        let reviews = cache.find_records_by_tool("review");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].id, "rev-001");

        let decisions = cache.find_records_by_tool("decision");
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, "dec-001");

        assert!(cache.find_records_by_tool("nonexistent").is_empty());
    }

    #[test]
    fn find_refs_for_record() {
        let cache = sample_cache();
        let refs = cache.find_refs_for_record("rev-001");
        assert_eq!(refs.len(), 2);
        let names: Vec<&str> = refs.iter().map(|r| r.qualified_name.as_str()).collect();
        assert!(names.contains(&"Vehicle"));
        assert!(names.contains(&"Engine"));
    }

    #[test]
    fn find_records_referencing_element() {
        let cache = sample_cache();

        // Vehicle is referenced by both rev-001 and dec-001
        let records = cache.find_records_referencing("Vehicle");
        assert_eq!(records.len(), 2);
        let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"rev-001"));
        assert!(ids.contains(&"dec-001"));

        // Engine is referenced only by rev-001
        let records = cache.find_records_referencing("Engine");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "rev-001");

        // MassReq is not referenced by any record
        assert!(cache.find_records_referencing("MassReq").is_empty());
    }

    #[test]
    fn git_head_lifecycle() {
        let mut cache = Cache::new();

        // Initially no HEAD stored
        assert!(cache.git_head().is_none());
        assert!(cache.is_stale("abc123"));

        // Store a HEAD
        cache.set_git_head("abc123");
        assert_eq!(cache.git_head(), Some("abc123"));
        assert!(!cache.is_stale("abc123"));

        // Different HEAD means stale
        assert!(cache.is_stale("def456"));

        // Update HEAD
        cache.set_git_head("def456");
        assert!(!cache.is_stale("def456"));
    }

    #[test]
    fn clear_resets_everything() {
        let mut cache = sample_cache();
        cache.set_git_head("abc123");

        assert!(cache.stats().nodes > 0);
        assert!(cache.git_head().is_some());

        cache.clear();

        assert_eq!(
            cache.stats(),
            CacheStats {
                nodes: 0,
                edges: 0,
                records: 0,
                ref_edges: 0,
            }
        );
        assert!(cache.git_head().is_none());
    }

    #[test]
    fn add_and_query_roundtrip() {
        let mut cache = Cache::new();

        cache.add_node(CacheNode {
            qualified_name: "Pkg::A".into(),
            kind: "part def".into(),
            file: "a.sysml".into(),
            line: 1,
            parent: Some("Pkg".into()),
        });
        cache.add_node(CacheNode {
            qualified_name: "Pkg::B".into(),
            kind: "port def".into(),
            file: "a.sysml".into(),
            line: 5,
            parent: Some("Pkg".into()),
        });
        cache.add_edge(CacheEdge {
            source: "Pkg::A".into(),
            target: "Pkg::B".into(),
            kind: "connects".into(),
        });

        assert_eq!(cache.find_nodes_by_kind("part def").len(), 1);
        assert_eq!(cache.find_nodes_by_kind("port def").len(), 1);
        assert_eq!(cache.find_edges_from("Pkg::A").len(), 1);
        assert_eq!(cache.find_edges_to("Pkg::B").len(), 1);
        assert!(cache.find_edges_from("Pkg::B").is_empty());
    }
}
