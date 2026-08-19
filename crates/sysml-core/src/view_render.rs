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
    let view_exists = models
        .iter()
        .any(|m| m.views.iter().any(|v| v.name == view_name));
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
            .map(|(_, v)| unescape(unquote(v)))
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
    row.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
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
        return relation_rows(content, models, rel.trim());
    }
    if let Some(root) = spec.strip_prefix("composition:") {
        return Ok(composition_rows(content, models, root.trim()));
    }
    if spec == "composition" {
        // No root named: walk from every definition that nothing uses as
        // a type, i.e. the tops of the composition forest.
        let scope: &[Model] = if models.is_empty() { content } else { models };
        let used: std::collections::HashSet<&str> = scope
            .iter()
            .flat_map(|m| m.usages.iter())
            .filter_map(|u| u.type_ref.as_deref().map(simple_name))
            .collect();
        let mut rows = Vec::new();
        for m in content {
            for d in &m.definitions {
                if !used.contains(d.name.as_str()) {
                    rows.extend(composition_rows(content, models, &d.name));
                }
            }
        }
        return Ok(rows);
    }
    match spec {
        "trace" => Ok(trace_rows(content)),
        "kindcounts" => Ok(kindcount_rows(content)),
        "uncertainty" => Ok(uncertainty_rows(content, models, warnings)),
        other => Err(format!(
            "unknown row provider `{other}` (expected @Metadata, type:, kind:, \
             relation:, composition:, trace, kindcounts, or uncertainty)"
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

    // Rows come from the target files; `models` above resolves the
    // specialization chain across the whole context.
    let mut rows = Vec::new();
    for m in content {
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

/// True when `type_name` is, or transitively specializes, `target`.
/// Resolution walks `models` (the whole include path), so a project type
/// declared against a library supertype still matches.
fn specializes_type(models: &[Model], type_name: &str, target: &str) -> bool {
    let mut current = simple_name(type_name).to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if current == target {
            return true;
        }
        if !seen.insert(current.clone()) {
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
}

fn relation_rows(content: &[Model], models: &[Model], rel: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    // `connection:Mate` restricts to connections of that type (or a
    // specialization). Without it a "fit table" would list every
    // connection in the model, hazard causations included.
    let (rel, type_filter) = match rel.split_once(':') {
        Some((base, ty)) => (base.trim(), Some(ty.trim())),
        None => (rel, None),
    };
    if type_filter.is_some() && !matches!(rel, "connection" | "succession") {
        return Err(format!(
            "relation `{rel}` takes no type filter (only `connection:<Type>` \
             and `succession:<Type>` do)"
        ));
    }
    let models = if models.is_empty() { content } else { models };
    match rel {
        "allocation" => {
            for m in content {
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
            for m in content {
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
            for m in content {
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
            for m in content {
                for c in &m.connections {
                    match (type_filter, c.type_ref.as_deref()) {
                        (Some(want), Some(have)) if specializes_type(models, have, want) => {}
                        (Some(_), _) => continue,
                        (None, _) => {}
                    }
                    rows.push(vec![
                        ("connection".into(), c.name.clone().unwrap_or_default()),
                        (
                            "type".into(),
                            c.type_ref.as_deref().map(simple_name).unwrap_or("").into(),
                        ),
                        ("source".into(), c.source.clone()),
                        ("target".into(), c.target.clone()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        // The dependency edges of an action flow. A named succession keeps
        // its name *and* its endpoints, so this is the graph a scheduler or
        // critical-path export reads.
        "succession" => {
            for m in content {
                for u in &m.usages {
                    if u.kind != "succession" {
                        continue;
                    }
                    let (Some(src), Some(tgt)) = (u.source.as_deref(), u.target.as_deref()) else {
                        continue;
                    };
                    match (type_filter, u.type_ref.as_deref()) {
                        (Some(want), Some(have)) if specializes_type(models, have, want) => {}
                        (Some(_), _) => continue,
                        (None, _) => {}
                    }
                    rows.push(vec![
                        ("succession".into(), u.name.clone()),
                        (
                            "type".into(),
                            u.type_ref.as_deref().map(simple_name).unwrap_or("").into(),
                        ),
                        ("source".into(), src.to_string()),
                        ("target".into(), tgt.to_string()),
                        ("parent".into(), u.parent_def.clone().unwrap_or_default()),
                        ("file".into(), m.file.clone()),
                    ]);
                }
            }
        }
        other => {
            return Err(format!(
                "unknown relation `{other}` \
                 (allocation, satisfy, verify, connection, succession)"
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
                (
                    "satisfied".into(),
                    if satisfied { "yes" } else { "no" }.into(),
                ),
                (
                    "verified".into(),
                    if verified { "yes" } else { "no" }.into(),
                ),
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
fn uncertainty_rows(content: &[Model], models: &[Model], warnings: &mut Vec<String>) -> Vec<Row> {
    use crate::sim::uncertainty::{rss, worst_case, PassFail};
    use crate::sim::uncertainty_model::{extract_case, find_uncertainty_cases};

    let mut rows = Vec::new();
    // Cases are the user's; extraction resolves types across everything.
    for (name, _file, ty) in find_uncertainty_cases(content, models) {
        match extract_case(models, &name) {
            Ok(case) => {
                let wc = worst_case(&case.inputs, &case.target, &case.settings);
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
            // Enum values arrive qualified (`RiskCategory::software`) but
            // a `where` clause is written with the name a person says, so
            // bind the simple name — the same loose comparison
            // `list --metadata --where` uses. The displayed column keeps
            // the qualified value.
            env.bind(k.clone(), Value::String(simple_name(v).to_string()));
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

/// Strip one layer of quoting. `trim_matches` would strip every trailing
/// quote, which eats the closing quote of a nested string: the spec
/// `"category == \"software\""` ends in two quotes and only the outer
/// one is delimiting.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    match (s.strip_prefix('"'), s.strip_suffix('"')) {
        (Some(_), Some(_)) if s.len() >= 2 => &s[1..s.len() - 1],
        _ => s,
    }
}

/// Resolve the escapes a SysML string literal carries, so a spec that
/// nests a string — `where = "category == \"software\""` — reaches the
/// expression parser as the expression the author wrote.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The composition tree under `root`, one row per part or item usage:
/// a bill of materials. Quantity comes from the usage's multiplicity and
/// `extended` multiplies it down the tree, so a part used 2x inside an
/// assembly used 3x reports 6.
///
/// Only `part` and `item` usages are walked. Connections and flows are
/// structure, not content — including them is what made a "BOM" list
/// connectors alongside parts.
///
/// Structure only: which attributes matter (mass, cost, supplier) is the
/// model's business, so a view names them as columns and they are read
/// off the usage's type.
fn composition_rows(content: &[Model], models: &[Model], root: &str) -> Vec<Row> {
    use crate::sim::resolve::quantity_from_multiplicity;

    let scope: &[Model] = if models.is_empty() { content } else { models };
    let mut rows = Vec::new();
    let mut stack = vec![(root.to_string(), String::new(), 0usize, 1u32)];
    let mut seen = std::collections::HashSet::new();

    while let Some((def_name, path, depth, mult)) = stack.pop() {
        // Cycle guard: a type that contains itself would recurse forever.
        if !seen.insert((def_name.clone(), depth)) || depth > 64 {
            continue;
        }
        for m in scope {
            for u in m.usages_in_def(&def_name) {
                if !matches!(u.kind.as_str(), "part" | "item") || u.name.is_empty() {
                    continue;
                }
                let ty = u.type_ref.as_deref().map(simple_name).unwrap_or("");
                let qty = quantity_from_multiplicity(u);
                let extended = mult.saturating_mul(qty);
                let child_path = if path.is_empty() {
                    u.name.clone()
                } else {
                    format!("{path}.{}", u.name)
                };
                let mut row: Row = vec![
                    ("element".into(), u.name.clone()),
                    ("type".into(), ty.to_string()),
                    ("parent".into(), def_name.clone()),
                    ("path".into(), child_path.clone()),
                    ("depth".into(), depth.to_string()),
                    ("quantity".into(), qty.to_string()),
                    ("extended".into(), extended.to_string()),
                    ("file".into(), m.file.clone()),
                ];
                // Attributes of the usage's type, so a view can ask for
                // `mass` or `cost` as a column without this code knowing
                // which attributes exist.
                if !ty.is_empty() {
                    for am in scope {
                        for a in am.usages_in_def(ty) {
                            if a.kind == "attribute" && !a.name.is_empty() {
                                if let Some(v) = a.value_expr.as_deref() {
                                    if !row.iter().any(|(k, _)| k == &a.name) {
                                        row.push((a.name.clone(), unquote(v.trim()).to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
                rows.push(row);
                if !ty.is_empty() {
                    stack.push((ty.to_string(), child_path, depth + 1, extended));
                }
            }
        }
    }
    rows
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
            vec![
                "element",
                "failureMode",
                "severity",
                "occurrence",
                "detection",
                "rpn"
            ]
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

    /// Rows come from the target file, but types resolve against the
    /// whole include path. Scoping rows to the target without giving the
    /// resolver the libraries made every stackup invisible: the case is
    /// declared in the model, `ToleranceStackup :> UncertaintyAnalysis`
    /// only in the library.
    #[test]
    fn uncertainty_rows_resolve_types_outside_the_target() {
        let lib = r#"
            package Uncertainty {
                metadata def TableRendering;
                analysis def UncertaintyAnalysis;
                analysis def ToleranceStackup :> UncertaintyAnalysis;
            }
        "#;
        let target = r#"
            package M {
                part def Asm {
                    attribute d1 { :>> nominal = 10.0; :>> plus = 0.1; :>> minus = 0.1; }
                    analysis gap : ToleranceStackup {
                        attribute :>> target {
                            :>> nominal = 10.0; :>> lower = 9.0; :>> upper = 11.0;
                        }
                        attribute c1 :> contributions { :>> dim = d1; }
                    }
                }
                view def Stack {
                    @TableRendering {
                        rows = "uncertainty";
                        columns = "case; nominal; result";
                    }
                }
            }
        "#;
        let ms = vec![
            parse_file("lib.sysml", lib),
            parse_file("target.sysml", target),
        ];
        let out = render_view(&ms, &["target.sysml".to_string()], "Stack").unwrap();
        assert_eq!(out.rows.len(), 1, "case should be found: {out:?}");
        assert_eq!(out.rows[0][0], "gap");
    }

    /// A `where` clause comparing strings — filtering a worksheet to one
    /// category is the ordinary case, and the value arrives qualified
    /// (`RiskCategory::software`) while the author writes the plain name.
    #[test]
    fn where_clause_compares_strings() {
        let src = r#"
            package P {
                metadata def TableRendering;
                metadata def Line { attribute failureMode : String; attribute category : String; }
                part w {
                    @Line { failureMode = "A"; category = RiskCategory::software; }
                    @Line { failureMode = "B"; category = RiskCategory::design; }
                }
                view def SW {
                    @TableRendering {
                        rows = "@Line";
                        columns = "failureMode; category";
                        where = "category == \"software\"";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("w.sysml", src)];
        let out = render_view(&ms, &[], "SW").unwrap();
        let names: Vec<&str> = out.rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(names, vec!["A"], "{out:?}");
    }

    /// A "fit table" must not list hazard causations. Before connections
    /// carried a type, `relation:connection` returned every connection in
    /// the model and the filter had nothing to key on.
    #[test]
    fn connection_rows_filter_by_type() {
        let src = r#"
            package P {
                metadata def TableRendering;
                part def Mate;
                part def Causation;
                part def Bolted :> Mate;
                part a; part b; part c; part d; part e; part f;
                connection m1 : Mate connect a to b;
                connection m2 : Bolted connect c to d;
                connection k1 : Causation connect e to f;
                view def Fits {
                    @TableRendering {
                        rows = "relation:connection:Mate";
                        columns = "connection; source; target";
                        sortBy = "connection";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("f.sysml", src)];
        let out = render_view(&ms, &[], "Fits").unwrap();
        let names: Vec<&str> = out.rows.iter().map(|r| r[0].as_str()).collect();
        // Bolted specializes Mate, so it is a fit; Causation is not.
        assert_eq!(names, vec!["m1", "m2"], "{out:?}");
    }

    /// A named succession keeps its name, its type, *and* its endpoints.
    /// The endpoints used to be stored in `name`/`type_ref`, so naming a
    /// succession silently discarded the edge it declared.
    #[test]
    fn succession_rows_keep_name_type_and_endpoints() {
        let src = r#"
            package P {
                metadata def TableRendering;
                action def Dependency;
                action def StartToStart :> Dependency;
                action def Prog {
                    action a; action b; action c; action d;
                    succession s1 first a then b;
                    first b then c;
                    succession s2 : StartToStart first c then d;
                }
                view def Links {
                    @TableRendering {
                        rows = "relation:succession";
                        columns = "succession; type; source; target";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("f.sysml", src)];
        let out = render_view(&ms, &[], "Links").unwrap();
        let got: Vec<Vec<&str>> = out
            .rows
            .iter()
            .map(|r| r.iter().map(|c| c.as_str()).collect())
            .collect();
        assert_eq!(
            got,
            vec![
                vec!["s1", "", "a", "b"],
                // The anonymous form is still an edge, just an unnamed one.
                vec!["", "", "b", "c"],
                // The type must not be mistaken for an endpoint.
                vec!["s2", "StartToStart", "c", "d"],
            ],
            "{out:?}"
        );
    }

    /// Dependency kind is a type on the succession, so the same type filter
    /// that connections use applies — closing over specialization.
    #[test]
    fn succession_rows_filter_by_type() {
        let src = r#"
            package P {
                metadata def TableRendering;
                action def Dependency;
                action def StartToStart :> Dependency;
                action def Prog {
                    action a; action b; action c; action d;
                    succession plain first a then b;
                    succession ss : StartToStart first c then d;
                }
                view def Deps {
                    @TableRendering {
                        rows = "relation:succession:Dependency";
                        columns = "succession; source; target";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("f.sysml", src)];
        let out = render_view(&ms, &[], "Deps").unwrap();
        let names: Vec<&str> = out.rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(names, vec!["ss"], "{out:?}");
    }

    /// Endpoints may be feature chains, not just plain names.
    #[test]
    fn succession_endpoints_may_be_feature_chains() {
        let src = r#"
            package P {
                metadata def TableRendering;
                action def Prog {
                    action a; action b;
                    first a.inner then b.inner;
                }
                view def Links {
                    @TableRendering {
                        rows = "relation:succession";
                        columns = "source; target";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("f.sysml", src)];
        let out = render_view(&ms, &[], "Links").unwrap();
        assert_eq!(out.rows.len(), 1, "{out:?}");
        assert_eq!(out.rows[0], vec!["a.inner", "b.inner"], "{out:?}");
    }

    /// The BOM is parts and items only, with quantity multiplied down the
    /// tree. Connections are structure, not content.
    #[test]
    fn composition_rows_are_a_bom() {
        let src = r#"
            package P {
                metadata def TableRendering;
                part def Wheel;
                part def Axle { part wheel : Wheel[2]; }
                part def Car { part axle : Axle[2]; connection j : Wheel connect axle to axle; }
                view def Bom {
                    @TableRendering {
                        rows = "composition:Car";
                        columns = "path; quantity; extended";
                        sortBy = "path";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("b.sysml", src)];
        let out = render_view(&ms, &[], "Bom").unwrap();
        let got: Vec<(&str, &str)> = out
            .rows
            .iter()
            .map(|r| (r[0].as_str(), r[2].as_str()))
            .collect();
        assert_eq!(
            got,
            vec![("axle", "2"), ("axle.wheel", "4")],
            "2 axles x 2 wheels = 4 extended, and no connection row: {out:?}"
        );
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
