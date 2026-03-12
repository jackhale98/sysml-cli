//! Bill of materials domain for SysML v2 models.
//!
//! Extracts a hierarchical BOM tree from a parsed [`Model`] by walking the
//! composition hierarchy (part usages inside part definitions). Provides
//! mass/cost rollup, flattening, where-used queries, and text/CSV export.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sysml_core::model::{DefKind, Model};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Classification of a part in the BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartCategory {
    Assembly,
    Subassembly,
    Component,
    RawMaterial,
    Fastener,
    Consumable,
    Software,
    Document,
}

impl std::fmt::Display for PartCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Assembly => "Assembly",
            Self::Subassembly => "Subassembly",
            Self::Component => "Component",
            Self::RawMaterial => "Raw Material",
            Self::Fastener => "Fastener",
            Self::Consumable => "Consumable",
            Self::Software => "Software",
            Self::Document => "Document",
        };
        write!(f, "{label}")
    }
}

/// Lifecycle phase of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Concept,
    Development,
    Prototype,
    Production,
    Obsolete,
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Concept => "Concept",
            Self::Development => "Development",
            Self::Prototype => "Prototype",
            Self::Production => "Production",
            Self::Obsolete => "Obsolete",
        };
        write!(f, "{label}")
    }
}

/// Make-or-buy decision for a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MakeOrBuy {
    Make,
    Buy,
    MakeAndBuy,
    Tbd,
}

impl std::fmt::Display for MakeOrBuy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Make => "Make",
            Self::Buy => "Buy",
            Self::MakeAndBuy => "Make & Buy",
            Self::Tbd => "TBD",
        };
        write!(f, "{label}")
    }
}

// ---------------------------------------------------------------------------
// Property structs
// ---------------------------------------------------------------------------

/// Part identification metadata.
#[derive(Debug, Clone, Serialize)]
pub struct PartIdentity {
    pub part_number: String,
    pub revision: String,
    pub description: String,
    pub category: PartCategory,
    pub lifecycle_state: LifecycleState,
    pub make_or_buy: MakeOrBuy,
}

/// Mass property for a part.
#[derive(Debug, Clone, Serialize)]
pub struct MassProperty {
    /// Mass in kilograms.
    pub mass_kg: f64,
    /// How the value was obtained: "actual", "estimated", "calculated", or "allocated".
    pub mass_type: String,
    /// Margin percentage (e.g. 10.0 means 10%).
    pub margin_pct: f64,
}

/// Cost property for a part.
#[derive(Debug, Clone, Serialize)]
pub struct CostProperty {
    /// Per-unit recurring cost.
    pub unit_cost: f64,
    /// One-time tooling / NRE cost.
    pub tooling_cost: f64,
    /// Basis description (e.g. "supplier quote", "parametric estimate").
    pub cost_basis: String,
    /// ISO 4217 currency code.
    pub currency: String,
}

// ---------------------------------------------------------------------------
// BOM tree
// ---------------------------------------------------------------------------

/// A single node in the hierarchical bill of materials.
#[derive(Debug, Clone, Serialize)]
pub struct BomNode {
    /// Usage name (instance name).
    pub name: String,
    /// Definition name (type).
    pub definition: String,
    /// Quantity required (from multiplicity, defaults to 1).
    pub quantity: u32,
    pub identity: Option<PartIdentity>,
    pub mass: Option<MassProperty>,
    pub cost: Option<CostProperty>,
    pub children: Vec<BomNode>,
}

/// Aggregate statistics for a BOM tree.
#[derive(Debug, Clone, Serialize)]
pub struct BomSummary {
    pub total_parts: usize,
    pub unique_parts: usize,
    pub total_mass_kg: Option<f64>,
    pub total_cost: Option<f64>,
    pub max_depth: usize,
}

/// A flattened BOM row for tabular export.
#[derive(Debug, Clone, Serialize)]
pub struct FlatBomRow {
    pub level: usize,
    pub name: String,
    pub definition: String,
    /// Cumulative quantity through the tree path.
    pub quantity: u32,
    pub part_number: Option<String>,
    pub revision: Option<String>,
    pub description: Option<String>,
    pub category: Option<PartCategory>,
}

// ---------------------------------------------------------------------------
// Attribute extraction helpers
// ---------------------------------------------------------------------------

/// Try to parse a quantity from a SysML v2 multiplicity.
///
/// Uses the lower bound if present, otherwise defaults to 1.
fn quantity_from_multiplicity(mult: &sysml_core::model::Multiplicity) -> u32 {
    if let Some(lo) = &mult.lower {
        lo.parse::<u32>().unwrap_or(1)
    } else if let Some(up) = &mult.upper {
        up.parse::<u32>().unwrap_or(1)
    } else {
        1
    }
}

/// Extract a [`PartIdentity`] from attribute usages nested in a definition.
fn extract_identity(model: &Model, def_name: &str) -> Option<PartIdentity> {
    let attrs = attribute_map(model, def_name);
    // Require at minimum a part_number attribute to build an identity.
    let part_number = attrs.get("part_number").or_else(|| attrs.get("partNumber"))?;
    Some(PartIdentity {
        part_number: part_number.clone(),
        revision: attrs
            .get("revision")
            .cloned()
            .unwrap_or_else(|| "A".to_string()),
        description: attrs
            .get("description")
            .cloned()
            .unwrap_or_default(),
        category: parse_category(attrs.get("category").map(|s| s.as_str())),
        lifecycle_state: parse_lifecycle(attrs.get("lifecycle_state").or_else(|| attrs.get("lifecycleState")).map(|s| s.as_str())),
        make_or_buy: parse_make_or_buy(attrs.get("make_or_buy").or_else(|| attrs.get("makeOrBuy")).map(|s| s.as_str())),
    })
}

