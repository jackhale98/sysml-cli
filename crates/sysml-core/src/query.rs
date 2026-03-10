/// Model query functions for filtering and inspecting SysML v2 elements.
///
/// These functions provide the logic behind CLI commands like `list`, `show`,
/// `trace`, `interfaces`, `stats`, `deps`, `diff`, `allocation`, and `coverage`.

use crate::model::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

// ========================================================================
// Element enumeration — a unified view over definitions and usages
// ========================================================================

/// A unified view of any model element (definition or usage).
#[derive(Debug, Clone)]
pub enum Element<'a> {
    Def(&'a Definition),
    Usage(&'a Usage),
}

impl<'a> Element<'a> {
    pub fn name(&self) -> &str {
        match self {
            Element::Def(d) => &d.name,
            Element::Usage(u) => &u.name,
        }
    }

    pub fn kind_label(&self) -> &str {
        match self {
            Element::Def(d) => d.kind.label(),
            Element::Usage(u) => &u.kind,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Element::Def(d) => &d.span,
            Element::Usage(u) => &u.span,
        }
    }

    pub fn parent_def(&self) -> Option<&str> {
        match self {
            Element::Def(d) => d.parent_def.as_deref(),
            Element::Usage(u) => u.parent_def.as_deref(),
        }
    }

    pub fn type_ref(&self) -> Option<&str> {
        match self {
            Element::Def(d) => d.super_type.as_deref(),
            Element::Usage(u) => u.type_ref.as_deref(),
        }
    }

    pub fn short_name(&self) -> Option<&str> {
        match self {
            Element::Def(d) => d.short_name.as_deref(),
            Element::Usage(_) => None,
        }
    }

    pub fn doc(&self) -> Option<&str> {
        match self {
            Element::Def(d) => d.doc.as_deref(),
            Element::Usage(_) => None,
        }
    }
}

// ========================================================================
// Filter types
// ========================================================================

/// Kind filter for the `list` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindFilter {
    /// All definitions.
    Definitions,
    /// All usages.
    Usages,
    /// Specific definition kind.
    DefKind(DefKind),
    /// Specific usage kind string (e.g., "part", "port").
    UsageKind(String),
    /// Everything.
    All,
}

/// Filter criteria for listing model elements.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Filter by element kind.
    pub kind: Option<KindFilter>,
    /// Filter by name pattern (substring match).
    pub name_pattern: Option<String>,
    /// Filter by parent definition name.
    pub parent: Option<String>,
    /// Only show unused definitions.
    pub unused_only: bool,
    /// Only show abstract definitions.
    pub abstract_only: bool,
    /// Filter by visibility.
    pub visibility: Option<Visibility>,
}

/// List model elements matching the given filter.
pub fn list_elements<'a>(model: &'a Model, filter: &ListFilter) -> Vec<Element<'a>> {
    let mut results = Vec::new();

    let include_defs = match &filter.kind {
        None | Some(KindFilter::All) | Some(KindFilter::Definitions) => true,
        Some(KindFilter::DefKind(_)) => true,
        Some(KindFilter::UsageKind(_)) | Some(KindFilter::Usages) => false,
    };

    let include_usages = match &filter.kind {
        None | Some(KindFilter::All) | Some(KindFilter::Usages) => true,
        Some(KindFilter::UsageKind(_)) => true,
        Some(KindFilter::DefKind(_)) | Some(KindFilter::Definitions) => false,
    };

    let referenced = if filter.unused_only {
        Some(model.referenced_names())
    } else {
        None
    };

    if include_defs {
        for def in &model.definitions {
            if let Some(KindFilter::DefKind(k)) = &filter.kind {
                if def.kind != *k {
                    continue;
                }
            }
            if let Some(pat) = &filter.name_pattern {
                if !def.name.contains(pat.as_str()) {
                    continue;
                }
            }
            if let Some(parent) = &filter.parent {
                if def.parent_def.as_deref() != Some(parent.as_str()) {
                    continue;
                }
            }
            if filter.abstract_only && !def.is_abstract {
                continue;
            }
            if let Some(vis) = &filter.visibility {
                if def.visibility.as_ref() != Some(vis) {
                    continue;
                }
            }
            if let Some(ref refs) = referenced {
                if refs.contains(def.name.as_str()) {
                    continue;
                }
            }
            results.push(Element::Def(def));
        }
    }

    if include_usages {
        for usage in &model.usages {
            if let Some(KindFilter::UsageKind(ref k)) = filter.kind {
                if usage.kind != *k {
                    continue;
                }
            }
            if let Some(pat) = &filter.name_pattern {
                if !usage.name.contains(pat.as_str()) {
                    continue;
                }
            }
            if let Some(parent) = &filter.parent {
                if usage.parent_def.as_deref() != Some(parent.as_str()) {
                    continue;
                }
            }
            results.push(Element::Usage(usage));
        }
    }

    results
}

// ========================================================================
// View-based filtering
// ========================================================================

/// Apply a named view definition's filters to build a `ListFilter`.
///
/// Looks up the view by name in the model, then converts its expose/filter
/// clauses into a `ListFilter`. Returns None if the view is not found.
pub fn filter_from_view(model: &Model, view_name: &str) -> Option<ListFilter> {
    let view = model.views.iter().find(|v| v.name == view_name)?;
    let mut filter = ListFilter::default();

    // Apply kind filters from the view
    if let Some(kind_str) = view.kind_filters.first() {
        filter.kind = parse_kind_filter(kind_str);
    }

    // Apply expose scope — if an expose targets "Foo::*", set parent=Foo
    for expose in &view.exposes {
        if let Some(base) = expose.strip_suffix("::*").or_else(|| expose.strip_suffix("::**")) {
            filter.parent = Some(base.to_string());
            break;
        }
    }

    Some(filter)
}

