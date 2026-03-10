//! Supplier management domain for SysML v2 models.
//!
//! Provides types and functions for tracking suppliers, sourcing decisions,
//! quotes, and supplier scorecards extracted from or linked to SysML models.

use std::collections::BTreeMap;

use serde::Serialize;
use sysml_core::model::{DefKind, Model};
use sysml_core::record::{self, RecordEnvelope, RecordMeta, RecordValue};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Qualification status of a supplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStatus {
    Pending,
    Conditional,
    Approved,
    Preferred,
    Probation,
    Disqualified,
}

impl QualificationStatus {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Conditional => "conditional",
            Self::Approved => "approved",
            Self::Preferred => "preferred",
            Self::Probation => "probation",
            Self::Disqualified => "disqualified",
        }
    }
}

/// Sourcing strategy for a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Only one supplier exists for this part.
    Sole,
    /// Only one supplier is used by choice.
    Single,
    /// Two suppliers are qualified.
    Dual,
    /// Three or more suppliers are qualified.
    Multi,
}

impl SourceType {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sole => "sole",
            Self::Single => "single",
            Self::Dual => "dual",
            Self::Multi => "multi",
        }
    }
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A supplier in the supply chain.
#[derive(Debug, Clone, Serialize)]
pub struct Supplier {
    pub name: String,
    pub code: String,
    pub qualification_status: QualificationStatus,
    pub certifications: Vec<String>,
    pub notes: String,
}

/// A sourcing relationship between a part and a supplier.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub part_name: String,
    pub supplier_name: String,
    pub supplier_part_number: String,
    pub lead_time_days: u32,
    pub moq: u32,
    pub unit_price: f64,
    pub source_type: SourceType,
    pub is_preferred: bool,
}

/// A supplier quote for a part.
#[derive(Debug, Clone, Serialize)]
pub struct Quote {
    pub part_name: String,
    pub supplier_name: String,
    pub unit_price: f64,
    pub lead_time_days: u32,
    pub moq: u32,
    pub valid_until: String,
    pub notes: String,
}

/// Side-by-side comparison entry for a quote.
#[derive(Debug, Clone, Serialize)]
pub struct QuoteComparison {
    pub supplier_name: String,
    pub unit_price: f64,
    pub lead_time_days: u32,
    pub moq: u32,
    pub price_rank: usize,
    pub lead_time_rank: usize,
}

/// Aggregated scorecard for a supplier.
#[derive(Debug, Clone, Serialize)]
pub struct SupplierScore {
    pub supplier_name: String,
    pub delivery_score: f64,
    pub quality_score: f64,
    pub price_score: f64,
    pub overall_score: f64,
}

// ---------------------------------------------------------------------------
// Model extraction
// ---------------------------------------------------------------------------

/// Extract supplier records from part definitions that specialize `SupplierDef`.
///
/// Looks for `part def` nodes whose `super_type` is `"SupplierDef"` and
/// interprets the definition name as the supplier name, the short name as the
/// supplier code, and doc text as notes.  Certifications are extracted from
/// attribute usages named `certifications` within the definition.
pub fn extract_suppliers(model: &Model) -> Vec<Supplier> {
    let mut suppliers = Vec::new();

    for def in &model.definitions {
        if def.kind != DefKind::Part {
            continue;
        }
        let super_type = match &def.super_type {
            Some(s) => s,
            None => continue,
        };
        if !super_type_matches(super_type, "SupplierDef") {
            continue;
        }

        let code = def.short_name.clone().unwrap_or_default();
        let notes = def.doc.clone().unwrap_or_default();

        // Gather certifications from attribute usages inside this definition.
        let certifications: Vec<String> = model
            .usages
            .iter()
            .filter(|u| {
                u.parent_def.as_deref() == Some(&def.name) && u.name == "certifications"
            })
            .filter_map(|u| u.value_expr.clone())
            .collect();

        // Determine qualification status from attribute usages named "status".
        let qualification_status = model
            .usages
            .iter()
            .find(|u| {
                u.parent_def.as_deref() == Some(&def.name) && u.name == "status"
            })
            .and_then(|u| u.value_expr.as_deref())
            .map(parse_qualification_status)
            .unwrap_or(QualificationStatus::Pending);

        suppliers.push(Supplier {
            name: def.name.clone(),
            code,
            qualification_status,
            certifications,
            notes,
        });
    }

    suppliers
}