/// Extract a [`MassProperty`] from attribute usages nested in a definition.
fn extract_mass(model: &Model, def_name: &str) -> Option<MassProperty> {
    let attrs = attribute_map(model, def_name);
    let mass_kg = attrs.get("mass_kg").or_else(|| attrs.get("mass"))
        .and_then(|v| v.parse::<f64>().ok())?;
    Some(MassProperty {
        mass_kg,
        mass_type: attrs
            .get("mass_type")
            .cloned()
            .unwrap_or_else(|| "estimated".to_string()),
        margin_pct: attrs
            .get("margin_pct")
            .or_else(|| attrs.get("mass_margin"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0),
    })
}

/// Extract a [`CostProperty`] from attribute usages nested in a definition.
fn extract_cost(model: &Model, def_name: &str) -> Option<CostProperty> {
    let attrs = attribute_map(model, def_name);
    let unit_cost = attrs.get("unit_cost").or_else(|| attrs.get("cost"))
        .and_then(|v| v.parse::<f64>().ok())?;
    Some(CostProperty {
        unit_cost,
        tooling_cost: attrs
            .get("tooling_cost")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0),
        cost_basis: attrs
            .get("cost_basis")
            .cloned()
            .unwrap_or_else(|| "estimate".to_string()),
        currency: attrs
            .get("currency")
            .cloned()
            .unwrap_or_else(|| "USD".to_string()),
    })
}

/// Collect attribute usages inside a definition as a name→value map.
///
/// Only usages whose kind starts with "attribute" and that have a
/// `value_expr` are included.  Surrounding quotes are stripped.
fn attribute_map(model: &Model, def_name: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for u in &model.usages {
        if u.parent_def.as_deref() == Some(def_name) && u.kind.starts_with("attribute") {
            if let Some(val) = &u.value_expr {
                let cleaned = val.trim().trim_matches('"').trim_matches('\'').to_string();
                map.insert(u.name.clone(), cleaned);
            }
        }
    }
    map
}

fn parse_category(s: Option<&str>) -> PartCategory {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("assembly") => PartCategory::Assembly,
        Some("subassembly") => PartCategory::Subassembly,
        Some("component") => PartCategory::Component,
        Some("raw_material" | "rawmaterial") => PartCategory::RawMaterial,
        Some("fastener") => PartCategory::Fastener,
        Some("consumable") => PartCategory::Consumable,
        Some("software") => PartCategory::Software,
        Some("document") => PartCategory::Document,
        _ => PartCategory::Component,
    }
}

fn parse_lifecycle(s: Option<&str>) -> LifecycleState {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("concept") => LifecycleState::Concept,
        Some("development") => LifecycleState::Development,
        Some("prototype") => LifecycleState::Prototype,
        Some("production") => LifecycleState::Production,
        Some("obsolete") => LifecycleState::Obsolete,
        _ => LifecycleState::Development,
    }
}

fn parse_make_or_buy(s: Option<&str>) -> MakeOrBuy {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("make") => MakeOrBuy::Make,
        Some("buy") => MakeOrBuy::Buy,
        Some("make_and_buy" | "makeandbuy") => MakeOrBuy::MakeAndBuy,
        Some("tbd") => MakeOrBuy::Tbd,
        _ => MakeOrBuy::Tbd,
    }
}

// ---------------------------------------------------------------------------
// Core BOM functions
// ---------------------------------------------------------------------------

/// Build a hierarchical BOM tree starting from the named part definition.
///
/// Returns `None` if the root definition does not exist or is not a part def.
/// Walks the composition hierarchy: for each usage whose `parent_def` matches
/// the current definition and whose `type_ref` points to another part def, a
/// child [`BomNode`] is created recursively.
pub fn build_bom_tree(model: &Model, root: &str) -> Option<BomNode> {
    let def = model.definitions.iter().find(|d| d.name == root)?;
    if def.kind != DefKind::Part && def.kind != DefKind::Package {
        return None;
    }

    // Track visited definitions to prevent infinite recursion from cycles.
    let mut visited = HashSet::new();
    Some(build_node(model, root, root, 1, &mut visited))
}

fn build_node(
    model: &Model,
    def_name: &str,
    usage_name: &str,
    quantity: u32,
    visited: &mut HashSet<String>,
) -> BomNode {
    visited.insert(def_name.to_string());

    let children: Vec<BomNode> = model
        .usages
        .iter()
        .filter(|u| {
            u.parent_def.as_deref() == Some(def_name)
                && u.type_ref.is_some()
                && is_part_usage(u)
        })
        .filter_map(|u| {
            let type_name = u.type_ref.as_deref()?;
            // Only recurse into definitions that exist and are part defs.
            let child_def = model.definitions.iter().find(|d| d.name == type_name)?;
            if child_def.kind != DefKind::Part {
                return None;
            }
            let qty = u
                .multiplicity
                .as_ref()
                .map(|m| quantity_from_multiplicity(m))
                .unwrap_or(1);
            if visited.contains(type_name) {
                // Break cycles — emit a leaf with no children.
                Some(BomNode {
                    name: u.name.clone(),
                    definition: type_name.to_string(),
                    quantity: qty,
                    identity: extract_identity(model, type_name),
                    mass: extract_mass(model, type_name),
                    cost: extract_cost(model, type_name),
                    children: Vec::new(),
                })
            } else {
                Some(build_node(model, type_name, &u.name, qty, visited))
            }
        })
        .collect();

    // After recursing children, remove self so sibling branches can still reference this def.
    visited.remove(def_name);

    BomNode {
        name: usage_name.to_string(),
        definition: def_name.to_string(),
        quantity,
        identity: extract_identity(model, def_name),
        mass: extract_mass(model, def_name),
        cost: extract_cost(model, def_name),
        children,
    }
}