/// Parse a kind string into a `KindFilter` (reusable helper).
pub fn parse_kind_filter(s: &str) -> Option<KindFilter> {
    match s.to_lowercase().as_str() {
        "all" => Some(KindFilter::All),
        "definitions" | "defs" => Some(KindFilter::Definitions),
        "usages" => Some(KindFilter::Usages),
        // Definition kinds (plural form)
        "parts" => Some(KindFilter::DefKind(DefKind::Part)),
        "ports" => Some(KindFilter::DefKind(DefKind::Port)),
        "actions" => Some(KindFilter::DefKind(DefKind::Action)),
        "states" => Some(KindFilter::DefKind(DefKind::State)),
        "requirements" => Some(KindFilter::DefKind(DefKind::Requirement)),
        "constraints" => Some(KindFilter::DefKind(DefKind::Constraint)),
        "connections" => Some(KindFilter::DefKind(DefKind::Connection)),
        "interfaces" => Some(KindFilter::DefKind(DefKind::Interface)),
        "flows" => Some(KindFilter::DefKind(DefKind::Flow)),
        "calculations" | "calcs" => Some(KindFilter::DefKind(DefKind::Calc)),
        "views" => Some(KindFilter::DefKind(DefKind::View)),
        "viewpoints" => Some(KindFilter::DefKind(DefKind::Viewpoint)),
        "enums" => Some(KindFilter::DefKind(DefKind::Enum)),
        "packages" => Some(KindFilter::DefKind(DefKind::Package)),
        "attributes" | "attrs" => Some(KindFilter::DefKind(DefKind::Attribute)),
        "items" => Some(KindFilter::DefKind(DefKind::Item)),
        // Usage kinds (singular form)
        "part" => Some(KindFilter::UsageKind("part".to_string())),
        "port" => Some(KindFilter::UsageKind("port".to_string())),
        "action" => Some(KindFilter::UsageKind("action".to_string())),
        "state" => Some(KindFilter::UsageKind("state".to_string())),
        "requirement" => Some(KindFilter::UsageKind("requirement".to_string())),
        "constraint" => Some(KindFilter::UsageKind("constraint".to_string())),
        "attribute" | "attr" => Some(KindFilter::UsageKind("attribute".to_string())),
        "item" => Some(KindFilter::UsageKind("item".to_string())),
        "connection" => Some(KindFilter::UsageKind("connection".to_string())),
        _ => None,
    }
}

// ========================================================================
// Requirements traceability
// ========================================================================

/// A row in the requirements traceability matrix.
#[derive(Debug, Clone)]
pub struct TraceRow {
    pub requirement: String,
    pub satisfied_by: Vec<String>,
    pub verified_by: Vec<String>,
}

/// Generate a requirements traceability matrix.
pub fn trace_requirements(model: &Model) -> Vec<TraceRow> {
    let req_defs: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Requirement)
        .collect();

    let mut rows = Vec::new();
    for req in &req_defs {
        let satisfied_by: Vec<String> = model
            .satisfactions
            .iter()
            .filter(|s| simple_name(&s.requirement) == req.name)
            .map(|s| s.by.clone().unwrap_or_else(|| "(implicit)".to_string()))
            .collect();

        let verified_by: Vec<String> = model
            .verifications
            .iter()
            .filter(|v| simple_name(&v.requirement) == req.name)
            .map(|v| v.by.clone())
            .collect();

        rows.push(TraceRow {
            requirement: req.name.clone(),
            satisfied_by,
            verified_by,
        });
    }
    rows
}

/// Trace coverage statistics.
#[derive(Debug, Clone)]
pub struct TraceCoverage {
    pub total_requirements: usize,
    pub satisfied_count: usize,
    pub verified_count: usize,
    pub fully_traced_count: usize,
}

/// Compute requirements trace coverage.
pub fn trace_coverage(rows: &[TraceRow]) -> TraceCoverage {
    let total = rows.len();
    let satisfied = rows.iter().filter(|r| !r.satisfied_by.is_empty()).count();
    let verified = rows.iter().filter(|r| !r.verified_by.is_empty()).count();
    let fully_traced = rows
        .iter()
        .filter(|r| !r.satisfied_by.is_empty() && !r.verified_by.is_empty())
        .count();

    TraceCoverage {
        total_requirements: total,
        satisfied_count: satisfied,
        verified_count: verified,
        fully_traced_count: fully_traced,
    }
}

// ========================================================================
// Interface / port analysis
// ========================================================================

/// Information about a port within a definition.
#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub type_ref: Option<String>,
    pub direction: Option<Direction>,
    pub is_conjugated: bool,
    pub owner: String,
}

/// List all ports in the model with their owners.
pub fn list_ports(model: &Model) -> Vec<PortInfo> {
    model
        .usages
        .iter()
        .filter(|u| u.kind == "port")
        .map(|u| PortInfo {
            name: u.name.clone(),
            type_ref: u.type_ref.clone(),
            direction: u.direction,
            is_conjugated: u.is_conjugated,
            owner: u.parent_def.clone().unwrap_or_default(),
        })
        .collect()
}