/// Extract source records from connection usages that specialize `SourceDef`.
///
/// The connection's source endpoint is treated as the part name and the target
/// endpoint as the supplier name.  Additional attributes (lead time, MOQ,
/// unit price, source type, preferred flag) are read from sibling usages.
pub fn extract_sources(model: &Model) -> Vec<Source> {
    let mut sources = Vec::new();

    for def in &model.definitions {
        if def.kind != DefKind::Connection {
            continue;
        }
        let super_type = match &def.super_type {
            Some(s) => s,
            None => continue,
        };
        if !super_type_matches(super_type, "SourceDef") {
            continue;
        }

        // Find the connection usage that belongs to this definition.
        let conn = model
            .connections
            .iter()
            .find(|c| c.name.as_deref() == Some(&*def.name));

        let (part_name, supplier_name) = match conn {
            Some(c) => (c.source.clone(), c.target.clone()),
            None => continue,
        };

        // Helper closure: find a usage value inside this definition by name.
        let find_usage_value = |name: &str| -> Option<String> {
            model
                .usages
                .iter()
                .find(|u| u.parent_def.as_deref() == Some(&def.name) && u.name == name)
                .and_then(|u| u.value_expr.clone())
        };

        let supplier_part_number = find_usage_value("supplier_part_number").unwrap_or_default();
        let lead_time_days = find_usage_value("lead_time_days")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let moq = find_usage_value("moq")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        let unit_price = find_usage_value("unit_price")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let source_type = find_usage_value("source_type")
            .map(|v| parse_source_type(&v))
            .unwrap_or(SourceType::Single);
        let is_preferred = find_usage_value("is_preferred")
            .map(|v| v == "true")
            .unwrap_or(false);

        sources.push(Source {
            part_name,
            supplier_name,
            supplier_part_number,
            lead_time_days,
            moq,
            unit_price,
            source_type,
            is_preferred,
        });
    }

    sources
}

// ---------------------------------------------------------------------------
// Quote operations
// ---------------------------------------------------------------------------

/// Create a TOML record envelope for a supplier quote.
pub fn create_quote_record(quote: &Quote, author: &str) -> RecordEnvelope {
    let id = record::generate_record_id("source", "quote", author);
    let created = record::now_iso8601();

    let mut refs = BTreeMap::new();
    refs.insert("part".into(), vec![quote.part_name.clone()]);
    refs.insert("supplier".into(), vec![quote.supplier_name.clone()]);

    let mut data = BTreeMap::new();
    data.insert(
        "unit_price".into(),
        RecordValue::Float(quote.unit_price),
    );
    data.insert(
        "lead_time_days".into(),
        RecordValue::Integer(quote.lead_time_days as i64),
    );
    data.insert("moq".into(), RecordValue::Integer(quote.moq as i64));
    data.insert(
        "valid_until".into(),
        RecordValue::String(quote.valid_until.clone()),
    );
    if !quote.notes.is_empty() {
        data.insert("notes".into(), RecordValue::String(quote.notes.clone()));
    }

    RecordEnvelope {
        meta: RecordMeta {
            id,
            tool: "source".into(),
            record_type: "quote".into(),
            created,
            author: author.into(),
        },
        refs,
        data,
    }
}

/// Compare quotes side-by-side, sorted by ascending unit price.
///
/// Each entry receives independent rank values for price and lead time,
/// where rank 1 is the best (lowest) value.
pub fn compare_quotes(quotes: &[Quote]) -> Vec<QuoteComparison> {
    if quotes.is_empty() {
        return Vec::new();
    }

    let mut comparisons: Vec<QuoteComparison> = quotes
        .iter()
        .map(|q| QuoteComparison {
            supplier_name: q.supplier_name.clone(),
            unit_price: q.unit_price,
            lead_time_days: q.lead_time_days,
            moq: q.moq,
            price_rank: 0,
            lead_time_rank: 0,
        })
        .collect();

    // Sort by unit price ascending.
    comparisons.sort_by(|a, b| a.unit_price.partial_cmp(&b.unit_price).unwrap_or(std::cmp::Ordering::Equal));

    // Assign price ranks (1-based).
    for (i, comp) in comparisons.iter_mut().enumerate() {
        comp.price_rank = i + 1;
    }

    // Compute lead-time ranks by sorting indices.
    let mut lt_indices: Vec<usize> = (0..comparisons.len()).collect();
    lt_indices.sort_by_key(|&i| comparisons[i].lead_time_days);
    for (rank, &idx) in lt_indices.iter().enumerate() {
        comparisons[idx].lead_time_rank = rank + 1;
    }

    comparisons
}

