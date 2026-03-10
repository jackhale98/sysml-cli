/// Indexes SysML model files and TOML records into the cache.
///
/// The [`Indexer`] walks a parsed [`Model`] and populates a [`Cache`] with
/// nodes (definitions and usages) and edges (relationships).  It also scans
/// a records directory for `.toml` files and indexes them.

use std::collections::HashMap;
use std::path::Path;

use crate::cache::{Cache, CacheEdge, CacheNode, CacheRecord, CacheRefEdge};
use crate::model::Model;
use crate::parser;

/// Stateless indexer — all methods are associated functions.
pub struct Indexer;

impl Indexer {
    /// Index a single parsed model into the cache.
    ///
    /// Extracts definitions (as nodes with qualified names built from
    /// `parent_def` chains), usages, and all relationship types (as edges).
    pub fn index_model(cache: &mut Cache, model: &Model) {
        // Build a lookup from simple name -> qualified name for definitions so
        // we can resolve `parent_def` chains.  The parser stores only the
        // immediate parent's simple name in `parent_def`, so we need to walk
        // the chain to produce fully-qualified names.
        let qualified_names = Self::build_qualified_names(model);

        // -- Index definitions as nodes -------------------------------------
        for def in &model.definitions {
            let qn = qualified_names
                .get(def.name.as_str())
                .cloned()
                .unwrap_or_else(|| def.name.clone());

            let parent = def.parent_def.as_ref().and_then(|p| {
                qualified_names.get(p.as_str()).cloned()
            });

            cache.add_node(CacheNode {
                qualified_name: qn,
                kind: def.kind.label().to_string(),
                file: model.file.clone(),
                line: def.span.start_row,
                parent,
            });

            // Specialization edge
            if let Some(ref super_type) = def.super_type {
                let source_qn = qualified_names
                    .get(def.name.as_str())
                    .cloned()
                    .unwrap_or_else(|| def.name.clone());
                cache.add_edge(CacheEdge {
                    source: source_qn,
                    target: super_type.clone(),
                    kind: "specializes".to_string(),
                });
            }
        }

        // -- Index usages as nodes ------------------------------------------
        for usage in &model.usages {
            if usage.name.is_empty() {
                continue;
            }
            let parent_qn = usage.parent_def.as_ref().and_then(|p| {
                qualified_names.get(p.as_str()).cloned()
            });

            let qn = match &parent_qn {
                Some(pqn) => format!("{}::{}", pqn, usage.name),
                None => usage.name.clone(),
            };

            cache.add_node(CacheNode {
                qualified_name: qn,
                kind: usage.kind.clone(),
                file: model.file.clone(),
                line: usage.span.start_row,
                parent: parent_qn,
            });
        }

        // -- Index relationship edges ---------------------------------------

        // Connections
        for conn in &model.connections {
            cache.add_edge(CacheEdge {
                source: conn.source.clone(),
                target: conn.target.clone(),
                kind: "connects".to_string(),
            });
        }

        // Flows
        for flow in &model.flows {
            cache.add_edge(CacheEdge {
                source: flow.source.clone(),
                target: flow.target.clone(),
                kind: "flows".to_string(),
            });
        }

        // Satisfactions
        for sat in &model.satisfactions {
            let source = sat
                .by
                .clone()
                .unwrap_or_default();
            if !source.is_empty() {
                cache.add_edge(CacheEdge {
                    source,
                    target: sat.requirement.clone(),
                    kind: "satisfies".to_string(),
                });
            }
        }

        // Verifications
        for ver in &model.verifications {
            cache.add_edge(CacheEdge {
                source: ver.by.clone(),
                target: ver.requirement.clone(),
                kind: "verifies".to_string(),
            });
        }

        // Allocations
        for alloc in &model.allocations {
            cache.add_edge(CacheEdge {
                source: alloc.source.clone(),
                target: alloc.target.clone(),
                kind: "allocates".to_string(),
            });
        }
    }

    /// Parse all `.sysml` files under `dir` (recursively) and index them.
    pub fn index_directory(cache: &mut Cache, dir: &Path) {
        let mut files = Vec::new();
        collect_sysml_files(dir, &mut files);

        for file_path in &files {
            let path_str = file_path.to_string_lossy().to_string();
            if let Ok(source) = std::fs::read_to_string(file_path) {
                let model = parser::parse_file(&path_str, &source);
                Self::index_model(cache, &model);
            }
        }
    }