/// Find ports that are not referenced by any connection.
pub fn unconnected_ports(model: &Model) -> Vec<PortInfo> {
    let connected_names: std::collections::HashSet<&str> = model
        .connections
        .iter()
        .flat_map(|c| vec![simple_name(&c.source), simple_name(&c.target)])
        .collect();

    list_ports(model)
        .into_iter()
        .filter(|p| !connected_names.contains(p.name.as_str()))
        .collect()
}

// ========================================================================
// Model statistics
// ========================================================================

/// Aggregate metrics about a model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    pub total_definitions: usize,
    pub total_usages: usize,
    pub def_counts: Vec<(String, usize)>,
    pub usage_counts: Vec<(String, usize)>,
    pub connection_count: usize,
    pub flow_count: usize,
    pub satisfaction_count: usize,
    pub verification_count: usize,
    pub allocation_count: usize,
    pub import_count: usize,
    pub package_count: usize,
    pub abstract_def_count: usize,
    pub doc_coverage: DocCoverage,
    pub max_nesting_depth: usize,
}

/// Documentation coverage statistics.
#[derive(Debug, Clone, Serialize)]
pub struct DocCoverage {
    pub documented: usize,
    pub total: usize,
    pub percentage: f64,
}

/// Compute aggregate statistics for a model.
pub fn model_stats(model: &Model) -> ModelStats {
    let total_definitions = model.definitions.len();
    let total_usages = model.usages.len();

    // Count definitions by kind
    let mut def_map: HashMap<&str, usize> = HashMap::new();
    for d in &model.definitions {
        *def_map.entry(d.kind.label()).or_insert(0) += 1;
    }
    let mut def_counts: Vec<(String, usize)> = def_map
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    def_counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    // Count usages by kind
    let mut usage_map: HashMap<&str, usize> = HashMap::new();
    for u in &model.usages {
        *usage_map.entry(&u.kind).or_insert(0) += 1;
    }
    let mut usage_counts: Vec<(String, usize)> = usage_map
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    usage_counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let abstract_def_count = model.definitions.iter().filter(|d| d.is_abstract).count();
    let package_count = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Package)
        .count();

    // Documentation coverage (exclude packages)
    let non_pkg_defs: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| d.kind != DefKind::Package)
        .collect();
    let documented = non_pkg_defs.iter().filter(|d| d.doc.is_some()).count();
    let doc_total = non_pkg_defs.len();
    let doc_pct = if doc_total > 0 {
        100.0 * documented as f64 / doc_total as f64
    } else {
        100.0
    };

    // Max nesting depth via parent_def chains
    let def_names: HashMap<&str, &Definition> = model
        .definitions
        .iter()
        .map(|d| (d.name.as_str(), d))
        .collect();
    let mut max_depth = 0usize;
    for d in &model.definitions {
        let mut depth = 0;
        let mut current = d;
        while let Some(ref parent) = current.parent_def {
            depth += 1;
            if let Some(p) = def_names.get(parent.as_str()) {
                current = p;
            } else {
                break;
            }
        }
        max_depth = max_depth.max(depth);
    }

    ModelStats {
        total_definitions,
        total_usages,
        def_counts,
        usage_counts,
        connection_count: model.connections.len(),
        flow_count: model.flows.len(),
        satisfaction_count: model.satisfactions.len(),
        verification_count: model.verifications.len(),
        allocation_count: model.allocations.len(),
        import_count: model.imports.len(),
        package_count,
        abstract_def_count,
        doc_coverage: DocCoverage {
            documented,
            total: doc_total,
            percentage: doc_pct,
        },
        max_nesting_depth: max_depth,
    }
}

// ========================================================================
// Dependency / impact analysis
// ========================================================================

/// Dependency analysis result for a target element.
#[derive(Debug, Clone, Serialize)]
pub struct DepAnalysis {
    pub target: String,
    pub referenced_by: Vec<DepRef>,
    pub depends_on: Vec<DepRef>,
}

/// A single dependency reference.
#[derive(Debug, Clone, Serialize)]
pub struct DepRef {
    pub name: String,
    pub kind: String,
    pub relationship: String,
}

