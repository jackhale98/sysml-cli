/// SQLite-backed model cache.
///
/// Drop-in replacement for the in-memory [`Cache`] that persists data to
/// `.sysml/cache.db`. Same public query API — all methods accept the same
/// arguments and return the same types.
///
/// Enabled via the `sqlite` Cargo feature.

#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};

use crate::cache::{CacheEdge, CacheNode, CacheRecord, CacheRefEdge, CacheStats};

/// SQLite-backed cache that persists to a `.db` file.
#[cfg(feature = "sqlite")]
pub struct SqliteCache {
    conn: Connection,
}

#[cfg(feature = "sqlite")]
impl SqliteCache {
    /// Open (or create) an SQLite cache at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        let cache = Self { conn };
        cache.create_tables()?;
        Ok(cache)
    }

    /// Create an in-memory SQLite cache (useful for testing).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let cache = Self { conn };
        cache.create_tables()?;
        Ok(cache)
    }

    fn create_tables(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                qualified_name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                parent TEXT
            );
            CREATE TABLE IF NOT EXISTS edges (
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                kind TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                tool TEXT NOT NULL,
                record_type TEXT NOT NULL,
                created TEXT NOT NULL,
                author TEXT NOT NULL,
                file TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ref_edges (
                record_id TEXT NOT NULL,
                qualified_name TEXT NOT NULL,
                ref_kind TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
            CREATE INDEX IF NOT EXISTS idx_ref_edges_record ON ref_edges(record_id);
            CREATE INDEX IF NOT EXISTS idx_ref_edges_qn ON ref_edges(qualified_name);
            CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
            CREATE INDEX IF NOT EXISTS idx_records_tool ON records(tool);",
        )?;
        Ok(())
    }

    // -- mutators -----------------------------------------------------------

    /// Insert a model node.
    pub fn add_node(&self, node: CacheNode) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO nodes (qualified_name, kind, file, line, parent)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![node.qualified_name, node.kind, node.file, node.line, node.parent],
        );
    }

    /// Insert a relationship edge.
    pub fn add_edge(&self, edge: CacheEdge) {
        let _ = self.conn.execute(
            "INSERT INTO edges (source, target, kind) VALUES (?1, ?2, ?3)",
            params![edge.source, edge.target, edge.kind],
        );
    }

    /// Insert a record.
    pub fn add_record(&self, record: CacheRecord) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO records (id, tool, record_type, created, author, file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id,
                record.tool,
                record.record_type,
                record.created,
                record.author,
                record.file,
            ],
        );
    }

    /// Insert a reference edge.
    pub fn add_ref_edge(&self, ref_edge: CacheRefEdge) {
        let _ = self.conn.execute(
            "INSERT INTO ref_edges (record_id, qualified_name, ref_kind) VALUES (?1, ?2, ?3)",
            params![ref_edge.record_id, ref_edge.qualified_name, ref_edge.ref_kind],
        );
    }

    // -- node queries -------------------------------------------------------

    /// Return all nodes whose `kind` matches exactly.
    pub fn find_nodes_by_kind(&self, kind: &str) -> Vec<CacheNode> {
        let mut stmt = self
            .conn
            .prepare("SELECT qualified_name, kind, file, line, parent FROM nodes WHERE kind = ?1")
            .unwrap();
        stmt.query_map(params![kind], |row| {
            Ok(CacheNode {
                qualified_name: row.get(0)?,
                kind: row.get(1)?,
                file: row.get(2)?,
                line: row.get(3)?,
                parent: row.get(4)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Return the first node whose qualified name matches.
    pub fn find_node(&self, qualified_name: &str) -> Option<CacheNode> {
        self.conn
            .query_row(
                "SELECT qualified_name, kind, file, line, parent FROM nodes WHERE qualified_name = ?1",
                params![qualified_name],
                |row| {
                    Ok(CacheNode {
                        qualified_name: row.get(0)?,
                        kind: row.get(1)?,
                        file: row.get(2)?,
                        line: row.get(3)?,
                        parent: row.get(4)?,
                    })
                },
            )
            .ok()
    }

    // -- edge queries -------------------------------------------------------

    /// Return all edges originating from `source`.
    pub fn find_edges_from(&self, source: &str) -> Vec<CacheEdge> {
        let mut stmt = self
            .conn
            .prepare("SELECT source, target, kind FROM edges WHERE source = ?1")
            .unwrap();
        stmt.query_map(params![source], |row| {
            Ok(CacheEdge {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Return all edges pointing to `target`.
    pub fn find_edges_to(&self, target: &str) -> Vec<CacheEdge> {
        let mut stmt = self
            .conn
            .prepare("SELECT source, target, kind FROM edges WHERE target = ?1")
            .unwrap();
        stmt.query_map(params![target], |row| {
            Ok(CacheEdge {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    // -- record queries -----------------------------------------------------

    /// Return all records produced by a given tool.
    pub fn find_records_by_tool(&self, tool: &str) -> Vec<CacheRecord> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, tool, record_type, created, author, file FROM records WHERE tool = ?1")
            .unwrap();
        stmt.query_map(params![tool], |row| {
            Ok(CacheRecord {
                id: row.get(0)?,
                tool: row.get(1)?,
                record_type: row.get(2)?,
                created: row.get(3)?,
                author: row.get(4)?,
                file: row.get(5)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Return all reference edges for a given record id.
    pub fn find_refs_for_record(&self, record_id: &str) -> Vec<CacheRefEdge> {
        let mut stmt = self
            .conn
            .prepare("SELECT record_id, qualified_name, ref_kind FROM ref_edges WHERE record_id = ?1")
            .unwrap();
        stmt.query_map(params![record_id], |row| {
            Ok(CacheRefEdge {
                record_id: row.get(0)?,
                qualified_name: row.get(1)?,
                ref_kind: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Return all records that reference a particular model element.
    pub fn find_records_referencing(&self, qualified_name: &str) -> Vec<CacheRecord> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT r.id, r.tool, r.record_type, r.created, r.author, r.file
                 FROM records r
                 INNER JOIN ref_edges re ON r.id = re.record_id
                 WHERE re.qualified_name = ?1",
            )
            .unwrap();
        stmt.query_map(params![qualified_name], |row| {
            Ok(CacheRecord {
                id: row.get(0)?,
                tool: row.get(1)?,
                record_type: row.get(2)?,
                created: row.get(3)?,
                author: row.get(4)?,
                file: row.get(5)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    // -- stats & git --------------------------------------------------------

    /// Return summary counts.
    pub fn stats(&self) -> CacheStats {
        let nodes: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .unwrap_or(0);
        let edges: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap_or(0);
        let records: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap_or(0);
        let ref_edges: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM ref_edges", [], |row| row.get(0))
            .unwrap_or(0);
        CacheStats {
            nodes,
            edges,
            records,
            ref_edges,
        }
    }

    /// Store the current git HEAD hash.
    pub fn set_git_head(&self, hash: &str) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('git_head', ?1)",
            params![hash],
        );
    }

    /// Return the stored git HEAD hash.
    pub fn git_head(&self) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'git_head'",
                [],
                |row| row.get(0),
            )
            .ok()
    }

    /// Returns `true` when the stored HEAD differs from `current_head`.
    pub fn is_stale(&self, current_head: &str) -> bool {
        match self.git_head() {
            Some(stored) => stored != current_head,
            None => true,
        }
    }

    /// Drop all cached data.
    pub fn clear(&self) {
        let _ = self.conn.execute_batch(
            "DELETE FROM nodes;
             DELETE FROM edges;
             DELETE FROM records;
             DELETE FROM ref_edges;
             DELETE FROM metadata;",
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;

    fn sample_cache() -> SqliteCache {
        let cache = SqliteCache::open_in_memory().unwrap();

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
    fn new_sqlite_cache_is_empty() {
        let cache = SqliteCache::open_in_memory().unwrap();
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

        let records = cache.find_records_referencing("Vehicle");
        assert_eq!(records.len(), 2);
        let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"rev-001"));
        assert!(ids.contains(&"dec-001"));

        let records = cache.find_records_referencing("Engine");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "rev-001");

        assert!(cache.find_records_referencing("MassReq").is_empty());
    }

    #[test]
    fn git_head_lifecycle() {
        let cache = SqliteCache::open_in_memory().unwrap();

        assert!(cache.git_head().is_none());
        assert!(cache.is_stale("abc123"));

        cache.set_git_head("abc123");
        assert_eq!(cache.git_head().as_deref(), Some("abc123"));
        assert!(!cache.is_stale("abc123"));

        assert!(cache.is_stale("def456"));

        cache.set_git_head("def456");
        assert!(!cache.is_stale("def456"));
    }

    #[test]
    fn clear_resets_everything() {
        let cache = sample_cache();
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
    fn persistence_across_reopen() {
        let tmp = std::env::temp_dir().join("sysml_sqlite_cache_test.db");
        let _ = std::fs::remove_file(&tmp);

        {
            let cache = SqliteCache::open(&tmp).unwrap();
            cache.add_node(CacheNode {
                qualified_name: "Persistent".into(),
                kind: "part def".into(),
                file: "test.sysml".into(),
                line: 1,
                parent: None,
            });
            cache.set_git_head("abc123");
        }

        // Reopen
        {
            let cache = SqliteCache::open(&tmp).unwrap();
            assert_eq!(cache.stats().nodes, 1);
            let node = cache.find_node("Persistent");
            assert!(node.is_some());
            assert_eq!(cache.git_head().as_deref(), Some("abc123"));
        }

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn node_upsert_replaces() {
        let cache = SqliteCache::open_in_memory().unwrap();
        cache.add_node(CacheNode {
            qualified_name: "A".into(),
            kind: "part def".into(),
            file: "old.sysml".into(),
            line: 1,
            parent: None,
        });
        cache.add_node(CacheNode {
            qualified_name: "A".into(),
            kind: "port def".into(),
            file: "new.sysml".into(),
            line: 5,
            parent: None,
        });
        assert_eq!(cache.stats().nodes, 1);
        let node = cache.find_node("A").unwrap();
        assert_eq!(node.kind, "port def");
        assert_eq!(node.file, "new.sysml");
    }
}
