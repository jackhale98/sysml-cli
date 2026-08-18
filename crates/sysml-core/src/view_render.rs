//! Generic view rendering: the tool-side interpreter of `view def`s.
//!
//! There is no report engine and no per-report code. A `view def` carries
//! a `@TableRendering` annotation describing rows, columns, sort, and
//! pivot; this module evaluates that spec against the model and returns a
//! rendered table. Domain packages ship their standard views
//! (FmeaWorksheet, StackupSummary, ...) as models — adding a new report
//! anywhere requires zero tool changes.
//!
//! `@TableRendering` fields (all strings, `;`-separated lists so column
//! expressions may contain commas):
//!
//!   rows    — the row provider:
//!               "@Fmea"                annotations of a metadata type
//!               "type:Hazard"          usages typed by a definition
//!                                       (or any of its specializations)
//!               "kind:port"            usages of a syntactic kind
//!               "relation:allocation"  allocation pairs
//!               "relation:satisfy"     satisfy relationships
//!               "relation:verify"      verify relationships
//!               "relation:connection"  connections
//!               "trace"                requirement coverage rows
//!               "kindcounts"           definitions/usages grouped by kind
//!               "uncertainty"          uncertainty analysis results
//!   columns — "name; name = expression; ..." — bare names copy row
//!             fields; expressions evaluate against the row's numeric
//!             fields via the shared evaluator
//!   sortBy  — column name, "-" prefix for descending
//!   where   — boolean expression filtering rows
//!   pivot   — "rowField; colField" — grid of row counts

use std::collections::BTreeMap;

use crate::model::{simple_name, Model, Usage};

/// A rendered table, ready for text/json/csv/markdown output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderedTable {
    pub view: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Warnings produced while rendering (unknown columns, eval errors).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// All views (name, source file, has table spec) across the models.
pub fn available_views(models: &[Model]) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for m in models {
        for v in &m.views {
            let has_spec = find_table_annotation(models, &v.name).is_some();
            out.push((v.name.clone(), m.file.clone(), has_spec));
        }
    }
    out
}