// ---------------------------------------------------------------------------
// Scorecard
// ---------------------------------------------------------------------------

/// Compute an aggregated supplier scorecard.
///
/// - `delivery_records`: each entry is `(on_time: bool, date: String)`.
/// - `quality_records`: each entry is `(passed: bool, date: String)`.
/// - `quote_prices`: historical quoted prices for comparison.
/// - `market_avg_price`: average market price for price-competitiveness scoring.
///
/// Scores are in the range `[0.0, 1.0]`.  The overall score is a weighted
/// average: delivery 40%, quality 40%, price 20%.
pub fn compute_scorecard(
    supplier: &str,
    delivery_records: &[(bool, String)],
    quality_records: &[(bool, String)],
    quote_prices: &[f64],
    market_avg_price: f64,
) -> SupplierScore {
    let delivery_score = if delivery_records.is_empty() {
        0.0
    } else {
        let on_time = delivery_records.iter().filter(|(ok, _)| *ok).count();
        on_time as f64 / delivery_records.len() as f64
    };

    let quality_score = if quality_records.is_empty() {
        0.0
    } else {
        let passed = quality_records.iter().filter(|(ok, _)| *ok).count();
        passed as f64 / quality_records.len() as f64
    };

    let price_score = if quote_prices.is_empty() || market_avg_price <= 0.0 {
        0.0
    } else {
        let avg_quote = quote_prices.iter().sum::<f64>() / quote_prices.len() as f64;
        // Score = how much cheaper than market. Capped at [0, 1].
        // If avg_quote == market_avg, score = 0.5. Lower quote = higher score.
        let ratio = avg_quote / market_avg_price;
        (2.0 - ratio).clamp(0.0, 1.0)
    };

    // Weighted: delivery 40%, quality 40%, price 20%.
    let overall_score = 0.4 * delivery_score + 0.4 * quality_score + 0.2 * price_score;

    SupplierScore {
        supplier_name: supplier.to_string(),
        delivery_score,
        quality_score,
        price_score,
        overall_score,
    }
}

// ---------------------------------------------------------------------------
// Filtering & generation
// ---------------------------------------------------------------------------

/// Filter sources to only those whose supplier is approved or preferred.
pub fn approved_source_list(sources: &[Source], suppliers: &[Supplier]) -> Vec<Source> {
    let approved_names: std::collections::HashSet<&str> = suppliers
        .iter()
        .filter(|s| matches!(
            s.qualification_status,
            QualificationStatus::Approved | QualificationStatus::Preferred
        ))
        .map(|s| s.name.as_str())
        .collect();

    sources
        .iter()
        .filter(|src| approved_names.contains(src.supplier_name.as_str()))
        .cloned()
        .collect()
}