    /// Scan for `.toml` files in a records directory and index them.
    ///
    /// Each TOML file is expected to contain top-level keys:
    ///   `id`, `tool`, `type`, `created`, `author`
    /// and an optional `refs` array of tables with `qualified_name` and
    /// `kind` keys.
    ///
    /// Because `sysml-core` does not depend on a TOML parser crate, this
    /// uses a minimal key-value extractor that handles simple flat TOML
    /// files.  Complex nested structures are not supported.
    pub fn index_records(cache: &mut Cache, dir: &Path) {
        if !dir.is_dir() {
            return;
        }
        let mut files = Vec::new();
        collect_toml_files(dir, &mut files);

        for file_path in &files {
            let path_str = file_path.to_string_lossy().to_string();
            if let Ok(contents) = std::fs::read_to_string(file_path) {
                let kv = parse_flat_toml(&contents);

                let id = kv.get("id").cloned().unwrap_or_default();
                if id.is_empty() {
                    continue;
                }

                let record = CacheRecord {
                    id: id.clone(),
                    tool: kv.get("tool").cloned().unwrap_or_default(),
                    record_type: kv.get("type").cloned().unwrap_or_default(),
                    created: kv.get("created").cloned().unwrap_or_default(),
                    author: kv.get("author").cloned().unwrap_or_default(),
                    file: path_str,
                };
                cache.add_record(record);

                // Parse [[refs]] sections
                for (qn, ref_kind) in parse_toml_refs(&contents) {
                    cache.add_ref_edge(CacheRefEdge {
                        record_id: id.clone(),
                        qualified_name: qn,
                        ref_kind,
                    });
                }
            }
        }
    }

    // -- private helpers ----------------------------------------------------

    /// Build a mapping from simple name -> qualified name for all
    /// definitions in a model.
    ///
    /// The parser populates `parent_def` with the immediate parent's simple
    /// name.  We walk definitions in order (parents appear before children
    /// in the tree-sitter walk) and chain names with `::`.
    fn build_qualified_names(model: &Model) -> HashMap<&str, String> {
        let mut qn_map: HashMap<&str, String> = HashMap::new();

        for def in &model.definitions {
            let qn = match &def.parent_def {
                Some(parent_name) => {
                    let parent_qn = qn_map
                        .get(parent_name.as_str())
                        .cloned()
                        .unwrap_or_else(|| parent_name.clone());
                    format!("{}::{}", parent_qn, def.name)
                }
                None => def.name.clone(),
            };
            qn_map.insert(&def.name, qn);
        }

        qn_map
    }
}

// ---------------------------------------------------------------------------
// File collection helpers
// ---------------------------------------------------------------------------

fn collect_sysml_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_sysml_files(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "sysml" || ext == "kerml" {
                    files.push(path);
                }
            }
        }
    }
}