/// Render the named view against the models.
/// Render a model-defined view.
///
/// `models` is the full resolution context (target files PLUS anything
/// on the include path); `targets` names the files whose content
/// should produce rows. Include paths exist so references resolve and
/// so library-defined views can be found — their contents are not the
/// user's model, and counting them made `ModelStats` report the
/// library's definitions and `trace` list requirements the user never
/// wrote. An empty `targets` means "everything is content".
pub fn render_view(
    models: &[Model],
    targets: &[String],
    view_name: &str,
) -> Result<RenderedTable, String> {
    let view_exists = models.iter().any(|m| m.views.iter().any(|v| v.name == view_name));
    if !view_exists {
        let names: Vec<String> = available_views(models)
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        return Err(format!(
            "view `{view_name}` not found. Available views: {}",
            if names.is_empty() {
                "(none)".to_string()
            } else {
                names.join(", ")
            }
        ));
    }
    let ann = find_table_annotation(models, view_name).ok_or_else(|| {
        format!(
            "view `{view_name}` has no @TableRendering annotation — nothing \
             specifies its rows and columns (see the domain libraries for examples)"
        )
    })?;

    let get = |key: &str| -> Option<String> {
        ann.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| unquote(v).to_string())
    };
    let rows_spec = get("rows").ok_or("missing `rows` in @TableRendering")?;
    let mut warnings = Vec::new();
    let rows = provide_rows(models, targets, &rows_spec, &mut warnings)?;

    // Filter with `where`.
    let rows = if let Some(cond) = get("where") {
        rows.into_iter()
            .filter(|r| eval_bool(&cond, r).unwrap_or(false))
            .collect()
    } else {
        rows
    };

    // Pivot short-circuits normal column handling.
    if let Some(p) = get("pivot") {
        let parts: Vec<&str> = p.split(';').map(str::trim).collect();
        if parts.len() != 2 {
            return Err("pivot expects `rowField; colField`".to_string());
        }
        return Ok(pivot_table(view_name, &rows, parts[0], parts[1]));
    }

    let column_spec = get("columns").ok_or("missing `columns` in @TableRendering")?;
    let columns: Vec<(String, Option<String>)> = column_spec
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|c| match c.split_once('=') {
            Some((name, expr)) => (name.trim().to_string(), Some(expr.trim().to_string())),
            None => (c.to_string(), None),
        })
        .collect();

    let mut table_rows: Vec<Vec<String>> = Vec::new();
    for row in &rows {
        let mut out_row = Vec::new();
        for (name, expr) in &columns {
            let cell = match expr {
                None => field(row, name).unwrap_or_default(),
                Some(e) => match eval_num(e, row) {
                    Some(v) => format_num(v),
                    None => {
                        // fall back to string field lookup of the expr
                        field(row, e).unwrap_or_default()
                    }
                },
            };
            out_row.push(cell);
        }
        table_rows.push(out_row);
    }

    // Sort.
    if let Some(sort) = get("sortBy") {
        let (desc, key) = match sort.strip_prefix('-') {
            Some(k) => (true, k.trim().to_string()),
            None => (false, sort),
        };
        if let Some(idx) = columns.iter().position(|(n, _)| *n == key) {
            table_rows.sort_by(|a, b| {
                let (x, y) = (&a[idx], &b[idx]);
                let ord = match (x.parse::<f64>(), y.parse::<f64>()) {
                    (Ok(nx), Ok(ny)) => nx.total_cmp(&ny),
                    _ => x.cmp(y),
                };
                if desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
        } else {
            warnings.push(format!("sortBy column `{key}` is not in the column list"));
        }
    }

    Ok(RenderedTable {
        view: view_name.to_string(),
        columns: columns.into_iter().map(|(n, _)| n).collect(),
        rows: table_rows,
        warnings,
    })
}

// --- rows -----------------------------------------------------------------

/// A row is an ordered list of (field, value) pairs.
type Row = Vec<(String, String)>;

fn field(row: &Row, name: &str) -> Option<String> {
    row.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// Models whose content produces rows: the targets, or everything when
/// no targets were given.
fn content_models(models: &[Model], targets: &[String]) -> Vec<Model> {
    if targets.is_empty() {
        return models.to_vec();
    }
    models
        .iter()
        .filter(|m| targets.iter().any(|t| t == &m.file))
        .cloned()
        .collect()
}

fn provide_rows(
    models: &[Model],
    targets: &[String],
    spec: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Row>, String> {
    // Rows come from the target files; `models` stays available for
    // resolution that must see the whole context (specialization
    // chains, imported types).
    let content = content_models(models, targets);
    let content = content.as_slice();
    if let Some(meta) = spec.strip_prefix('@') {
        return Ok(metadata_rows(content, meta));
    }
    if let Some(ty) = spec.strip_prefix("type:") {
        return Ok(typed_usage_rows(content, models, ty.trim()));
    }
    if let Some(kind) = spec.strip_prefix("kind:") {
        return Ok(kind_rows(content, kind.trim()));
    }
    if let Some(rel) = spec.strip_prefix("relation:") {
        return relation_rows(content, rel.trim());
    }
    match spec {
        "trace" => Ok(trace_rows(content)),
        "kindcounts" => Ok(kindcount_rows(content)),
        "uncertainty" => Ok(uncertainty_rows(content, models, warnings)),
        other => Err(format!(
            "unknown row provider `{other}` (expected @Metadata, type:, kind:, \
             relation:, trace, kindcounts, or uncertainty)"
        )),
    }
}

fn metadata_rows(models: &[Model], meta_type: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for m in models {
        for a in &m.annotations {
            if simple_name(&a.metadata_type) != meta_type {
                continue;
            }
            let mut row: Row = vec![
                ("element".into(), a.target.clone().unwrap_or_default()),
                ("file".into(), m.file.clone()),
                ("line".into(), (a.span.start_row + 1).to_string()),
            ];
            for (k, v) in &a.values {
                row.push((k.clone(), unquote(v).to_string()));
            }
            rows.push(row);
        }
    }
    rows
}

/// Usages typed by `ty` or any definition specializing it.
fn typed_usage_rows(content: &[Model], models: &[Model], ty: &str) -> Vec<Row> {
    let specializes = |type_name: &str| -> bool {
        let mut current = simple_name(type_name).to_string();
        let mut depth = 0;
        loop {
            if current == ty {
                return true;
            }
            depth += 1;
            if depth > 32 {
                return false;
            }
            let Some(def) = models
                .iter()
                .find_map(|m| m.definitions.iter().find(|d| d.name == current))
            else {
                return false;
            };
            match def.super_type.as_deref() {
                Some(s) => current = simple_name(s).to_string(),
                None => return false,
            }
        }
    };

    let mut rows = Vec::new();
    for m in models {
        for u in &m.usages {
            let Some(ref t) = u.type_ref else { continue };
            if u.name.is_empty() || !specializes(t) {
                continue;
            }
            let mut row: Row = vec![
                ("element".into(), u.name.clone()),
                ("type".into(), simple_name(t).to_string()),
                ("kind".into(), u.kind.clone()),
                ("parent".into(), u.parent_def.clone().unwrap_or_default()),
                ("file".into(), m.file.clone()),
                ("line".into(), (u.span.start_row + 1).to_string()),
            ];
            // Body values (`:>> description = "..."`) become fields.
            for c in body_children(m, u) {
                if let Some(ref v) = c.value_expr {
                    row.push((c.name.clone(), unquote(v).to_string()));
                }
            }
            rows.push(row);
        }
    }
    rows
}

fn kind_rows(models: &[Model], kind: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for m in models {
        for u in &m.usages {
            if u.kind != kind || u.name.is_empty() {
                continue;
            }
            rows.push(vec![
                ("element".into(), u.name.clone()),
                ("parent".into(), u.parent_def.clone().unwrap_or_default()),
                ("type".into(), u.type_ref.clone().unwrap_or_default()),
                (
                    "direction".into(),
                    u.direction
                        .map(|d| format!("{d:?}").to_lowercase())
                        .unwrap_or_default(),
                ),
                ("file".into(), m.file.clone()),
                ("line".into(), (u.span.start_row + 1).to_string()),
            ]);
        }
    }
    rows
}

fn relation_rows(models: &[Model], rel: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    match rel {
        "allocation" => {
            for m in models {
                for a in &m.allocations {
                    rows.push(vec![
                        ("source".into(), a.source.clone()),
                        ("target".into(), a.target.clone()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        "satisfy" => {
            for m in models {
                for s in &m.satisfactions {
                    rows.push(vec![
                        ("requirement".into(), s.requirement.clone()),
                        ("by".into(), s.by.clone().unwrap_or_default()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        "verify" => {
            for m in models {
                for v in &m.verifications {
                    rows.push(vec![
                        ("requirement".into(), v.requirement.clone()),
                        ("by".into(), v.by.clone()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        "connection" => {
            for m in models {
                for c in &m.connections {
                    rows.push(vec![
                        ("source".into(), c.source.clone()),
                        ("target".into(), c.target.clone()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        other => {
            return Err(format!(
                "unknown relation `{other}` (allocation, satisfy, verify, connection)"
            ))
        }
    }
    Ok(rows)
}

/// Requirement coverage: one row per requirement def. Uses the same
/// project-wide resolution as the W002/W003 checks — satisfy targets
/// resolve through usages and close over specialization, so satisfying
/// `Derived :> Base` satisfies `Base` here too.
fn trace_rows(models: &[Model]) -> Vec<Row> {
    use crate::model::target_matches;
    let project = crate::resolver::Project::from_models(models.to_vec());
    let (satisfied_set, verified_set) = project.traced_requirements();
    let mut rows = Vec::new();
    for m in models {
        for d in &m.definitions {
            if d.kind != crate::model::DefKind::Requirement {
                continue;
            }
            let traced = |set: &std::collections::HashSet<String>| -> bool {
                set.iter()
                    .any(|t| target_matches(t, &d.name, d.short_name.as_deref()))
            };
            let satisfied = traced(&satisfied_set);
            let verified = traced(&verified_set);
            rows.push(vec![
                ("requirement".into(), d.name.clone()),
                (
                    "id".into(),
                    d.short_name
                        .as_deref()
                        .map(|s| crate::model::unquote_name(s).to_string())
                        .unwrap_or_default(),
                ),
                ("satisfied".into(), if satisfied { "yes" } else { "no" }.into()),
                ("verified".into(), if verified { "yes" } else { "no" }.into()),
                ("file".into(), m.file.clone()),
            ]);
        }
    }
    rows
}

fn kindcount_rows(models: &[Model]) -> Vec<Row> {
    let mut defs: BTreeMap<String, usize> = BTreeMap::new();
    let mut usages: BTreeMap<String, usize> = BTreeMap::new();
    for m in models {
        for d in &m.definitions {
            *defs.entry(d.kind.label().to_string()).or_default() += 1;
        }
        for u in &m.usages {
            if !u.name.is_empty() {
                *usages.entry(u.kind.clone()).or_default() += 1;
            }
        }
    }
    let mut kinds: Vec<String> = defs.keys().cloned().collect();
    for k in usages.keys() {
        if !kinds.contains(k) {
            kinds.push(k.clone());
        }
    }
    kinds
        .into_iter()
        .map(|k| {
            vec![
                ("kind".into(), k.clone()),
                (
                    "definitions".into(),
                    defs.get(&k).copied().unwrap_or(0).to_string(),
                ),
                (
                    "usages".into(),
                    usages.get(&k).copied().unwrap_or(0).to_string(),
                ),
            ]
        })
        .collect()
}

/// One row per uncertainty analysis case, evaluated (worst-case + RSS).
fn uncertainty_rows(
    content: &[Model],
    models: &[Model],
    warnings: &mut Vec<String>,
) -> Vec<Row> {
    use crate::sim::uncertainty::{rss, worst_case, PassFail};
    use crate::sim::uncertainty_model::{extract_case, find_uncertainty_cases};

    let mut rows = Vec::new();
    // Cases are the user's; extraction resolves types across everything.
    for (name, _file, ty) in find_uncertainty_cases(content) {
        match extract_case(models, &name) {
            Ok(case) => {
                let wc = worst_case(&case.inputs, &case.target);
                let r = rss(&case.inputs, &case.target, &case.settings);
                let verdict = match (wc.result, r.result) {
                    (PassFail::Fail, _) | (_, PassFail::Fail) => "FAIL",
                    (PassFail::Marginal, _) | (_, PassFail::Marginal) => "MARGINAL",
                    _ => "PASS",
                };
                rows.push(vec![
                    ("case".into(), case.name.clone()),
                    ("type".into(), ty),
                    ("critical".into(), case.critical.to_string()),
                    ("nominal".into(), format_num(case.target.nominal)),
                    ("lower".into(), format_num(case.target.lower)),
                    ("upper".into(), format_num(case.target.upper)),
                    ("wcMin".into(), format_num(wc.min)),
                    ("wcMax".into(), format_num(wc.max)),
                    ("margin".into(), format_num(wc.margin)),
                    ("cp".into(), format_num(r.cp)),
                    ("cpk".into(), format_num(r.cpk)),
                    ("result".into(), verdict.into()),
                ]);
            }
            Err(e) => warnings.push(format!("case `{name}`: {e}")),
        }
    }
    rows
}

// --- pivot ------------------------------------------------------------------

fn pivot_table(view: &str, rows: &[Row], row_field: &str, col_field: &str) -> RenderedTable {
    let mut row_keys: Vec<String> = Vec::new();
    let mut col_keys: Vec<String> = Vec::new();
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in rows {
        let rv = field(r, row_field).unwrap_or_default();
        let cv = field(r, col_field).unwrap_or_default();
        if !row_keys.contains(&rv) {
            row_keys.push(rv.clone());
        }
        if !col_keys.contains(&cv) {
            col_keys.push(cv.clone());
        }
        *counts.entry((rv, cv)).or_default() += 1;
    }
    // Numeric-aware ordering, descending rows (severity 10 on top).
    let by_num = |a: &String, b: &String| match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.cmp(b),
    };
    row_keys.sort_by(|a, b| by_num(b, a));
    col_keys.sort_by(&by_num);

    let mut columns = vec![format!("{row_field}\\{col_field}")];
    columns.extend(col_keys.iter().cloned());
    let mut out_rows = Vec::new();
    for rk in &row_keys {
        let mut row = vec![rk.clone()];
        for ck in &col_keys {
            let n = counts.get(&(rk.clone(), ck.clone())).copied().unwrap_or(0);
            row.push(if n == 0 { String::new() } else { n.to_string() });
        }
        out_rows.push(row);
    }
    RenderedTable {
        view: view.to_string(),
        columns,
        rows: out_rows,
        warnings: Vec::new(),
    }
}

// --- helpers ----------------------------------------------------------------

fn find_table_annotation<'a>(
    models: &'a [Model],
    view_name: &str,
) -> Option<&'a Vec<(String, String)>> {
    models.iter().find_map(|m| {
        m.annotations
            .iter()
            .find(|a| {
                simple_name(&a.metadata_type) == "TableRendering"
                    && a.target.as_deref() == Some(view_name)
            })
            .map(|a| &a.values)
    })
}

/// Direct body members of a usage (same rule as the uncertainty extractor).
fn body_children<'a>(m: &'a Model, parent: &Usage) -> Vec<&'a Usage> {
    m.usages
        .iter()
        .filter(|u| {
            u.parent_def.as_deref() == Some(parent.name.as_str())
                && u.span.start_byte >= parent.span.start_byte
                && u.span.end_byte <= parent.span.end_byte
                && u.span.start_byte != parent.span.start_byte
        })
        .collect()
}

fn row_env(row: &Row) -> crate::sim::expr::Env {
    use crate::sim::expr::{Env, Value};
    let mut env = Env::new();
    for (k, v) in row {
        if let Ok(n) = v.parse::<f64>() {
            env.bind(k.clone(), Value::Number(n));
        } else {
            env.bind(k.clone(), Value::String(v.clone()));
        }
    }
    env
}

fn eval_num(expr: &str, row: &Row) -> Option<f64> {
    let parsed = crate::sim::expr_parser::parse_expr_str(expr).ok()?;
    crate::sim::eval::evaluate(&parsed, &row_env(row))
        .ok()
        .and_then(|v| v.as_number())
}

fn eval_bool(expr: &str, row: &Row) -> Option<bool> {
    let parsed = crate::sim::expr_parser::parse_expr_str(expr).ok()?;
    crate::sim::eval::evaluate_constraint(&parsed, &row_env(row)).ok()
}

fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.4}")
    }
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    const FMEA_MODEL: &str = r#"
        package P {
            metadata def Fmea;
            metadata def TableRendering;

            view def Worksheet {
                @TableRendering {
                    rows = "@Fmea";
                    columns = "element; failureMode; severity; occurrence; detection; rpn = severity*occurrence*detection";
                    sortBy = "-rpn";
                }
            }
            view def Matrix {
                @TableRendering {
                    rows = "@Fmea";
                    pivot = "severity; occurrence";
                }
            }
            view def HighRisk {
                @TableRendering {
                    rows = "@Fmea";
                    columns = "element; rpn = severity*occurrence*detection";
                    where = "severity >= 8";
                }
            }

            part def Battery;
            part battery : Battery {
                @Fmea {
                    failureMode = "Thermal runaway";
                    severity = 9;
                    occurrence = 3;
                    detection = 4;
                }
                @Fmea {
                    failureMode = "Capacity fade";
                    severity = 5;
                    occurrence = 6;
                    detection = 3;
                }
            }
        }
    "#;

    fn models() -> Vec<Model> {
        vec![parse_file("t.sysml", FMEA_MODEL)]
    }

    #[test]
    fn worksheet_rows_computed_and_sorted() {
        let t = render_view(&models(), &[], "Worksheet").expect("render");
        assert_eq!(
            t.columns,
            vec!["element", "failureMode", "severity", "occurrence", "detection", "rpn"]
        );
        assert_eq!(t.rows.len(), 2);
        // Sorted by RPN descending: 9*3*4=108 first, 5*6*3=90 second.
        assert_eq!(t.rows[0][5], "108");
        assert_eq!(t.rows[1][5], "90");
        assert_eq!(t.rows[0][1], "Thermal runaway");
    }

    #[test]
    fn where_filters_rows() {
        let t = render_view(&models(), &[], "HighRisk").expect("render");
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0][1], "108");
    }

    #[test]
    fn pivot_counts() {
        let t = render_view(&models(), &[], "Matrix").expect("render");
        // Rows: severity 9 and 5 (descending). Columns: occurrence 3 and 6.
        assert_eq!(t.columns[0], "severity\\occurrence");
        assert_eq!(t.columns[1..], ["3", "6"]);
        assert_eq!(t.rows[0][0], "9");
        assert_eq!(t.rows[0][1], "1"); // severity 9 x occurrence 3
        assert_eq!(t.rows[1][2], "1"); // severity 5 x occurrence 6
    }

    #[test]
    fn unknown_view_lists_available() {
        let err = render_view(&models(), &[], "Nope").unwrap_err();
        assert!(err.contains("Worksheet"), "{err}");
    }

    #[test]
    fn view_without_spec_is_explained() {
        let src = "package P { view def Empty; }";
        let ms = vec![parse_file("e.sysml", src)];
        let err = render_view(&ms, &[], "Empty").unwrap_err();
        assert!(err.contains("@TableRendering"), "{err}");
    }

    #[test]
    fn kindcounts_provider() {
        let src = r#"
            package P {
                metadata def TableRendering;
                view def Stats {
                    @TableRendering {
                        rows = "kindcounts";
                        columns = "kind; definitions; usages";
                    }
                }
                part def A;
                part def B;
                part a : A;
            }
        "#;
        let ms = vec![parse_file("s.sysml", src)];
        let t = render_view(&ms, &[], "Stats").expect("render");
        let part_row = t.rows.iter().find(|r| r[0] == "part def").expect("row");
        assert_eq!(part_row[1], "2");
    }

    #[test]
    fn typed_usage_rows_include_body_values() {
        let src = r#"
            package P {
                metadata def TableRendering;
                occurrence def Hazard {
                    attribute description : String[0..1];
                }
                view def Log {
                    @TableRendering {
                        rows = "type:Hazard";
                        columns = "element; description";
                    }
                }
                occurrence fire : Hazard {
                    :>> description = "Lithium cell fire";
                }
            }
        "#;
        let ms = vec![parse_file("h.sysml", src)];
        let t = render_view(&ms, &[], "Log").expect("render");
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0], vec!["fire", "Lithium cell fire"]);
    }
}