/// Whether a usage looks like a part composition (part usage or item usage).
fn is_part_usage(u: &sysml_core::model::Usage) -> bool {
    let k = u.kind.as_str();
    k == "part" || k == "part usage" || k == "item" || k == "item usage"
}

/// Flatten a BOM tree into a list of rows, each annotated with its level in
/// the hierarchy. Quantities are cumulative (multiplied through ancestors).
pub fn flatten_bom(node: &BomNode) -> Vec<FlatBomRow> {
    let mut rows = Vec::new();
    flatten_recursive(node, 0, 1, &mut rows);
    rows
}

fn flatten_recursive(node: &BomNode, level: usize, parent_qty: u32, rows: &mut Vec<FlatBomRow>) {
    let cumulative_qty = parent_qty * node.quantity;
    rows.push(FlatBomRow {
        level,
        name: node.name.clone(),
        definition: node.definition.clone(),
        quantity: cumulative_qty,
        part_number: node.identity.as_ref().map(|id| id.part_number.clone()),
        revision: node.identity.as_ref().map(|id| id.revision.clone()),
        description: node.identity.as_ref().map(|id| id.description.clone()),
        category: node.identity.as_ref().map(|id| id.category),
    });
    for child in &node.children {
        flatten_recursive(child, level + 1, cumulative_qty, rows);
    }
}

/// Recursive mass rollup: `quantity * mass + sum(children rollup)`.
///
/// If a node has no mass property the node itself contributes 0 but its
/// children are still rolled up.
pub fn mass_rollup(node: &BomNode) -> f64 {
    let own = node
        .mass
        .as_ref()
        .map(|m| {
            let with_margin = m.mass_kg * (1.0 + m.margin_pct / 100.0);
            f64::from(node.quantity) * with_margin
        })
        .unwrap_or(0.0);
    let children_mass: f64 = node.children.iter().map(|c| mass_rollup(c)).sum();
    own + f64::from(node.quantity) * children_mass
}

/// Recursive cost rollup returning `(recurring_cost, tooling_cost)`.
///
/// Recurring costs are scaled by quantity through the tree; tooling costs are
/// incurred once per unique definition regardless of quantity.
pub fn cost_rollup(node: &BomNode) -> (f64, f64) {
    let own_recurring = node
        .cost
        .as_ref()
        .map(|c| f64::from(node.quantity) * c.unit_cost)
        .unwrap_or(0.0);
    let own_tooling = node
        .cost
        .as_ref()
        .map(|c| c.tooling_cost)
        .unwrap_or(0.0);

    let mut total_recurring = own_recurring;
    let mut total_tooling = own_tooling;

    for child in &node.children {
        let (cr, ct) = cost_rollup(child);
        total_recurring += f64::from(node.quantity) * cr;
        total_tooling += ct;
    }

    (total_recurring, total_tooling)
}

/// Reverse lookup: find all definitions that contain `part_name` as a usage.
pub fn where_used(model: &Model, part_name: &str) -> Vec<String> {
    let mut parents = Vec::new();
    let mut seen = HashSet::new();
    for u in &model.usages {
        let type_matches = u.type_ref.as_deref() == Some(part_name);
        if type_matches {
            if let Some(parent) = &u.parent_def {
                if seen.insert(parent.clone()) {
                    parents.push(parent.clone());
                }
            }
        }
    }
    parents.sort();
    parents
}

/// Compute aggregate summary statistics for a BOM tree.
pub fn bom_summary(node: &BomNode) -> BomSummary {
    let mut total = 0usize;
    let mut unique = HashSet::new();
    let mut has_mass = false;
    let mut has_cost = false;
    let depth = tree_depth(node);

    count_parts(node, 1, &mut total, &mut unique, &mut has_mass, &mut has_cost);

    let total_mass = if has_mass {
        Some(mass_rollup(node))
    } else {
        None
    };

    let (recurring, tooling) = cost_rollup(node);
    let total_cost = if has_cost {
        Some(recurring + tooling)
    } else {
        None
    };

    BomSummary {
        total_parts: total,
        unique_parts: unique.len(),
        total_mass_kg: total_mass,
        total_cost,
        max_depth: depth,
    }
}

fn count_parts(
    node: &BomNode,
    parent_qty: u32,
    total: &mut usize,
    unique: &mut HashSet<String>,
    has_mass: &mut bool,
    has_cost: &mut bool,
) {
    let cumulative = parent_qty * node.quantity;
    *total += cumulative as usize;
    unique.insert(node.definition.clone());
    if node.mass.is_some() {
        *has_mass = true;
    }
    if node.cost.is_some() {
        *has_cost = true;
    }
    for child in &node.children {
        count_parts(child, cumulative, total, unique, has_mass, has_cost);
    }
}