/// Compute forward and reverse dependencies for a named element.
pub fn dependency_analysis(model: &Model, target_name: &str) -> DepAnalysis {
    let mut referenced_by = Vec::new();
    let mut depends_on = Vec::new();

    // Reverse: who references target?
    for u in &model.usages {
        if let Some(ref t) = u.type_ref {
            if simple_name(t) == target_name {
                referenced_by.push(DepRef {
                    name: u.name.clone(),
                    kind: u.kind.clone(),
                    relationship: "type_ref".to_string(),
                });
            }
        }
    }
    for d in &model.definitions {
        if let Some(ref s) = d.super_type {
            if simple_name(s) == target_name {
                referenced_by.push(DepRef {
                    name: d.name.clone(),
                    kind: d.kind.label().to_string(),
                    relationship: "specializes".to_string(),
                });
            }
        }
    }
    for c in &model.connections {
        if simple_name(&c.source) == target_name || simple_name(&c.target) == target_name {
            let other = if simple_name(&c.source) == target_name {
                &c.target
            } else {
                &c.source
            };
            referenced_by.push(DepRef {
                name: c.name.clone().unwrap_or_else(|| other.clone()),
                kind: "connection".to_string(),
                relationship: "connection".to_string(),
            });
        }
    }
    for f in &model.flows {
        if simple_name(&f.source) == target_name || simple_name(&f.target) == target_name {
            let other = if simple_name(&f.source) == target_name {
                &f.target
            } else {
                &f.source
            };
            referenced_by.push(DepRef {
                name: other.clone(),
                kind: "flow".to_string(),
                relationship: "flow".to_string(),
            });
        }
    }
    for s in &model.satisfactions {
        if simple_name(&s.requirement) == target_name {
            referenced_by.push(DepRef {
                name: s.by.clone().unwrap_or_else(|| "(implicit)".to_string()),
                kind: "satisfaction".to_string(),
                relationship: "satisfies".to_string(),
            });
        }
    }
    for v in &model.verifications {
        if simple_name(&v.requirement) == target_name {
            referenced_by.push(DepRef {
                name: v.by.clone(),
                kind: "verification".to_string(),
                relationship: "verifies".to_string(),
            });
        }
    }
    for a in &model.allocations {
        if simple_name(&a.source) == target_name || simple_name(&a.target) == target_name {
            let other = if simple_name(&a.source) == target_name {
                &a.target
            } else {
                &a.source
            };
            referenced_by.push(DepRef {
                name: other.clone(),
                kind: "allocation".to_string(),
                relationship: "allocation".to_string(),
            });
        }
    }

    // Forward: what does target depend on?
    if let Some(def) = model.find_def(target_name) {
        if let Some(ref s) = def.super_type {
            depends_on.push(DepRef {
                name: s.clone(),
                kind: "super_type".to_string(),
                relationship: "specializes".to_string(),
            });
        }
    }
    for u in &model.usages {
        if u.parent_def.as_deref() == Some(target_name) {
            if let Some(ref t) = u.type_ref {
                depends_on.push(DepRef {
                    name: t.clone(),
                    kind: u.kind.clone(),
                    relationship: "type_ref".to_string(),
                });
            }
        }
    }

    DepAnalysis {
        target: target_name.to_string(),
        referenced_by,
        depends_on,
    }
}

// ========================================================================
// Semantic model diff
// ========================================================================

/// Semantic diff between two models.
#[derive(Debug, Clone, Serialize)]
pub struct ModelDiff {
    pub added_defs: Vec<String>,
    pub removed_defs: Vec<String>,
    pub changed_defs: Vec<DefChange>,
    pub added_usages: Vec<UsageKey>,
    pub removed_usages: Vec<UsageKey>,
    pub changed_usages: Vec<UsageChange>,
    pub added_connections: Vec<String>,
    pub removed_connections: Vec<String>,
}

/// A definition that changed between versions.
#[derive(Debug, Clone, Serialize)]
pub struct DefChange {
    pub name: String,
    pub changes: Vec<String>,
}

/// Key for identifying a usage (name + parent).
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct UsageKey {
    pub name: String,
    pub parent: Option<String>,
}

/// A usage that changed between versions.
#[derive(Debug, Clone, Serialize)]
pub struct UsageChange {
    pub key: UsageKey,
    pub changes: Vec<String>,
}

/// Compute semantic diff between two models.
pub fn model_diff(old: &Model, new: &Model) -> ModelDiff {
    // Definitions
    let old_defs: HashMap<&str, &Definition> =
        old.definitions.iter().map(|d| (d.name.as_str(), d)).collect();
    let new_defs: HashMap<&str, &Definition> =
        new.definitions.iter().map(|d| (d.name.as_str(), d)).collect();

    let mut added_defs = Vec::new();
    let mut removed_defs = Vec::new();
    let mut changed_defs = Vec::new();

    for name in new_defs.keys() {
        if !old_defs.contains_key(name) {
            added_defs.push(name.to_string());
        }
    }
    for name in old_defs.keys() {
        if !new_defs.contains_key(name) {
            removed_defs.push(name.to_string());
        }
    }
    for (name, new_d) in &new_defs {
        if let Some(old_d) = old_defs.get(name) {
            let mut changes = Vec::new();
            if old_d.kind != new_d.kind {
                changes.push(format!(
                    "kind: {} -> {}",
                    old_d.kind.label(),
                    new_d.kind.label()
                ));
            }
            if old_d.super_type != new_d.super_type {
                changes.push(format!(
                    "super_type: {} -> {}",
                    old_d.super_type.as_deref().unwrap_or("none"),
                    new_d.super_type.as_deref().unwrap_or("none")
                ));
            }
            if old_d.is_abstract != new_d.is_abstract {
                changes.push(format!(
                    "abstract: {} -> {}",
                    old_d.is_abstract, new_d.is_abstract
                ));
            }
            if old_d.visibility != new_d.visibility {
                changes.push(format!(
                    "visibility: {} -> {}",
                    old_d
                        .visibility
                        .as_ref()
                        .map(|v| v.label())
                        .unwrap_or("default"),
                    new_d
                        .visibility
                        .as_ref()
                        .map(|v| v.label())
                        .unwrap_or("default")
                ));
            }
            if old_d.doc != new_d.doc {
                changes.push("doc changed".to_string());
            }
            if !changes.is_empty() {
                changed_defs.push(DefChange {
                    name: name.to_string(),
                    changes,
                });
            }
        }
    }

    added_defs.sort();
    removed_defs.sort();

    // Usages (keyed by name + parent)
    fn usage_key(u: &Usage) -> UsageKey {
        UsageKey {
            name: u.name.clone(),
            parent: u.parent_def.clone(),
        }
    }
    let old_usages: HashMap<UsageKey, &Usage> =
        old.usages.iter().map(|u| (usage_key(u), u)).collect();
    let new_usages: HashMap<UsageKey, &Usage> =
        new.usages.iter().map(|u| (usage_key(u), u)).collect();

    let mut added_usages = Vec::new();
    let mut removed_usages = Vec::new();
    let mut changed_usages = Vec::new();

    for key in new_usages.keys() {
        if !old_usages.contains_key(key) {
            added_usages.push(key.clone());
        }
    }
    for key in old_usages.keys() {
        if !new_usages.contains_key(key) {
            removed_usages.push(key.clone());
        }
    }
    for (key, new_u) in &new_usages {
        if let Some(old_u) = old_usages.get(key) {
            let mut changes = Vec::new();
            if old_u.type_ref != new_u.type_ref {
                changes.push(format!(
                    "type: {} -> {}",
                    old_u.type_ref.as_deref().unwrap_or("none"),
                    new_u.type_ref.as_deref().unwrap_or("none")
                ));
            }
            if old_u.direction != new_u.direction {
                changes.push(format!(
                    "direction: {} -> {}",
                    old_u
                        .direction
                        .as_ref()
                        .map(|d| d.label())
                        .unwrap_or("none"),
                    new_u
                        .direction
                        .as_ref()
                        .map(|d| d.label())
                        .unwrap_or("none")
                ));
            }
            if !changes.is_empty() {
                changed_usages.push(UsageChange {
                    key: key.clone(),
                    changes,
                });
            }
        }
    }

    // Connections
    fn conn_key(c: &Connection) -> String {
        format!("{} -> {}", c.source, c.target)
    }
    let old_conns: HashSet<String> = old.connections.iter().map(conn_key).collect();
    let new_conns: HashSet<String> = new.connections.iter().map(conn_key).collect();
    let mut added_connections: Vec<String> = new_conns.difference(&old_conns).cloned().collect();
    let mut removed_connections: Vec<String> = old_conns.difference(&new_conns).cloned().collect();
    added_connections.sort();
    removed_connections.sort();

    ModelDiff {
        added_defs,
        removed_defs,
        changed_defs,
        added_usages,
        removed_usages,
        changed_usages,
        added_connections,
        removed_connections,
    }
}