fn collect_toml_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_toml_files(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "toml" {
                    files.push(path);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal TOML parser (flat key-value only)
// ---------------------------------------------------------------------------

/// Extract top-level `key = "value"` pairs from a TOML string.
///
/// This intentionally ignores array-of-table sections (`[[...]]`) and
/// nested tables (`[...]`).  Only simple string values are extracted.
fn parse_flat_toml(contents: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_section = false;

    for line in contents.lines() {
        let trimmed = line.trim();

        // Detect section headers — skip everything inside them
        if trimmed.starts_with('[') {
            in_section = true;
            continue;
        }

        if in_section {
            // Still inside a section — skip until we hit the next
            // top-level key.  We detect top-level keys by the absence of
            // indentation and presence of `=`.
            if !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains('=') {
                // Could be a key inside the section — we can't reliably
                // tell without full TOML parsing, so we stay in section
                // mode.
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = parse_kv_line(trimmed) {
            map.insert(key, value);
        }
    }

    map
}

/// Parse a single `key = "value"` line, returning the unquoted key and
/// value.
fn parse_kv_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim().to_string();
    let raw_value = parts.next()?.trim();

    // Strip surrounding quotes
    let value = if (raw_value.starts_with('"') && raw_value.ends_with('"'))
        || (raw_value.starts_with('\'') && raw_value.ends_with('\''))
    {
        raw_value[1..raw_value.len() - 1].to_string()
    } else {
        raw_value.to_string()
    };

    Some((key, value))
}

/// Parse `[[refs]]` sections from TOML content.
///
/// Each section is expected to contain `qualified_name` and `kind` keys.
/// Returns a vec of `(qualified_name, kind)` pairs.
fn parse_toml_refs(contents: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    let mut in_refs = false;
    let mut current_qn = String::new();
    let mut current_kind = String::new();

    for line in contents.lines() {
        let trimmed = line.trim();

        if trimmed == "[[refs]]" {
            // Flush any previous ref
            if in_refs && !current_qn.is_empty() {
                refs.push((
                    std::mem::take(&mut current_qn),
                    std::mem::take(&mut current_kind),
                ));
            }
            in_refs = true;
            current_qn.clear();
            current_kind.clear();
            continue;
        }

        // Another section header ends the refs block
        if trimmed.starts_with('[') && trimmed != "[[refs]]" {
            if in_refs && !current_qn.is_empty() {
                refs.push((
                    std::mem::take(&mut current_qn),
                    std::mem::take(&mut current_kind),
                ));
            }
            in_refs = false;
            continue;
        }

        if in_refs {
            if let Some((key, value)) = parse_kv_line(trimmed) {
                match key.as_str() {
                    "qualified_name" => current_qn = value,
                    "kind" => current_kind = value,
                    _ => {}
                }
            }
        }
    }

    // Flush trailing ref
    if in_refs && !current_qn.is_empty() {
        refs.push((current_qn, current_kind));
    }

    refs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use crate::model::*;

    /// Helper: build a minimal model with the given definitions, usages,
    /// and relationships.
    fn vehicle_model() -> Model {
        let mut model = Model::new("vehicle.sysml".to_string());

        model.definitions.push(Definition {
            kind: DefKind::Package,
            name: "VehicleModel".into(),
            super_type: None,
            span: Span {
                start_row: 1,
                start_col: 1,
                end_row: 30,
                end_col: 1,
                start_byte: 0,
                end_byte: 500,
            },
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: None,
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "Vehicle".into(),
            super_type: None,
            span: Span {
                start_row: 3,
                start_col: 5,
                end_row: 15,
                end_col: 5,
                start_byte: 40,
                end_byte: 300,
            },
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: Some("VehicleModel".into()),
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "Engine".into(),
            super_type: Some("PowerSource".into()),
            span: Span {
                start_row: 17,
                start_col: 5,
                end_row: 20,
                end_col: 5,
                start_byte: 310,
                end_byte: 400,
            },
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: Some("VehicleModel".into()),
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        model.definitions.push(Definition {
            kind: DefKind::Requirement,
            name: "MassReq".into(),
            super_type: None,
            span: Span {
                start_row: 22,
                start_col: 5,
                end_row: 25,
                end_col: 5,
                start_byte: 410,
                end_byte: 480,
            },
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: Some("VehicleModel".into()),
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        // Usages
        model.usages.push(Usage {
            kind: "part".into(),
            name: "engine".into(),
            type_ref: Some("Engine".into()),
            span: Span {
                start_row: 5,
                start_col: 9,
                end_row: 5,
                end_col: 30,
                start_byte: 80,
                end_byte: 110,
            },
            direction: None,
            is_conjugated: false,
            parent_def: Some("Vehicle".into()),
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });
        model.usages.push(Usage {
            kind: "part".into(),
            name: "wheel".into(),
            type_ref: Some("Wheel".into()),
            span: Span {
                start_row: 6,
                start_col: 9,
                end_row: 6,
                end_col: 30,
                start_byte: 111,
                end_byte: 140,
            },
            direction: None,
            is_conjugated: false,
            parent_def: Some("Vehicle".into()),
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        // Connections
        model.connections.push(Connection {
            name: Some("engineToTrans".into()),
            source: "engine".into(),
            target: "transmission".into(),
            span: Span {
                start_row: 10,
                start_col: 9,
                end_row: 10,
                end_col: 50,
                start_byte: 180,
                end_byte: 230,
            },
        });

        // Satisfactions
        model.satisfactions.push(Satisfaction {
            requirement: "MassReq".into(),
            by: Some("Vehicle".into()),
            span: Span {
                start_row: 28,
                start_col: 5,
                end_row: 28,
                end_col: 40,
                start_byte: 490,
                end_byte: 500,
            },
        });

        model
    }

    #[test]
    fn index_model_creates_nodes_for_definitions() {
        let mut cache = Cache::new();
        let model = vehicle_model();
        Indexer::index_model(&mut cache, &model);

        // 4 definitions + 2 usages = 6 nodes
        assert_eq!(cache.stats().nodes, 6);

        // Check qualified name of a nested definition
        let vehicle = cache.find_node("VehicleModel::Vehicle");
        assert!(vehicle.is_some(), "Vehicle should have qualified name VehicleModel::Vehicle");
        let vehicle = vehicle.unwrap();
        assert_eq!(vehicle.kind, "part def");
        assert_eq!(vehicle.file, "vehicle.sysml");
        assert_eq!(vehicle.line, 3);
        assert_eq!(vehicle.parent.as_deref(), Some("VehicleModel"));
    }

    #[test]
    fn index_model_creates_nodes_for_usages() {
        let mut cache = Cache::new();
        let model = vehicle_model();
        Indexer::index_model(&mut cache, &model);

        // Usage "engine" is under Vehicle, which is under VehicleModel
        let engine = cache.find_node("VehicleModel::Vehicle::engine");
        assert!(engine.is_some(), "engine usage should have fully qualified name");
        let engine = engine.unwrap();
        assert_eq!(engine.kind, "part");
        assert_eq!(engine.parent.as_deref(), Some("VehicleModel::Vehicle"));
    }

    #[test]
    fn index_model_creates_specialization_edges() {
        let mut cache = Cache::new();
        let model = vehicle_model();
        Indexer::index_model(&mut cache, &model);

        let edges = cache.find_edges_from("VehicleModel::Engine");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "PowerSource");
        assert_eq!(edges[0].kind, "specializes");
    }

    #[test]
    fn index_model_creates_connection_edges() {
        let mut cache = Cache::new();
        let model = vehicle_model();
        Indexer::index_model(&mut cache, &model);

        let edges: Vec<_> = cache
            .find_edges_from("engine")
            .into_iter()
            .filter(|e| e.kind == "connects")
            .collect();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "transmission");
    }

    #[test]
    fn index_model_creates_satisfaction_edges() {
        let mut cache = Cache::new();
        let model = vehicle_model();
        Indexer::index_model(&mut cache, &model);

        let edges: Vec<_> = cache
            .find_edges_from("Vehicle")
            .into_iter()
            .filter(|e| e.kind == "satisfies")
            .collect();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "MassReq");
    }

    #[test]
    fn index_model_handles_empty_model() {
        let mut cache = Cache::new();
        let model = Model::new("empty.sysml".to_string());
        Indexer::index_model(&mut cache, &model);

        assert_eq!(cache.stats().nodes, 0);
        assert_eq!(cache.stats().edges, 0);
    }

    #[test]
    fn index_model_skips_unnamed_usages() {
        let mut cache = Cache::new();
        let mut model = Model::new("test.sysml".to_string());
        model.usages.push(Usage {
            kind: "part".into(),
            name: "".into(),
            type_ref: Some("Foo".into()),
            span: Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: None,
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        Indexer::index_model(&mut cache, &model);
        assert_eq!(cache.stats().nodes, 0);
    }

    #[test]
    fn build_qualified_names_chains_parents() {
        let mut model = Model::new("test.sysml".to_string());

        // A -> B -> C  (three levels of nesting)
        model.definitions.push(Definition {
            kind: DefKind::Package,
            name: "A".into(),
            super_type: None,
            span: Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: None,
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });
        model.definitions.push(Definition {
            kind: DefKind::Part,
            name: "B".into(),
            super_type: None,
            span: Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: Some("A".into()),
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });
        model.definitions.push(Definition {
            kind: DefKind::Port,
            name: "C".into(),
            super_type: None,
            span: Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: Some("B".into()),
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });

        let qn = Indexer::build_qualified_names(&model);
        assert_eq!(qn.get("A").map(String::as_str), Some("A"));
        assert_eq!(qn.get("B").map(String::as_str), Some("A::B"));
        assert_eq!(qn.get("C").map(String::as_str), Some("A::B::C"));
    }

    #[test]
    fn parse_flat_toml_extracts_top_level_keys() {
        let contents = r#"
id = "rev-001"
tool = "review"
type = "design-review"
created = "2025-01-15"
author = "alice"

[[refs]]
qualified_name = "Vehicle"
kind = "reviews"
"#;
        let kv = parse_flat_toml(contents);
        assert_eq!(kv.get("id").map(String::as_str), Some("rev-001"));
        assert_eq!(kv.get("tool").map(String::as_str), Some("review"));
        assert_eq!(kv.get("type").map(String::as_str), Some("design-review"));
        assert_eq!(kv.get("created").map(String::as_str), Some("2025-01-15"));
        assert_eq!(kv.get("author").map(String::as_str), Some("alice"));
    }

    #[test]
    fn parse_flat_toml_ignores_comments_and_blanks() {
        let contents = r#"
# This is a comment
id = "test-001"

# Another comment
tool = "lint"
"#;
        let kv = parse_flat_toml(contents);
        assert_eq!(kv.len(), 2);
        assert_eq!(kv.get("id").map(String::as_str), Some("test-001"));
    }

    #[test]
    fn parse_toml_refs_extracts_multiple_refs() {
        let contents = r#"
id = "rev-001"
tool = "review"

[[refs]]
qualified_name = "Vehicle"
kind = "reviews"

[[refs]]
qualified_name = "Engine"
kind = "reviews"
"#;
        let refs = parse_toml_refs(contents);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("Vehicle".to_string(), "reviews".to_string()));
        assert_eq!(refs[1], ("Engine".to_string(), "reviews".to_string()));
    }

    #[test]
    fn parse_toml_refs_handles_no_refs() {
        let contents = r#"
id = "test-001"
tool = "lint"
"#;
        let refs = parse_toml_refs(contents);
        assert!(refs.is_empty());
    }

    #[test]
    fn parse_toml_refs_handles_other_sections() {
        let contents = r#"
id = "rev-002"

[[refs]]
qualified_name = "MassReq"
kind = "decides"

[metadata]
version = "1"
"#;
        let refs = parse_toml_refs(contents);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "MassReq");
    }

    #[test]
    fn index_records_from_temp_dir() {
        let tmp = std::env::temp_dir().join("sysml_index_test_records");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let toml_content = r#"
id = "rev-100"
tool = "review"
type = "design-review"
created = "2025-06-01"
author = "charlie"

[[refs]]
qualified_name = "Vehicle"
kind = "reviews"

[[refs]]
qualified_name = "Engine"
kind = "reviews"
"#;
        std::fs::write(tmp.join("rev-100.toml"), toml_content).unwrap();

        let mut cache = Cache::new();
        Indexer::index_records(&mut cache, &tmp);

        assert_eq!(cache.stats().records, 1);
        assert_eq!(cache.stats().ref_edges, 2);

        let records = cache.find_records_by_tool("review");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "rev-100");
        assert_eq!(records[0].author, "charlie");

        let refs = cache.find_refs_for_record("rev-100");
        assert_eq!(refs.len(), 2);

        let referencing = cache.find_records_referencing("Vehicle");
        assert_eq!(referencing.len(), 1);
        assert_eq!(referencing[0].id, "rev-100");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn index_records_skips_missing_dir() {
        let mut cache = Cache::new();
        Indexer::index_records(&mut cache, Path::new("/nonexistent/path"));
        assert_eq!(cache.stats().records, 0);
    }

    #[test]
    fn index_records_skips_files_without_id() {
        let tmp = std::env::temp_dir().join("sysml_index_test_no_id");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::fs::write(tmp.join("bad.toml"), "tool = \"review\"\n").unwrap();

        let mut cache = Cache::new();
        Indexer::index_records(&mut cache, &tmp);
        assert_eq!(cache.stats().records, 0);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn index_model_verification_edges() {
        let mut cache = Cache::new();
        let mut model = Model::new("test.sysml".to_string());

        model.verifications.push(Verification {
            requirement: "MassReq".into(),
            by: "MassTest".into(),
            span: Span::default(),
        });

        Indexer::index_model(&mut cache, &model);

        let edges = cache.find_edges_from("MassTest");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "MassReq");
        assert_eq!(edges[0].kind, "verifies");
    }

    #[test]
    fn index_model_allocation_edges() {
        let mut cache = Cache::new();
        let mut model = Model::new("test.sysml".to_string());

        model.allocations.push(Allocation {
            source: "SoftwareModule".into(),
            target: "ECU".into(),
            span: Span::default(),
        });

        Indexer::index_model(&mut cache, &model);

        let edges = cache.find_edges_from("SoftwareModule");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "ECU");
        assert_eq!(edges[0].kind, "allocates");
    }

    #[test]
    fn index_model_flow_edges() {
        let mut cache = Cache::new();
        let mut model = Model::new("test.sysml".to_string());

        model.flows.push(Flow {
            name: Some("fuelFlow".into()),
            item_type: Some("Fuel".into()),
            source: "fuelTank".into(),
            target: "engine".into(),
            span: Span::default(),
        });

        Indexer::index_model(&mut cache, &model);

        let edges = cache.find_edges_from("fuelTank");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "engine");
        assert_eq!(edges[0].kind, "flows");
    }

    #[test]
    fn index_model_top_level_usage_no_parent() {
        let mut cache = Cache::new();
        let mut model = Model::new("test.sysml".to_string());

        model.usages.push(Usage {
            kind: "part".into(),
            name: "standalone".into(),
            type_ref: None,
            span: Span {
                start_row: 1,
                start_col: 1,
                end_row: 1,
                end_col: 20,
                start_byte: 0,
                end_byte: 20,
            },
            direction: None,
            is_conjugated: false,
            parent_def: None,
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        Indexer::index_model(&mut cache, &model);

        let node = cache.find_node("standalone");
        assert!(node.is_some());
        assert!(node.unwrap().parent.is_none());
    }
}