fn tree_depth(node: &BomNode) -> usize {
    if node.children.is_empty() {
        1
    } else {
        1 + node.children.iter().map(|c| tree_depth(c)).max().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// Render an indented text tree of the BOM.
///
/// Optionally includes mass and cost columns.
pub fn format_bom_tree(node: &BomNode, include_mass: bool, include_cost: bool) -> String {
    let mut buf = String::new();
    format_tree_recursive(node, 0, include_mass, include_cost, &mut buf);
    buf
}

fn format_tree_recursive(
    node: &BomNode,
    level: usize,
    include_mass: bool,
    include_cost: bool,
    buf: &mut String,
) {
    let indent = "  ".repeat(level);
    let qty_str = if node.quantity > 1 {
        format!(" x{}", node.quantity)
    } else {
        String::new()
    };
    let pn_str = node
        .identity
        .as_ref()
        .map(|id| format!(" [{}]", id.part_number))
        .unwrap_or_default();

    let mut line = format!("{indent}{name} : {def}{qty}{pn}",
        indent = indent,
        name = node.name,
        def = node.definition,
        qty = qty_str,
        pn = pn_str,
    );

    if include_mass {
        if let Some(m) = &node.mass {
            line.push_str(&format!("  mass={:.3}kg", m.mass_kg));
        }
    }

    if include_cost {
        if let Some(c) = &node.cost {
            line.push_str(&format!("  cost={:.2}{}", c.unit_cost, c.currency));
        }
    }

    buf.push_str(&line);
    buf.push('\n');

    for child in &node.children {
        format_tree_recursive(child, level + 1, include_mass, include_cost, buf);
    }
}

/// Export a flattened BOM as CSV text.
///
/// Columns: Level, Name, Definition, Quantity, PartNumber, Revision, Description, Category
pub fn format_bom_csv(node: &BomNode) -> String {
    let rows = flatten_bom(node);
    let mut buf = String::from("Level,Name,Definition,Quantity,PartNumber,Revision,Description,Category\n");
    for row in &rows {
        buf.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            row.level,
            csv_escape(&row.name),
            csv_escape(&row.definition),
            row.quantity,
            csv_escape(&row.part_number.as_deref().unwrap_or("")),
            csv_escape(&row.revision.as_deref().unwrap_or("")),
            csv_escape(&row.description.as_deref().unwrap_or("")),
            row.category.map(|c| c.to_string()).unwrap_or_default(),
        ));
    }
    buf
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// SysML generation
// ---------------------------------------------------------------------------

/// A row in a costed BOM report.
#[derive(Debug, Clone, Serialize)]
pub struct CostedBomRow {
    pub level: usize,
    pub name: String,
    pub definition: String,
    /// Per-unit quantity in the BOM.
    pub bom_qty: u32,
    /// Cumulative quantity for the production order.
    pub total_qty: u32,
    /// Resolved unit price from quote (if available).
    pub unit_price: Option<f64>,
    /// Extended price = total_qty × unit_price.
    pub extended_price: Option<f64>,
    /// Supplier name from best quote.
    pub supplier: Option<String>,
    /// Currency code from quote.
    pub currency: Option<String>,
}

/// Build a costed BOM by walking the BOM tree and resolving quotes at the given
/// production quantity. Returns rows with pricing and a total cost.
///
/// `order_qty` is the number of top-level assemblies being produced. Each part's
/// total demand is `bom_qty × order_qty`, and the quote price break is resolved
/// at that demand level.
pub fn costed_bom(
    tree: &BomNode,
    quotes: &[sysml_source::Quote],
    order_qty: u32,
) -> (Vec<CostedBomRow>, f64) {
    let mut rows = Vec::new();
    let mut total_cost = 0.0;
    costed_bom_recursive(tree, quotes, order_qty, 0, 1, &mut rows, &mut total_cost);
    (rows, total_cost)
}

fn costed_bom_recursive(
    node: &BomNode,
    quotes: &[sysml_source::Quote],
    order_qty: u32,
    level: usize,
    parent_qty: u32,
    rows: &mut Vec<CostedBomRow>,
    total_cost: &mut f64,
) {
    let bom_qty = parent_qty * node.quantity;
    let total_qty = bom_qty * order_qty;

    let best = sysml_source::best_quote_for_part(quotes, &node.definition, total_qty);
    let unit_price = best.and_then(|q| q.unit_price_at(total_qty));
    let extended = unit_price.map(|p| p * total_qty as f64);
    if let Some(ext) = extended {
        *total_cost += ext;
    }

    rows.push(CostedBomRow {
        level,
        name: node.name.clone(),
        definition: node.definition.clone(),
        bom_qty,
        total_qty,
        unit_price,
        extended_price: extended,
        supplier: best.map(|q| q.supplier_name.clone()),
        currency: best.map(|q| q.currency.clone()),
    });

    for child in &node.children {
        costed_bom_recursive(child, quotes, order_qty, level + 1, bom_qty, rows, total_cost);
    }
}

/// Format a costed BOM as a text table.
pub fn format_costed_bom(rows: &[CostedBomRow], total_cost: f64) -> String {
    let mut buf = String::new();
    buf.push_str(&format!(
        "{:<6} {:<20} {:<20} {:>8} {:>10} {:>12} {:>14} {:<15}\n",
        "Level", "Name", "Definition", "BOM Qty", "Total Qty", "Unit Price", "Extended", "Supplier",
    ));
    buf.push_str(&format!("{}\n", "-".repeat(109)));

    for row in rows {
        let indent = "  ".repeat(row.level);
        let price_str = row.unit_price.map(|p| format!("{:.4}", p)).unwrap_or_else(|| "-".into());
        let ext_str = row.extended_price.map(|p| format!("{:.2}", p)).unwrap_or_else(|| "-".into());
        let supplier = row.supplier.as_deref().unwrap_or("-");
        buf.push_str(&format!(
            "{:<6} {:<20} {:<20} {:>8} {:>10} {:>12} {:>14} {:<15}\n",
            row.level,
            truncate_str(&format!("{}{}", indent, row.name), 19),
            truncate_str(&row.definition, 19),
            row.bom_qty,
            row.total_qty,
            price_str,
            ext_str,
            truncate_str(supplier, 14),
        ));
    }

    buf.push_str(&format!("{}\n", "-".repeat(109)));
    buf.push_str(&format!("{:>78} {:>14.2}\n", "TOTAL:", total_cost));
    buf
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

/// Generate `attribute unitCost = <price>;` lines for each part definition
/// in the BOM, suitable for insertion into model files.
///
/// Returns a vec of (definition_name, sysml_attribute_line) pairs.
pub fn cost_attributes_from_costed_bom(rows: &[CostedBomRow]) -> Vec<(String, String)> {
    rows.iter()
        .filter_map(|row| {
            let price = row.unit_price?;
            Some((
                row.definition.clone(),
                format!("attribute unitCost = {:.4};", price),
            ))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::model::{Definition, DefKind, Model, Multiplicity, Span, Usage};

    fn default_span() -> Span {
        Span::default()
    }

    fn part_def(name: &str) -> Definition {
        Definition {
            kind: DefKind::Part,
            name: name.to_string(),
            super_type: None,
            span: default_span(),
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
        }
    }

    fn part_usage(name: &str, type_ref: &str, parent: &str) -> Usage {
        Usage {
            kind: "part".to_string(),
            name: name.to_string(),
            type_ref: Some(type_ref.to_string()),
            span: default_span(),
            direction: None,
            is_conjugated: false,
            parent_def: Some(parent.to_string()),
            multiplicity: None,
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        }
    }

    fn attr_usage(name: &str, value: &str, parent: &str) -> Usage {
        Usage {
            kind: "attribute".to_string(),
            name: name.to_string(),
            type_ref: None,
            span: default_span(),
            direction: None,
            is_conjugated: false,
            parent_def: Some(parent.to_string()),
            multiplicity: None,
            value_expr: Some(value.to_string()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        }
    }

    fn part_usage_with_mult(name: &str, type_ref: &str, parent: &str, qty: u32) -> Usage {
        Usage {
            kind: "part".to_string(),
            name: name.to_string(),
            type_ref: Some(type_ref.to_string()),
            span: default_span(),
            direction: None,
            is_conjugated: false,
            parent_def: Some(parent.to_string()),
            multiplicity: Some(Multiplicity {
                lower: Some(qty.to_string()),
                upper: Some(qty.to_string()),
                is_ordered: false,
                is_nonunique: false,
            }),
            value_expr: None,
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        }
    }

    /// Build a simple Vehicle -> Engine + Wheel model.
    fn vehicle_model() -> Model {
        let mut model = Model::new("test.sysml".to_string());
        model.definitions.push(part_def("Vehicle"));
        model.definitions.push(part_def("Engine"));
        model.definitions.push(part_def("Wheel"));
        model.usages.push(part_usage("engine", "Engine", "Vehicle"));
        model.usages.push(part_usage_with_mult("wheels", "Wheel", "Vehicle", 4));
        model
    }

    /// Build a deeper hierarchy: Vehicle -> Chassis -> Suspension, Wheel x4.
    fn deep_model() -> Model {
        let mut model = Model::new("deep.sysml".to_string());
        model.definitions.push(part_def("Vehicle"));
        model.definitions.push(part_def("Chassis"));
        model.definitions.push(part_def("Suspension"));
        model.definitions.push(part_def("Wheel"));
        model.usages.push(part_usage("chassis", "Chassis", "Vehicle"));
        model.usages.push(part_usage("suspension", "Suspension", "Chassis"));
        model.usages.push(part_usage_with_mult("wheels", "Wheel", "Chassis", 4));
        model
    }

    fn model_with_attrs() -> Model {
        let mut model = vehicle_model();
        // Add mass attributes
        model.usages.push(attr_usage("mass_kg", "1500.0", "Vehicle"));
        model.usages.push(attr_usage("mass_type", "estimated", "Vehicle"));
        model.usages.push(attr_usage("mass_kg", "200.0", "Engine"));
        model.usages.push(attr_usage("mass_kg", "12.5", "Wheel"));
        // Add cost attributes
        model.usages.push(attr_usage("unit_cost", "25000.0", "Vehicle"));
        model.usages.push(attr_usage("unit_cost", "5000.0", "Engine"));
        model.usages.push(attr_usage("tooling_cost", "100000.0", "Engine"));
        model.usages.push(attr_usage("unit_cost", "150.0", "Wheel"));
        // Add identity attributes
        model.usages.push(attr_usage("part_number", "VH-001", "Vehicle"));
        model.usages.push(attr_usage("description", "Main vehicle assembly", "Vehicle"));
        model.usages.push(attr_usage("category", "assembly", "Vehicle"));
        model.usages.push(attr_usage("part_number", "EN-100", "Engine"));
        model.usages.push(attr_usage("part_number", "WH-200", "Wheel"));
        model
    }

    // -- build_bom_tree -------------------------------------------------------

    #[test]
    fn build_simple_tree() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        assert_eq!(tree.name, "Vehicle");
        assert_eq!(tree.definition, "Vehicle");
        assert_eq!(tree.children.len(), 2);
    }

    #[test]
    fn build_tree_returns_none_for_missing_root() {
        let model = vehicle_model();
        assert!(build_bom_tree(&model, "NonExistent").is_none());
    }

    #[test]
    fn build_tree_returns_none_for_non_part_def() {
        let mut model = Model::new("test.sysml".to_string());
        model.definitions.push(Definition {
            kind: DefKind::Action,
            name: "DoSomething".to_string(),
            super_type: None,
            span: default_span(),
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
        assert!(build_bom_tree(&model, "DoSomething").is_none());
    }

    #[test]
    fn multiplicity_sets_quantity() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let wheel = tree.children.iter().find(|c| c.definition == "Wheel").unwrap();
        assert_eq!(wheel.quantity, 4);
    }

    #[test]
    fn default_quantity_is_one() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let engine = tree.children.iter().find(|c| c.definition == "Engine").unwrap();
        assert_eq!(engine.quantity, 1);
    }

    #[test]
    fn deep_hierarchy() {
        let model = deep_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        assert_eq!(tree.children.len(), 1); // chassis
        let chassis = &tree.children[0];
        assert_eq!(chassis.children.len(), 2); // suspension + wheels
    }

    #[test]
    fn cycle_detection() {
        let mut model = Model::new("cycle.sysml".to_string());
        model.definitions.push(part_def("A"));
        model.definitions.push(part_def("B"));
        model.usages.push(part_usage("b", "B", "A"));
        model.usages.push(part_usage("a", "A", "B"));
        let tree = build_bom_tree(&model, "A").unwrap();
        // B should appear as a child of A, but the back-ref A inside B should be a leaf.
        assert_eq!(tree.children.len(), 1);
        let b_node = &tree.children[0];
        assert_eq!(b_node.definition, "B");
        assert_eq!(b_node.children.len(), 1);
        assert!(b_node.children[0].children.is_empty()); // cycle broken
    }

    // -- identity extraction --------------------------------------------------

    #[test]
    fn extract_identity_from_attrs() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let id = tree.identity.as_ref().unwrap();
        assert_eq!(id.part_number, "VH-001");
        assert_eq!(id.category, PartCategory::Assembly);
        assert_eq!(id.description, "Main vehicle assembly");
    }

    #[test]
    fn identity_absent_without_part_number() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        assert!(tree.identity.is_none());
    }

    // -- mass/cost extraction -------------------------------------------------

    #[test]
    fn extract_mass_from_attrs() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let m = tree.mass.as_ref().unwrap();
        assert!((m.mass_kg - 1500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_cost_from_attrs() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let c = tree.cost.as_ref().unwrap();
        assert!((c.unit_cost - 25000.0).abs() < f64::EPSILON);
    }

    // -- flatten_bom ----------------------------------------------------------

    #[test]
    fn flatten_simple() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let rows = flatten_bom(&tree);
        assert_eq!(rows.len(), 3); // Vehicle, Engine, Wheel
        assert_eq!(rows[0].level, 0);
        assert_eq!(rows[0].quantity, 1);
    }

    #[test]
    fn flatten_cumulative_quantity() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let rows = flatten_bom(&tree);
        let wheel_row = rows.iter().find(|r| r.definition == "Wheel").unwrap();
        assert_eq!(wheel_row.quantity, 4);
    }

    #[test]
    fn flatten_deep_cumulative_quantity() {
        let mut model = Model::new("test.sysml".to_string());
        model.definitions.push(part_def("A"));
        model.definitions.push(part_def("B"));
        model.definitions.push(part_def("C"));
        model.usages.push(part_usage_with_mult("b", "B", "A", 3));
        model.usages.push(part_usage_with_mult("c", "C", "B", 2));
        let tree = build_bom_tree(&model, "A").unwrap();
        let rows = flatten_bom(&tree);
        let c_row = rows.iter().find(|r| r.definition == "C").unwrap();
        assert_eq!(c_row.quantity, 6); // 3 * 2
    }

    // -- mass_rollup ----------------------------------------------------------

    #[test]
    fn mass_rollup_simple() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let total = mass_rollup(&tree);
        // Vehicle: 1500 + Engine: 200 + 4*Wheel: 4*12.5 = 1500 + 200 + 50 = 1750
        assert!((total - 1750.0).abs() < 0.01);
    }

    #[test]
    fn mass_rollup_with_margin() {
        let node = BomNode {
            name: "part".to_string(),
            definition: "Part".to_string(),
            quantity: 1,
            identity: None,
            mass: Some(MassProperty {
                mass_kg: 100.0,
                mass_type: "estimated".to_string(),
                margin_pct: 10.0,
            }),
            cost: None,
            children: Vec::new(),
        };
        let total = mass_rollup(&node);
        assert!((total - 110.0).abs() < 0.01);
    }

    #[test]
    fn mass_rollup_empty() {
        let node = BomNode {
            name: "empty".to_string(),
            definition: "Empty".to_string(),
            quantity: 1,
            identity: None,
            mass: None,
            cost: None,
            children: Vec::new(),
        };
        assert!((mass_rollup(&node)).abs() < f64::EPSILON);
    }

    // -- cost_rollup ----------------------------------------------------------

    #[test]
    fn cost_rollup_simple() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let (recurring, tooling) = cost_rollup(&tree);
        // recurring: 25000 + 5000 + 4*150 = 30600
        assert!((recurring - 30600.0).abs() < 0.01);
        // tooling: 0 (Vehicle) + 100000 (Engine) + 0 (Wheel)
        assert!((tooling - 100000.0).abs() < 0.01);
    }

    #[test]
    fn cost_rollup_empty() {
        let node = BomNode {
            name: "x".to_string(),
            definition: "X".to_string(),
            quantity: 2,
            identity: None,
            mass: None,
            cost: None,
            children: Vec::new(),
        };
        let (r, t) = cost_rollup(&node);
        assert!(r.abs() < f64::EPSILON);
        assert!(t.abs() < f64::EPSILON);
    }

    // -- where_used -----------------------------------------------------------

    #[test]
    fn where_used_finds_parents() {
        let model = vehicle_model();
        let parents = where_used(&model, "Engine");
        assert_eq!(parents, vec!["Vehicle"]);
    }

    #[test]
    fn where_used_multiple() {
        let mut model = vehicle_model();
        model.definitions.push(part_def("Truck"));
        model.usages.push(part_usage("engine", "Engine", "Truck"));
        let parents = where_used(&model, "Engine");
        assert!(parents.contains(&"Truck".to_string()));
        assert!(parents.contains(&"Vehicle".to_string()));
    }

    #[test]
    fn where_used_not_found() {
        let model = vehicle_model();
        let parents = where_used(&model, "Nonexistent");
        assert!(parents.is_empty());
    }

    // -- bom_summary ----------------------------------------------------------

    #[test]
    fn summary_counts() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let summary = bom_summary(&tree);
        assert_eq!(summary.unique_parts, 3); // Vehicle, Engine, Wheel
        assert_eq!(summary.total_parts, 6);  // 1 Vehicle + 1 Engine + 4 Wheels
        assert_eq!(summary.max_depth, 2);
    }

    #[test]
    fn summary_with_mass_and_cost() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let summary = bom_summary(&tree);
        assert!(summary.total_mass_kg.is_some());
        assert!(summary.total_cost.is_some());
    }

    #[test]
    fn summary_no_mass_no_cost() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let summary = bom_summary(&tree);
        assert!(summary.total_mass_kg.is_none());
        assert!(summary.total_cost.is_none());
    }

    // -- format_bom_tree ------------------------------------------------------

    #[test]
    fn format_tree_basic() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let output = format_bom_tree(&tree, false, false);
        assert!(output.contains("Vehicle : Vehicle"));
        assert!(output.contains("  engine : Engine"));
        assert!(output.contains("  wheels : Wheel x4"));
    }

    #[test]
    fn format_tree_with_mass() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let output = format_bom_tree(&tree, true, false);
        assert!(output.contains("mass=1500.000kg"));
    }

    #[test]
    fn format_tree_with_cost() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let output = format_bom_tree(&tree, false, true);
        assert!(output.contains("cost=25000.00USD"));
    }

    #[test]
    fn format_tree_with_part_number() {
        let model = model_with_attrs();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let output = format_bom_tree(&tree, false, false);
        assert!(output.contains("[VH-001]"));
    }

    // -- format_bom_csv -------------------------------------------------------

    #[test]
    fn csv_header() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let csv = format_bom_csv(&tree);
        assert!(csv.starts_with("Level,Name,Definition,Quantity,PartNumber,Revision,Description,Category\n"));
    }

    #[test]
    fn csv_row_count() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let csv = format_bom_csv(&tree);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3 rows
    }

    #[test]
    fn csv_escapes_commas() {
        let escaped = csv_escape("hello, world");
        assert_eq!(escaped, "\"hello, world\"");
    }

    #[test]
    fn csv_escapes_quotes() {
        let escaped = csv_escape("say \"hi\"");
        assert_eq!(escaped, "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_no_escape_plain() {
        let escaped = csv_escape("hello");
        assert_eq!(escaped, "hello");
    }

    // -- enum Display ---------------------------------------------------------

    #[test]
    fn part_category_display() {
        assert_eq!(PartCategory::RawMaterial.to_string(), "Raw Material");
        assert_eq!(PartCategory::Assembly.to_string(), "Assembly");
    }

    #[test]
    fn lifecycle_state_display() {
        assert_eq!(LifecycleState::Production.to_string(), "Production");
    }

    #[test]
    fn make_or_buy_display() {
        assert_eq!(MakeOrBuy::MakeAndBuy.to_string(), "Make & Buy");
        assert_eq!(MakeOrBuy::Tbd.to_string(), "TBD");
    }

    // -- quantity_from_multiplicity -------------------------------------------

    #[test]
    fn quantity_from_lower_bound() {
        let m = Multiplicity {
            lower: Some("3".to_string()),
            upper: Some("5".to_string()),
            is_ordered: false,
            is_nonunique: false,
        };
        assert_eq!(quantity_from_multiplicity(&m), 3);
    }

    #[test]
    fn quantity_from_upper_when_no_lower() {
        let m = Multiplicity {
            lower: None,
            upper: Some("7".to_string()),
            is_ordered: false,
            is_nonunique: false,
        };
        assert_eq!(quantity_from_multiplicity(&m), 7);
    }

    #[test]
    fn quantity_default_star() {
        let m = Multiplicity {
            lower: None,
            upper: None,
            is_ordered: false,
            is_nonunique: false,
        };
        assert_eq!(quantity_from_multiplicity(&m), 1);
    }

    #[test]
    fn quantity_unparseable_defaults_to_one() {
        let m = Multiplicity {
            lower: Some("*".to_string()),
            upper: None,
            is_ordered: false,
            is_nonunique: false,
        };
        assert_eq!(quantity_from_multiplicity(&m), 1);
    }

    // -- parse helpers --------------------------------------------------------

    #[test]
    fn parse_category_variants() {
        assert_eq!(parse_category(Some("Assembly")), PartCategory::Assembly);
        assert_eq!(parse_category(Some("raw_material")), PartCategory::RawMaterial);
        assert_eq!(parse_category(Some("FASTENER")), PartCategory::Fastener);
        assert_eq!(parse_category(None), PartCategory::Component);
    }

    #[test]
    fn parse_lifecycle_variants() {
        assert_eq!(parse_lifecycle(Some("production")), LifecycleState::Production);
        assert_eq!(parse_lifecycle(Some("OBSOLETE")), LifecycleState::Obsolete);
        assert_eq!(parse_lifecycle(None), LifecycleState::Development);
    }

    #[test]
    fn parse_make_or_buy_variants() {
        assert_eq!(parse_make_or_buy(Some("make")), MakeOrBuy::Make);
        assert_eq!(parse_make_or_buy(Some("BUY")), MakeOrBuy::Buy);
        assert_eq!(parse_make_or_buy(Some("make_and_buy")), MakeOrBuy::MakeAndBuy);
        assert_eq!(parse_make_or_buy(None), MakeOrBuy::Tbd);
    }

    // -- costed BOM tests -----------------------------------------------------

    #[test]
    fn costed_bom_with_quotes() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let quotes = vec![
            sysml_source::Quote {
                part_name: "Engine".into(),
                supplier_name: "EngCo".into(),
                currency: "USD".into(),
                lead_time_days: 30,
                moq: 1,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
                price_breaks: vec![
                    sysml_source::PriceBreak { min_qty: 1, unit_price: 5000.0 },
                    sysml_source::PriceBreak { min_qty: 100, unit_price: 4500.0 },
                ],
            },
            sysml_source::Quote {
                part_name: "Wheel".into(),
                supplier_name: "WheelCo".into(),
                currency: "USD".into(),
                lead_time_days: 14,
                moq: 4,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
                price_breaks: vec![
                    sysml_source::PriceBreak { min_qty: 1, unit_price: 150.0 },
                    sysml_source::PriceBreak { min_qty: 100, unit_price: 120.0 },
                ],
            },
        ];

        // Order qty 10: Engine needs 10, Wheel needs 40
        let (rows, total) = costed_bom(&tree, &quotes, 10);
        assert_eq!(rows.len(), 3);

        // Vehicle has no quote
        assert!(rows[0].unit_price.is_none());

        // Engine: 1 × 10 = 10 total, price at 10 = 5000
        let engine = rows.iter().find(|r| r.definition == "Engine").unwrap();
        assert_eq!(engine.total_qty, 10);
        assert_eq!(engine.unit_price, Some(5000.0));
        assert!((engine.extended_price.unwrap() - 50000.0).abs() < 0.01);

        // Wheel: 4 × 10 = 40 total, price at 40 = 150
        let wheel = rows.iter().find(|r| r.definition == "Wheel").unwrap();
        assert_eq!(wheel.total_qty, 40);
        assert_eq!(wheel.unit_price, Some(150.0));
        assert!((wheel.extended_price.unwrap() - 6000.0).abs() < 0.01);

        assert!((total - 56000.0).abs() < 0.01);
    }

    #[test]
    fn costed_bom_price_breaks_at_volume() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let quotes = vec![
            sysml_source::Quote {
                part_name: "Wheel".into(),
                supplier_name: "WheelCo".into(),
                currency: "USD".into(),
                lead_time_days: 14,
                moq: 1,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
                price_breaks: vec![
                    sysml_source::PriceBreak { min_qty: 1, unit_price: 150.0 },
                    sysml_source::PriceBreak { min_qty: 100, unit_price: 120.0 },
                ],
            },
        ];

        // Order qty 25: 4 × 25 = 100 wheels → hits the 100-break
        let (rows, _) = costed_bom(&tree, &quotes, 25);
        let wheel = rows.iter().find(|r| r.definition == "Wheel").unwrap();
        assert_eq!(wheel.total_qty, 100);
        assert_eq!(wheel.unit_price, Some(120.0));
    }

    #[test]
    fn costed_bom_no_quotes() {
        let model = vehicle_model();
        let tree = build_bom_tree(&model, "Vehicle").unwrap();
        let (rows, total) = costed_bom(&tree, &[], 10);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.unit_price.is_none()));
        assert!(total.abs() < f64::EPSILON);
    }

    #[test]
    fn cost_attributes_from_costed() {
        let rows = vec![
            CostedBomRow {
                level: 0, name: "v".into(), definition: "Vehicle".into(),
                bom_qty: 1, total_qty: 10, unit_price: None,
                extended_price: None, supplier: None, currency: None,
            },
            CostedBomRow {
                level: 1, name: "e".into(), definition: "Engine".into(),
                bom_qty: 1, total_qty: 10, unit_price: Some(5000.0),
                extended_price: Some(50000.0), supplier: Some("EngCo".into()), currency: Some("USD".into()),
            },
        ];
        let attrs = cost_attributes_from_costed_bom(&rows);
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].0, "Engine");
        assert!(attrs[0].1.contains("unitCost = 5000.0000"));
    }

    #[test]
    fn format_costed_bom_output() {
        let rows = vec![
            CostedBomRow {
                level: 0, name: "v".into(), definition: "Vehicle".into(),
                bom_qty: 1, total_qty: 10, unit_price: None,
                extended_price: None, supplier: None, currency: None,
            },
            CostedBomRow {
                level: 1, name: "engine".into(), definition: "Engine".into(),
                bom_qty: 1, total_qty: 10, unit_price: Some(5000.0),
                extended_price: Some(50000.0), supplier: Some("EngCo".into()), currency: Some("USD".into()),
            },
        ];
        let text = format_costed_bom(&rows, 50000.0);
        assert!(text.contains("TOTAL:"));
        assert!(text.contains("50000.00"));
        assert!(text.contains("EngCo"));
    }
}