// ========================================================================
// Allocation traceability
// ========================================================================

/// Allocation traceability report.
#[derive(Debug, Clone, Serialize)]
pub struct AllocationReport {
    pub rows: Vec<AllocationRow>,
    pub unallocated_sources: Vec<String>,
    pub unallocated_targets: Vec<String>,
    pub total_allocations: usize,
}

/// A single allocation mapping.
#[derive(Debug, Clone, Serialize)]
pub struct AllocationRow {
    pub source: String,
    pub target: String,
}

/// Generate an allocation traceability report.
pub fn allocation_report(model: &Model) -> AllocationReport {
    let rows: Vec<AllocationRow> = model
        .allocations
        .iter()
        .map(|a| AllocationRow {
            source: a.source.clone(),
            target: a.target.clone(),
        })
        .collect();

    let allocated_names: HashSet<&str> = model
        .allocations
        .iter()
        .flat_map(|a| vec![simple_name(&a.source), simple_name(&a.target)])
        .collect();

    // Find action defs not in any allocation (logical elements)
    let unallocated_sources: Vec<String> = model
        .definitions
        .iter()
        .filter(|d| matches!(d.kind, DefKind::Action | DefKind::UseCase))
        .filter(|d| !allocated_names.contains(d.name.as_str()))
        .map(|d| d.name.clone())
        .collect();

    // Find part defs not in any allocation (physical elements)
    let unallocated_targets: Vec<String> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Part)
        .filter(|d| !allocated_names.contains(d.name.as_str()))
        .map(|d| d.name.clone())
        .collect();

    let total = rows.len();
    AllocationReport {
        rows,
        unallocated_sources,
        unallocated_targets,
        total_allocations: total,
    }
}

// ========================================================================
// Model coverage / completeness
// ========================================================================

/// Model completeness report.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub undocumented_defs: Vec<CoverageItem>,
    pub untyped_usages: Vec<CoverageItem>,
    pub empty_body_defs: Vec<CoverageItem>,
    pub no_member_defs: Vec<CoverageItem>,
    pub unsatisfied_reqs: Vec<CoverageItem>,
    pub unverified_reqs: Vec<CoverageItem>,
    pub summary: CoverageSummary,
}

/// An item flagged in the coverage report.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageItem {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

/// Coverage summary percentages.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub total_defs: usize,
    pub documented_pct: f64,
    pub typed_usages_pct: f64,
    pub populated_defs_pct: f64,
    pub req_satisfaction_pct: f64,
    pub req_verification_pct: f64,
    pub overall_score: f64,
}