/// Generate a simple request-for-quotation (RFQ) text document.
pub fn generate_rfq_text(
    part_name: &str,
    description: &str,
    quantity: u32,
    notes: &str,
) -> String {
    let mut text = String::new();
    text.push_str("REQUEST FOR QUOTATION\n");
    text.push_str("=====================\n\n");
    text.push_str(&format!("Part:        {part_name}\n"));
    text.push_str(&format!("Description: {description}\n"));
    text.push_str(&format!("Quantity:    {quantity}\n"));
    if !notes.is_empty() {
        text.push_str(&format!("\nNotes:\n{notes}\n"));
    }
    text.push_str("\nPlease provide:\n");
    text.push_str("  - Unit price\n");
    text.push_str("  - Lead time (calendar days)\n");
    text.push_str("  - Minimum order quantity\n");
    text.push_str("  - Quote validity period\n");
    text
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check whether a super-type reference matches a target name, handling
/// both qualified (`Pkg::SupplierDef`) and simple (`SupplierDef`) forms.
fn super_type_matches(super_type: &str, target: &str) -> bool {
    super_type == target || super_type.ends_with(&format!("::{target}"))
}

/// Parse a qualification status string (case-insensitive).
fn parse_qualification_status(s: &str) -> QualificationStatus {
    match s.trim().to_lowercase().as_str() {
        "approved" => QualificationStatus::Approved,
        "preferred" => QualificationStatus::Preferred,
        "conditional" => QualificationStatus::Conditional,
        "probation" => QualificationStatus::Probation,
        "disqualified" => QualificationStatus::Disqualified,
        _ => QualificationStatus::Pending,
    }
}

/// Parse a source type string (case-insensitive).
fn parse_source_type(s: &str) -> SourceType {
    match s.trim().to_lowercase().as_str() {
        "sole" => SourceType::Sole,
        "dual" => SourceType::Dual,
        "multi" => SourceType::Multi,
        _ => SourceType::Single,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::model::{Connection, Definition, DefKind, Model, Span, Usage};

    fn default_span() -> Span {
        Span {
            start_row: 1,
            start_col: 1,
            end_row: 1,
            end_col: 1,
            start_byte: 0,
            end_byte: 0,
        }
    }

    fn make_def(name: &str, kind: DefKind, super_type: Option<&str>) -> Definition {
        Definition {
            kind,
            name: name.to_string(),
            super_type: super_type.map(|s| s.to_string()),
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

    fn make_usage(name: &str, parent_def: Option<&str>, value_expr: Option<&str>) -> Usage {
        Usage {
            kind: "attribute".to_string(),
            name: name.to_string(),
            type_ref: None,
            span: default_span(),
            direction: None,
            is_conjugated: false,
            parent_def: parent_def.map(|s| s.to_string()),
            multiplicity: None,
            value_expr: value_expr.map(|s| s.to_string()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        }
    }

    fn make_model() -> Model {
        Model::new("test.sysml".to_string())
    }

    // -- QualificationStatus --

    #[test]
    fn qualification_status_labels() {
        assert_eq!(QualificationStatus::Pending.label(), "pending");
        assert_eq!(QualificationStatus::Conditional.label(), "conditional");
        assert_eq!(QualificationStatus::Approved.label(), "approved");
        assert_eq!(QualificationStatus::Preferred.label(), "preferred");
        assert_eq!(QualificationStatus::Probation.label(), "probation");
        assert_eq!(QualificationStatus::Disqualified.label(), "disqualified");
    }

    #[test]
    fn parse_qualification_status_variants() {
        assert_eq!(parse_qualification_status("approved"), QualificationStatus::Approved);
        assert_eq!(parse_qualification_status("PREFERRED"), QualificationStatus::Preferred);
        assert_eq!(parse_qualification_status("Conditional"), QualificationStatus::Conditional);
        assert_eq!(parse_qualification_status("probation"), QualificationStatus::Probation);
        assert_eq!(parse_qualification_status("disqualified"), QualificationStatus::Disqualified);
        assert_eq!(parse_qualification_status("unknown"), QualificationStatus::Pending);
        assert_eq!(parse_qualification_status(""), QualificationStatus::Pending);
    }

    // -- SourceType --

    #[test]
    fn source_type_labels() {
        assert_eq!(SourceType::Sole.label(), "sole");
        assert_eq!(SourceType::Single.label(), "single");
        assert_eq!(SourceType::Dual.label(), "dual");
        assert_eq!(SourceType::Multi.label(), "multi");
    }

    #[test]
    fn parse_source_type_variants() {
        assert_eq!(parse_source_type("sole"), SourceType::Sole);
        assert_eq!(parse_source_type("DUAL"), SourceType::Dual);
        assert_eq!(parse_source_type("Multi"), SourceType::Multi);
        assert_eq!(parse_source_type("anything"), SourceType::Single);
    }

    // -- super_type_matches --

    #[test]
    fn super_type_matches_simple_and_qualified() {
        assert!(super_type_matches("SupplierDef", "SupplierDef"));
        assert!(super_type_matches("Supply::SupplierDef", "SupplierDef"));
        assert!(!super_type_matches("OtherDef", "SupplierDef"));
        assert!(!super_type_matches("NotSupplierDef", "SupplierDef"));
    }

    // -- extract_suppliers --

    #[test]
    fn extract_suppliers_basic() {
        let mut model = make_model();
        let mut def = make_def("Acme", DefKind::Part, Some("SupplierDef"));
        def.short_name = Some("ACM".to_string());
        def.doc = Some("Acme Corp supplier".to_string());
        model.definitions.push(def);
        model.usages.push(make_usage("status", Some("Acme"), Some("approved")));
        model.usages.push(make_usage("certifications", Some("Acme"), Some("ISO9001")));
        model.usages.push(make_usage("certifications", Some("Acme"), Some("AS9100")));

        let suppliers = extract_suppliers(&model);
        assert_eq!(suppliers.len(), 1);
        assert_eq!(suppliers[0].name, "Acme");
        assert_eq!(suppliers[0].code, "ACM");
        assert_eq!(suppliers[0].qualification_status, QualificationStatus::Approved);
        assert_eq!(suppliers[0].certifications, vec!["ISO9001", "AS9100"]);
        assert_eq!(suppliers[0].notes, "Acme Corp supplier");
    }

    #[test]
    fn extract_suppliers_qualified_super_type() {
        let mut model = make_model();
        model.definitions.push(make_def("BobCo", DefKind::Part, Some("Supply::SupplierDef")));

        let suppliers = extract_suppliers(&model);
        assert_eq!(suppliers.len(), 1);
        assert_eq!(suppliers[0].name, "BobCo");
        assert_eq!(suppliers[0].qualification_status, QualificationStatus::Pending);
    }

    #[test]
    fn extract_suppliers_ignores_non_part_defs() {
        let mut model = make_model();
        model.definitions.push(make_def("NotAPart", DefKind::Action, Some("SupplierDef")));

        let suppliers = extract_suppliers(&model);
        assert!(suppliers.is_empty());
    }

    #[test]
    fn extract_suppliers_ignores_non_supplier_specialization() {
        let mut model = make_model();
        model.definitions.push(make_def("Gizmo", DefKind::Part, Some("PartDef")));

        let suppliers = extract_suppliers(&model);
        assert!(suppliers.is_empty());
    }

    #[test]
    fn extract_suppliers_no_super_type() {
        let mut model = make_model();
        model.definitions.push(make_def("NakedPart", DefKind::Part, None));

        let suppliers = extract_suppliers(&model);
        assert!(suppliers.is_empty());
    }

    // -- extract_sources --

    #[test]
    fn extract_sources_basic() {
        let mut model = make_model();
        model.definitions.push(make_def("AcmeSource", DefKind::Connection, Some("SourceDef")));
        model.connections.push(Connection {
            name: Some("AcmeSource".to_string()),
            source: "Resistor".to_string(),
            target: "Acme".to_string(),
            span: default_span(),
        });
        model.usages.push(make_usage("supplier_part_number", Some("AcmeSource"), Some("R-100K")));
        model.usages.push(make_usage("lead_time_days", Some("AcmeSource"), Some("14")));
        model.usages.push(make_usage("moq", Some("AcmeSource"), Some("100")));
        model.usages.push(make_usage("unit_price", Some("AcmeSource"), Some("0.05")));
        model.usages.push(make_usage("source_type", Some("AcmeSource"), Some("dual")));
        model.usages.push(make_usage("is_preferred", Some("AcmeSource"), Some("true")));

        let sources = extract_sources(&model);
        assert_eq!(sources.len(), 1);
        let s = &sources[0];
        assert_eq!(s.part_name, "Resistor");
        assert_eq!(s.supplier_name, "Acme");
        assert_eq!(s.supplier_part_number, "R-100K");
        assert_eq!(s.lead_time_days, 14);
        assert_eq!(s.moq, 100);
        assert_eq!((s.unit_price * 100.0).round(), 5.0);
        assert_eq!(s.source_type, SourceType::Dual);
        assert!(s.is_preferred);
    }

    #[test]
    fn extract_sources_defaults_when_no_attributes() {
        let mut model = make_model();
        model.definitions.push(make_def("MinSource", DefKind::Connection, Some("SourceDef")));
        model.connections.push(Connection {
            name: Some("MinSource".to_string()),
            source: "Widget".to_string(),
            target: "Vendor".to_string(),
            span: default_span(),
        });

        let sources = extract_sources(&model);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].lead_time_days, 0);
        assert_eq!(sources[0].moq, 1);
        assert_eq!(sources[0].unit_price, 0.0);
        assert_eq!(sources[0].source_type, SourceType::Single);
        assert!(!sources[0].is_preferred);
    }

    #[test]
    fn extract_sources_ignores_non_connection_defs() {
        let mut model = make_model();
        model.definitions.push(make_def("FakeSrc", DefKind::Part, Some("SourceDef")));

        let sources = extract_sources(&model);
        assert!(sources.is_empty());
    }

    // -- create_quote_record --

    #[test]
    fn create_quote_record_structure() {
        let quote = Quote {
            part_name: "Resistor".to_string(),
            supplier_name: "Acme".to_string(),
            unit_price: 0.05,
            lead_time_days: 14,
            moq: 100,
            valid_until: "2026-06-01".to_string(),
            notes: "Bulk discount available".to_string(),
        };

        let env = create_quote_record(&quote, "alice");
        assert!(env.meta.id.starts_with("source-quote-"));
        assert_eq!(env.meta.tool, "source");
        assert_eq!(env.meta.record_type, "quote");
        assert_eq!(env.meta.author, "alice");
        assert_eq!(env.refs.get("part"), Some(&vec!["Resistor".to_string()]));
        assert_eq!(env.refs.get("supplier"), Some(&vec!["Acme".to_string()]));
        assert_eq!(env.data.get("unit_price"), Some(&RecordValue::Float(0.05)));
        assert_eq!(env.data.get("lead_time_days"), Some(&RecordValue::Integer(14)));
        assert_eq!(env.data.get("moq"), Some(&RecordValue::Integer(100)));
        assert_eq!(
            env.data.get("valid_until"),
            Some(&RecordValue::String("2026-06-01".to_string()))
        );
        assert_eq!(
            env.data.get("notes"),
            Some(&RecordValue::String("Bulk discount available".to_string()))
        );
    }

    #[test]
    fn create_quote_record_omits_empty_notes() {
        let quote = Quote {
            part_name: "Cap".to_string(),
            supplier_name: "BobCo".to_string(),
            unit_price: 1.50,
            lead_time_days: 7,
            moq: 50,
            valid_until: "2026-12-31".to_string(),
            notes: String::new(),
        };

        let env = create_quote_record(&quote, "bob");
        assert!(!env.data.contains_key("notes"));
    }

    #[test]
    fn create_quote_record_toml_round_trip() {
        let quote = Quote {
            part_name: "Bolt".to_string(),
            supplier_name: "FastenerCo".to_string(),
            unit_price: 0.10,
            lead_time_days: 5,
            moq: 1000,
            valid_until: "2026-09-30".to_string(),
            notes: "Standard hex bolt".to_string(),
        };

        let env = create_quote_record(&quote, "carol");
        let toml_str = env.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.meta.tool, "source");
        assert_eq!(parsed.refs.get("part"), Some(&vec!["Bolt".to_string()]));
    }

    // -- compare_quotes --

    #[test]
    fn compare_quotes_sorted_by_price() {
        let quotes = vec![
            Quote {
                part_name: "R1".into(),
                supplier_name: "Expensive".into(),
                unit_price: 10.0,
                lead_time_days: 3,
                moq: 10,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
            },
            Quote {
                part_name: "R1".into(),
                supplier_name: "Cheap".into(),
                unit_price: 2.0,
                lead_time_days: 30,
                moq: 500,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
            },
            Quote {
                part_name: "R1".into(),
                supplier_name: "Mid".into(),
                unit_price: 5.0,
                lead_time_days: 10,
                moq: 100,
                valid_until: "2026-12-31".into(),
                notes: String::new(),
            },
        ];

        let result = compare_quotes(&quotes);
        assert_eq!(result.len(), 3);

        // Sorted by price ascending.
        assert_eq!(result[0].supplier_name, "Cheap");
        assert_eq!(result[1].supplier_name, "Mid");
        assert_eq!(result[2].supplier_name, "Expensive");

        // Price ranks.
        assert_eq!(result[0].price_rank, 1);
        assert_eq!(result[1].price_rank, 2);
        assert_eq!(result[2].price_rank, 3);

        // Lead time ranks: Expensive(3) < Mid(10) < Cheap(30).
        assert_eq!(result[2].lead_time_rank, 1); // Expensive has shortest lead time
        assert_eq!(result[1].lead_time_rank, 2); // Mid
        assert_eq!(result[0].lead_time_rank, 3); // Cheap has longest lead time
    }

    #[test]
    fn compare_quotes_empty() {
        let result = compare_quotes(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn compare_quotes_single() {
        let quotes = vec![Quote {
            part_name: "X".into(),
            supplier_name: "Only".into(),
            unit_price: 1.0,
            lead_time_days: 5,
            moq: 1,
            valid_until: "2026-12-31".into(),
            notes: String::new(),
        }];

        let result = compare_quotes(&quotes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].price_rank, 1);
        assert_eq!(result[0].lead_time_rank, 1);
    }

    // -- compute_scorecard --

    #[test]
    fn scorecard_perfect_delivery_and_quality() {
        let deliveries = vec![
            (true, "2026-01-01".into()),
            (true, "2026-02-01".into()),
            (true, "2026-03-01".into()),
        ];
        let quality = vec![
            (true, "2026-01-01".into()),
            (true, "2026-02-01".into()),
        ];
        let prices = vec![80.0, 90.0];
        let market = 100.0;

        let score = compute_scorecard("Acme", &deliveries, &quality, &prices, market);
        assert_eq!(score.supplier_name, "Acme");
        assert_eq!(score.delivery_score, 1.0);
        assert_eq!(score.quality_score, 1.0);
        // avg_quote = 85, ratio = 0.85, price_score = 2.0 - 0.85 = 1.15 clamped to 1.0
        assert_eq!(score.price_score, 1.0);
        // overall = 0.4*1 + 0.4*1 + 0.2*1 = 1.0
        assert!((score.overall_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scorecard_partial_delivery() {
        let deliveries = vec![
            (true, "2026-01-01".into()),
            (false, "2026-02-01".into()),
            (true, "2026-03-01".into()),
            (false, "2026-04-01".into()),
        ];
        let quality = vec![(true, "2026-01-01".into())];

        let score = compute_scorecard("BobCo", &deliveries, &quality, &[], 100.0);
        assert_eq!(score.delivery_score, 0.5);
        assert_eq!(score.quality_score, 1.0);
        assert_eq!(score.price_score, 0.0); // no quotes
    }

    #[test]
    fn scorecard_empty_records() {
        let score = compute_scorecard("Empty", &[], &[], &[], 0.0);
        assert_eq!(score.delivery_score, 0.0);
        assert_eq!(score.quality_score, 0.0);
        assert_eq!(score.price_score, 0.0);
        assert_eq!(score.overall_score, 0.0);
    }

    #[test]
    fn scorecard_expensive_supplier() {
        let prices = vec![200.0];
        let market = 100.0;

        let score = compute_scorecard("Pricey", &[], &[], &prices, market);
        // ratio = 2.0, price_score = 2.0 - 2.0 = 0.0
        assert_eq!(score.price_score, 0.0);
    }

    #[test]
    fn scorecard_overall_weighted() {
        // delivery: 50%, quality: 100%, price: market-equal (score 1.0)
        let deliveries = vec![(true, "d".into()), (false, "d".into())];
        let quality = vec![(true, "d".into())];
        let prices = vec![100.0];
        let market = 100.0;

        let score = compute_scorecard("Test", &deliveries, &quality, &prices, market);
        assert_eq!(score.delivery_score, 0.5);
        assert_eq!(score.quality_score, 1.0);
        // ratio = 1.0, price_score = 2.0 - 1.0 = 1.0
        assert_eq!(score.price_score, 1.0);
        // 0.4*0.5 + 0.4*1.0 + 0.2*1.0 = 0.2 + 0.4 + 0.2 = 0.8
        assert!((score.overall_score - 0.8).abs() < f64::EPSILON);
    }

    // -- approved_source_list --

    #[test]
    fn approved_source_list_filters_correctly() {
        let suppliers = vec![
            Supplier {
                name: "Acme".into(),
                code: "ACM".into(),
                qualification_status: QualificationStatus::Approved,
                certifications: vec![],
                notes: String::new(),
            },
            Supplier {
                name: "BobCo".into(),
                code: "BOB".into(),
                qualification_status: QualificationStatus::Pending,
                certifications: vec![],
                notes: String::new(),
            },
            Supplier {
                name: "Carol".into(),
                code: "CRL".into(),
                qualification_status: QualificationStatus::Preferred,
                certifications: vec![],
                notes: String::new(),
            },
            Supplier {
                name: "Dave".into(),
                code: "DVE".into(),
                qualification_status: QualificationStatus::Disqualified,
                certifications: vec![],
                notes: String::new(),
            },
        ];

        let sources = vec![
            Source {
                part_name: "R1".into(),
                supplier_name: "Acme".into(),
                supplier_part_number: "X".into(),
                lead_time_days: 7,
                moq: 10,
                unit_price: 1.0,
                source_type: SourceType::Dual,
                is_preferred: true,
            },
            Source {
                part_name: "R1".into(),
                supplier_name: "BobCo".into(),
                supplier_part_number: "Y".into(),
                lead_time_days: 14,
                moq: 20,
                unit_price: 0.9,
                source_type: SourceType::Dual,
                is_preferred: false,
            },
            Source {
                part_name: "R1".into(),
                supplier_name: "Carol".into(),
                supplier_part_number: "Z".into(),
                lead_time_days: 10,
                moq: 5,
                unit_price: 1.1,
                source_type: SourceType::Single,
                is_preferred: false,
            },
            Source {
                part_name: "R1".into(),
                supplier_name: "Dave".into(),
                supplier_part_number: "W".into(),
                lead_time_days: 3,
                moq: 1,
                unit_price: 2.0,
                source_type: SourceType::Sole,
                is_preferred: false,
            },
        ];

        let approved = approved_source_list(&sources, &suppliers);
        assert_eq!(approved.len(), 2);
        let names: Vec<&str> = approved.iter().map(|s| s.supplier_name.as_str()).collect();
        assert!(names.contains(&"Acme"));
        assert!(names.contains(&"Carol"));
        assert!(!names.contains(&"BobCo"));
        assert!(!names.contains(&"Dave"));
    }

    #[test]
    fn approved_source_list_empty_inputs() {
        let result = approved_source_list(&[], &[]);
        assert!(result.is_empty());
    }

    // -- generate_rfq_text --

    #[test]
    fn rfq_text_structure() {
        let text = generate_rfq_text("Resistor 100K", "Standard 1/4W resistor", 5000, "Tape and reel preferred");
        assert!(text.contains("REQUEST FOR QUOTATION"));
        assert!(text.contains("Part:        Resistor 100K"));
        assert!(text.contains("Description: Standard 1/4W resistor"));
        assert!(text.contains("Quantity:    5000"));
        assert!(text.contains("Tape and reel preferred"));
        assert!(text.contains("Unit price"));
        assert!(text.contains("Lead time"));
        assert!(text.contains("Minimum order quantity"));
        assert!(text.contains("Quote validity period"));
    }

    #[test]
    fn rfq_text_without_notes() {
        let text = generate_rfq_text("Bolt", "M5 hex bolt", 100, "");
        assert!(text.contains("Part:        Bolt"));
        assert!(!text.contains("Notes:"));
    }

    // -- Serialization --

    #[test]
    fn supplier_serializes_to_json() {
        let supplier = Supplier {
            name: "Acme".into(),
            code: "ACM".into(),
            qualification_status: QualificationStatus::Approved,
            certifications: vec!["ISO9001".into()],
            notes: "Good supplier".into(),
        };
        let json = serde_json::to_string(&supplier).unwrap();
        assert!(json.contains("\"qualification_status\":\"approved\""));
        assert!(json.contains("\"name\":\"Acme\""));
    }

    #[test]
    fn source_type_serializes_correctly() {
        let source = Source {
            part_name: "X".into(),
            supplier_name: "Y".into(),
            supplier_part_number: "Z".into(),
            lead_time_days: 1,
            moq: 1,
            unit_price: 1.0,
            source_type: SourceType::Multi,
            is_preferred: false,
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"source_type\":\"multi\""));
    }
}