/// Compute model completeness/coverage report.
pub fn coverage_report(model: &Model) -> CoverageReport {
    // Undocumented definitions (exclude packages, enums)
    let undocumented_defs: Vec<CoverageItem> = model
        .definitions
        .iter()
        .filter(|d| !matches!(d.kind, DefKind::Package | DefKind::Enum))
        .filter(|d| d.doc.is_none())
        .map(|d| CoverageItem {
            name: d.name.clone(),
            kind: d.kind.label().to_string(),
            line: d.span.start_row,
        })
        .collect();

    // Untyped usages
    let untyped_usages: Vec<CoverageItem> = model
        .usages
        .iter()
        .filter(|u| u.type_ref.is_none())
        .map(|u| CoverageItem {
            name: u.name.clone(),
            kind: u.kind.clone(),
            line: u.span.start_row,
        })
        .collect();

    // Empty-body definitions (no body block)
    let empty_body_defs: Vec<CoverageItem> = model
        .definitions
        .iter()
        .filter(|d| !d.has_body && !matches!(d.kind, DefKind::Package | DefKind::Enum))
        .map(|d| CoverageItem {
            name: d.name.clone(),
            kind: d.kind.label().to_string(),
            line: d.span.start_row,
        })
        .collect();

    // Definitions with body but no members
    let no_member_defs: Vec<CoverageItem> = model
        .definitions
        .iter()
        .filter(|d| {
            d.has_body
                && !matches!(d.kind, DefKind::Package | DefKind::Enum)
                && model.usages_in_def(&d.name).is_empty()
        })
        .map(|d| CoverageItem {
            name: d.name.clone(),
            kind: d.kind.label().to_string(),
            line: d.span.start_row,
        })
        .collect();

    // Unsatisfied requirements
    let satisfied_names: HashSet<&str> = model
        .satisfactions
        .iter()
        .map(|s| simple_name(&s.requirement))
        .collect();
    let unsatisfied_reqs: Vec<CoverageItem> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Requirement)
        .filter(|d| !satisfied_names.contains(d.name.as_str()))
        .map(|d| CoverageItem {
            name: d.name.clone(),
            kind: "requirement def".to_string(),
            line: d.span.start_row,
        })
        .collect();

    // Unverified requirements
    let verified_names: HashSet<&str> = model
        .verifications
        .iter()
        .map(|v| simple_name(&v.requirement))
        .collect();
    let unverified_reqs: Vec<CoverageItem> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Requirement)
        .filter(|d| !verified_names.contains(d.name.as_str()))
        .map(|d| CoverageItem {
            name: d.name.clone(),
            kind: "requirement def".to_string(),
            line: d.span.start_row,
        })
        .collect();

    // Summary percentages
    let non_pkg_defs: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| !matches!(d.kind, DefKind::Package | DefKind::Enum))
        .collect();
    let total_defs = non_pkg_defs.len();
    let documented_pct = if total_defs > 0 {
        100.0 * (total_defs - undocumented_defs.len()) as f64 / total_defs as f64
    } else {
        100.0
    };

    let total_usages = model.usages.len();
    let typed_usages_pct = if total_usages > 0 {
        100.0 * (total_usages - untyped_usages.len()) as f64 / total_usages as f64
    } else {
        100.0
    };

    let defs_with_body = non_pkg_defs.iter().filter(|d| d.has_body).count();
    let populated_defs_pct = if defs_with_body > 0 {
        100.0 * (defs_with_body - no_member_defs.len()) as f64 / defs_with_body as f64
    } else {
        100.0
    };

    let req_count = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Requirement)
        .count();
    let req_satisfaction_pct = if req_count > 0 {
        100.0 * (req_count - unsatisfied_reqs.len()) as f64 / req_count as f64
    } else {
        100.0
    };
    let req_verification_pct = if req_count > 0 {
        100.0 * (req_count - unverified_reqs.len()) as f64 / req_count as f64
    } else {
        100.0
    };

    // Weighted overall score
    let overall_score =
        documented_pct * 0.25 + typed_usages_pct * 0.25 + req_satisfaction_pct * 0.25 + req_verification_pct * 0.25;

    CoverageReport {
        undocumented_defs,
        untyped_usages,
        empty_body_defs,
        no_member_defs,
        unsatisfied_reqs,
        unverified_reqs,
        summary: CoverageSummary {
            total_defs,
            documented_pct,
            typed_usages_pct,
            populated_defs_pct,
            req_satisfaction_pct,
            req_verification_pct,
            overall_score,
        },
    }
}

/// Get enum member choices for use in wizard prompts.
/// Returns (member_name, doc) pairs from the named enum definition.
pub fn get_enum_choices(model: &Model, enum_name: &str) -> Vec<(String, Option<String>)> {
    model
        .definitions
        .iter()
        .find(|d| d.kind == DefKind::Enum && d.name == enum_name)
        .map(|d| {
            d.enum_members
                .iter()
                .map(|m| (m.name.clone(), m.doc.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn list_all_definitions() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
            port def DataPort;
        "#,
        );
        let results = list_elements(&model, &ListFilter::default());
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn list_filter_by_kind() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            port def DataPort;
            part def Engine;
        "#,
        );
        let filter = ListFilter {
            kind: Some(KindFilter::DefKind(DefKind::Part)),
            ..Default::default()
        };
        let results = list_elements(&model, &filter);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.kind_label() == "part def"));
    }

    #[test]
    fn list_filter_by_name() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def VehicleConfig;
            part def Engine;
        "#,
        );
        let filter = ListFilter {
            name_pattern: Some("Vehicle".to_string()),
            ..Default::default()
        };
        let results = list_elements(&model, &filter);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_filter_by_parent() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                part engine : Engine;
                part wheels : Wheel;
            }
            part def Standalone;
        "#,
        );
        let filter = ListFilter {
            kind: Some(KindFilter::Usages),
            parent: Some("Vehicle".to_string()),
            ..Default::default()
        };
        let results = list_elements(&model, &filter);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_unused_only() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Engine;
            part def Vehicle {
                part engine : Engine;
            }
            part def Orphan;
        "#,
        );
        let filter = ListFilter {
            unused_only: true,
            ..Default::default()
        };
        let results = list_elements(&model, &filter);
        let names: Vec<&str> = results.iter().map(|e| e.name()).collect();
        // Vehicle is used (Engine types to it... actually Engine is used by Vehicle, so Orphan + Vehicle are unused)
        assert!(
            names.contains(&"Orphan"),
            "Orphan should be unused, got: {:?}",
            names
        );
        assert!(
            !names.contains(&"Engine"),
            "Engine should not be unused, got: {:?}",
            names
        );
    }

    #[test]
    fn list_abstract_only() {
        let model = parse_file(
            "test.sysml",
            r#"
            abstract part def Base;
            part def Concrete;
        "#,
        );
        let filter = ListFilter {
            abstract_only: true,
            ..Default::default()
        };
        let results = list_elements(&model, &filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name(), "Base");
    }

    #[test]
    fn trace_requirements_empty() {
        let model = parse_file("test.sysml", "part def Vehicle;");
        let rows = trace_requirements(&model);
        assert!(rows.is_empty());
    }

    #[test]
    fn trace_requirements_satisfied() {
        let model = parse_file(
            "test.sysml",
            r#"
            requirement def MassReq {
                doc /* mass < 2000 */
            }
            part def Vehicle {
                satisfy MassReq;
            }
        "#,
        );
        let rows = trace_requirements(&model);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requirement, "MassReq");
        assert!(!rows[0].satisfied_by.is_empty());
    }

    #[test]
    fn trace_coverage_metrics() {
        let rows = vec![
            TraceRow {
                requirement: "R1".into(),
                satisfied_by: vec!["A".into()],
                verified_by: vec!["T1".into()],
            },
            TraceRow {
                requirement: "R2".into(),
                satisfied_by: vec!["B".into()],
                verified_by: vec![],
            },
            TraceRow {
                requirement: "R3".into(),
                satisfied_by: vec![],
                verified_by: vec![],
            },
        ];
        let cov = trace_coverage(&rows);
        assert_eq!(cov.total_requirements, 3);
        assert_eq!(cov.satisfied_count, 2);
        assert_eq!(cov.verified_count, 1);
        assert_eq!(cov.fully_traced_count, 1);
    }

    #[test]
    fn unconnected_ports_found() {
        let model = parse_file(
            "test.sysml",
            r#"
            port def FuelPort;
            part def Vehicle {
                port fuelIn : FuelPort;
                port dataOut : DataPort;
            }
        "#,
        );
        let unconn = unconnected_ports(&model);
        assert_eq!(unconn.len(), 2, "Both ports are unconnected");
    }

    #[test]
    fn list_ports_with_owner() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                port fuelIn : FuelPort;
            }
            part def Station {
                port fuelOut : FuelPort;
            }
        "#,
        );
        let ports = list_ports(&model);
        assert_eq!(ports.len(), 2);
        assert!(ports.iter().any(|p| p.name == "fuelIn" && p.owner == "Vehicle"));
        assert!(ports.iter().any(|p| p.name == "fuelOut" && p.owner == "Station"));
    }

    #[test]
    fn view_filter_with_expose() {
        let model = parse_file(
            "test.sysml",
            r#"
            package Pkg {
                part def Vehicle;
                part def Engine;
            }
            part def Unrelated;
            view def PkgView {
                expose Pkg::*;
            }
        "#,
        );
        assert!(!model.views.is_empty());
        let vf = filter_from_view(&model, "PkgView").unwrap();
        assert_eq!(vf.parent.as_deref(), Some("Pkg"));
    }

    #[test]
    fn parse_kind_filter_values() {
        assert_eq!(parse_kind_filter("parts"), Some(KindFilter::DefKind(DefKind::Part)));
        assert_eq!(parse_kind_filter("port"), Some(KindFilter::UsageKind("port".to_string())));
        assert_eq!(parse_kind_filter("all"), Some(KindFilter::All));
        assert_eq!(parse_kind_filter("nonsense"), None);
    }

    // ================================================================
    // stats tests
    // ================================================================

    #[test]
    fn stats_counts_definitions_and_usages() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                part engine : Engine;
                part wheels : Wheel;
            }
            part def Engine;
            port def DataPort;
        "#,
        );
        let stats = model_stats(&model);
        assert_eq!(stats.total_definitions, 3);
        assert_eq!(stats.total_usages, 2);
        assert!(stats.def_counts.iter().any(|(k, c)| k == "part def" && *c == 2));
        assert!(stats.def_counts.iter().any(|(k, c)| k == "port def" && *c == 1));
    }

    #[test]
    fn stats_doc_coverage() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                doc /* A vehicle */
            }
            part def Engine;
        "#,
        );
        let stats = model_stats(&model);
        assert_eq!(stats.doc_coverage.documented, 1);
        assert_eq!(stats.doc_coverage.total, 2);
        assert!((stats.doc_coverage.percentage - 50.0).abs() < 0.1);
    }

    #[test]
    fn stats_nesting_depth() {
        let model = parse_file(
            "test.sysml",
            r#"
            package P {
                part def Vehicle {
                    part engine : Engine;
                }
            }
        "#,
        );
        let stats = model_stats(&model);
        assert!(stats.max_nesting_depth >= 1, "depth={}", stats.max_nesting_depth);
    }

    // ================================================================
    // deps tests
    // ================================================================

    #[test]
    fn deps_finds_type_references() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Engine;
            part def Vehicle {
                part engine : Engine;
            }
        "#,
        );
        let deps = dependency_analysis(&model, "Engine");
        assert!(
            deps.referenced_by.iter().any(|r| r.name == "engine" && r.relationship == "type_ref"),
            "expected engine type_ref, got {:?}",
            deps.referenced_by
        );
    }

    #[test]
    fn deps_finds_specialization() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Base;
            part def Derived :> Base;
        "#,
        );
        let deps = dependency_analysis(&model, "Base");
        assert!(
            deps.referenced_by.iter().any(|r| r.name == "Derived" && r.relationship == "specializes"),
            "got {:?}",
            deps.referenced_by
        );
    }

    #[test]
    fn deps_forward_dependencies() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Engine;
            part def Wheel;
            part def Vehicle {
                part engine : Engine;
                part wheels : Wheel;
            }
        "#,
        );
        let deps = dependency_analysis(&model, "Vehicle");
        assert!(
            deps.depends_on.iter().any(|r| r.name == "Engine"),
            "got {:?}",
            deps.depends_on
        );
        assert!(
            deps.depends_on.iter().any(|r| r.name == "Wheel"),
            "got {:?}",
            deps.depends_on
        );
    }

    // ================================================================
    // diff tests
    // ================================================================

    #[test]
    fn diff_detects_added_and_removed_defs() {
        let old = parse_file("old.sysml", "part def Vehicle;\npart def Engine;\n");
        let new = parse_file("new.sysml", "part def Vehicle;\npart def Motor;\n");
        let diff = model_diff(&old, &new);
        assert!(diff.added_defs.contains(&"Motor".to_string()));
        assert!(diff.removed_defs.contains(&"Engine".to_string()));
        assert!(diff.changed_defs.is_empty());
    }

    #[test]
    fn diff_detects_changed_supertype() {
        let old = parse_file("old.sysml", "part def Vehicle :> Base;\n");
        let new = parse_file("new.sysml", "part def Vehicle :> NewBase;\n");
        let diff = model_diff(&old, &new);
        assert_eq!(diff.changed_defs.len(), 1);
        assert_eq!(diff.changed_defs[0].name, "Vehicle");
        assert!(diff.changed_defs[0].changes.iter().any(|c| c.contains("super_type")));
    }

    #[test]
    fn diff_detects_added_usage() {
        let old = parse_file("old.sysml", "part def Vehicle {\n    part engine : Engine;\n}\n");
        let new = parse_file("new.sysml", "part def Vehicle {\n    part engine : Engine;\n    part wheels : Wheel;\n}\n");
        let diff = model_diff(&old, &new);
        assert!(
            diff.added_usages.iter().any(|u| u.name == "wheels"),
            "got {:?}",
            diff.added_usages
        );
    }

    // ================================================================
    // allocation tests
    // ================================================================

    #[test]
    fn allocation_report_empty() {
        let model = parse_file("test.sysml", "part def Vehicle;\n");
        let report = allocation_report(&model);
        assert!(report.rows.is_empty());
        assert_eq!(report.total_allocations, 0);
    }

    #[test]
    fn allocation_report_with_allocations() {
        let model = parse_file(
            "test.sysml",
            r#"
            action def ProcessOrder;
            part def OrderSystem;
            allocate ProcessOrder to OrderSystem;
        "#,
        );
        let report = allocation_report(&model);
        assert_eq!(report.total_allocations, 1);
        assert_eq!(report.rows[0].source, "ProcessOrder");
        assert_eq!(report.rows[0].target, "OrderSystem");
    }

    #[test]
    fn allocation_finds_unallocated() {
        let model = parse_file(
            "test.sysml",
            r#"
            action def ProcessOrder;
            action def ShipOrder;
            part def OrderSystem;
            part def Warehouse;
            allocate ProcessOrder to OrderSystem;
        "#,
        );
        let report = allocation_report(&model);
        assert!(
            report.unallocated_sources.contains(&"ShipOrder".to_string()),
            "got {:?}",
            report.unallocated_sources
        );
        assert!(
            report.unallocated_targets.contains(&"Warehouse".to_string()),
            "got {:?}",
            report.unallocated_targets
        );
    }

    // ================================================================
    // coverage tests
    // ================================================================

    #[test]
    fn coverage_finds_undocumented() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                doc /* A vehicle */
            }
            part def Engine;
        "#,
        );
        let report = coverage_report(&model);
        assert!(
            report.undocumented_defs.iter().any(|i| i.name == "Engine"),
            "got {:?}",
            report.undocumented_defs
        );
        assert!(
            !report.undocumented_defs.iter().any(|i| i.name == "Vehicle"),
            "Vehicle should be documented"
        );
    }

    #[test]
    fn coverage_finds_untyped_usages() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                part engine : Engine;
                part trailer;
            }
        "#,
        );
        let report = coverage_report(&model);
        assert!(
            report.untyped_usages.iter().any(|i| i.name == "trailer"),
            "got {:?}",
            report.untyped_usages
        );
    }

    #[test]
    fn get_enum_choices_returns_members() {
        let model = parse_file("test.sysml", r#"
            enum def Color {
                enum red;
                enum green;
                enum blue;
            }
        "#);
        let choices = get_enum_choices(&model, "Color");
        let names: Vec<&str> = choices.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"red"), "Should contain red, got {:?}", names);
        assert!(names.contains(&"green"), "Should contain green, got {:?}", names);
        assert!(names.contains(&"blue"), "Should contain blue, got {:?}", names);
    }

    #[test]
    fn get_enum_choices_missing_enum() {
        let model = parse_file("test.sysml", "part def Vehicle;");
        let choices = get_enum_choices(&model, "NonExistent");
        assert!(choices.is_empty());
    }

    #[test]
    fn coverage_overall_score() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                doc /* A vehicle */
                part engine : Engine;
            }
        "#,
        );
        let report = coverage_report(&model);
        // Should be between 0 and 100
        assert!(report.summary.overall_score >= 0.0);
        assert!(report.summary.overall_score <= 100.0);
    }
}
