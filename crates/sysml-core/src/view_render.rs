//! Generic view rendering: the tool-side interpreter of SysML views.
//!
//! There is no report engine and no per-report code. A `view def` carries
//! a `@TableRendering` annotation describing rows, columns, sort, and
//! pivot; this module evaluates that spec against the model and returns a
//! rendered table. Domain packages ship their standard views
//! (FmeaWorksheet, StackupSummary, ...) as models — adding a new report
//! anywhere requires zero tool changes.
//!
//! A `view` usage typed by such a def renders too: it inherits the def's
//! `@TableRendering` (its own annotation, if any, overrides individual
//! fields) and its `expose` / `filter` clauses narrow the rows to part of
//! the model.
//!
//! `@TableRendering` fields (all strings, `;`-separated lists so column
//! expressions may contain commas):
//!
//!   rows    — the row provider:
//!               "@Fmea"                annotations of a metadata type
//!               "type:Hazard"          usages typed by a definition
//!                                       (or any of its specializations)
//!               "kind:port"            usages of a syntactic kind, with
//!                                       derived direction and connections
//!               "relation:allocation"  allocation pairs
//!               "relation:satisfy"     satisfy relationships
//!               "relation:verify"      verify relationships
//!               "relation:connection"  connections
//!               "relation:succession"  successions
//!               "composition[:Root]"   the part/item tree (a BOM)
//!               "requirements"         requirements with their attributes,
//!                                       status and trace links
//!               "trace"                requirement coverage rows
//!               "kindcounts"           definitions/usages grouped by kind
//!               "uncertainty"          uncertainty analysis results
//!   columns — "name; name = expression; ..." — bare names copy row
//!             fields; expressions evaluate against the row's numeric
//!             fields via the shared evaluator
//!   sortBy  — column name, "-" prefix for descending
//!   where   — boolean expression filtering rows
//!   pivot   — "rowField; colField" — grid of row counts

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::model::{simple_name, unquote_name, DefKind, Model, Usage, ViewDef};
use crate::view_edit::{self, ColumnEdit, FieldSpec, FieldType, ValueKind};

/// A rendered table, ready for text/json/csv/markdown output.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RenderedTable {
    pub view: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Warnings produced while rendering (unknown columns, eval errors).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Where each row comes from, parallel to `rows` (empty for pivots),
    /// so a viewer can link a row back to its element.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<RowSource>,
    /// Per column, how its cells are edited — `None` for computed,
    /// derived and trace columns (and every pivot column).
    #[serde(skip)]
    pub editable: Vec<Option<ColumnEdit>>,
    /// The fields the row provider offers, for choosing columns.
    #[serde(skip)]
    pub available_fields: Vec<String>,
}

/// The model element behind one table row.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct RowSource {
    /// Qualified name of the row's element (empty when the row has none).
    pub element: String,
    pub file: String,
    /// 1-based line, 0 when unknown.
    pub line: usize,
}

/// A view found in the models: a `view def` or a `view` usage.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    pub file: String,
    pub is_usage: bool,
    /// Whether a `@TableRendering` applies (own or inherited).
    pub has_table: bool,
    /// For a usage, the view def it is typed by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// The `render` clause, if any (e.g. `asTreeDiagram`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_as: Option<String>,
    /// The `expose` targets as written (`P`, `P::*`, `P::**`), so a front
    /// end can draw a diagram view from the elements it exposes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exposes: Vec<String>,
}

/// Every view def and view usage across the models.
pub fn list_views(models: &[Model]) -> Vec<ViewInfo> {
    let idx = Index::new(models);
    let mut out = Vec::new();
    for m in models {
        for v in &m.views {
            let path = idx.path_at(&m.file, v.span.start_byte);
            out.push(ViewInfo {
                name: unquote_name(&v.name).to_string(),
                qualified_name: (!path.is_empty()).then(|| display_qualified(&path)),
                file: m.file.clone(),
                is_usage: v.is_usage,
                has_table: table_spec(models, v, 0).is_some(),
                definition: if v.is_usage {
                    v.type_ref.as_deref().map(|t| simple_name(t).to_string())
                } else {
                    None
                },
                doc: v.doc.as_deref().map(collapse_ws),
                render_as: v.render_as.clone(),
                exposes: v.exposes.clone(),
            });
        }
    }
    out
}

/// All views (name, source file, has table spec) across the models.
pub fn available_views(models: &[Model]) -> Vec<(String, String, bool)> {
    list_views(models)
        .into_iter()
        .map(|v| (v.name, v.file, v.has_table))
        .collect()
}

/// Render a model-defined view (a `view def` or a `view` usage), found by
/// simple or qualified name.
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
    let idx = Index::new(models);
    let Some(view) = find_view(models, &idx, view_name) else {
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
    };
    let shown_name = unquote_name(&view.name).to_string();
    let ann = table_spec(models, view, 0).ok_or_else(|| {
        let typed = match (view.is_usage, view.type_ref.as_deref()) {
            (true, Some(t)) => format!(" (typed by `{}`)", simple_name(t)),
            _ => String::new(),
        };
        format!(
            "view `{shown_name}`{typed} has no @TableRendering annotation — nothing \
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
    let rows = provide_rows(&idx, models, targets, &rows_spec, &mut warnings)?;
    let probe = Probe::new(&rows_spec, &rows);

    // Narrow to what the view exposes and keeps.
    let rows = apply_scope(models, view, rows, &rows_spec, &mut warnings);

    // Filter with `where`.
    let rows: Vec<Row> = if let Some(cond) = get("where") {
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
        let mut t = pivot_table(&shown_name, &rows, parts[0], parts[1], get("sortBy"));
        t.warnings = warnings;
        t.editable = vec![None; t.columns.len()];
        t.available_fields = probe.fields;
        return Ok(t);
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

    let mut table: Vec<(Vec<String>, RowSource)> = Vec::new();
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
        table.push((out_row, row.source()));
    }

    // Sort.
    if let Some(sort) = get("sortBy") {
        let (desc, key) = match sort.strip_prefix('-') {
            Some(k) => (true, k.trim().to_string()),
            None => (false, sort),
        };
        if let Some(idx) = columns.iter().position(|(n, _)| *n == key) {
            table.sort_by(|(a, _), (b, _)| {
                let ord = cmp_cells(&a[idx], &b[idx]);
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

    let editable = columns
        .iter()
        .map(|(name, expr)| column_edit(models, &rows_spec, &probe, name, expr.as_deref()))
        .collect();
    let (rows, sources): (Vec<_>, Vec<_>) = table.into_iter().unzip();
    Ok(RenderedTable {
        view: shown_name,
        columns: columns.into_iter().map(|(n, _)| n).collect(),
        rows,
        warnings,
        sources,
        editable,
        available_fields: probe.fields,
    })
}

// --- editing --------------------------------------------------------------------

/// What the provider's rows offer, gathered before `expose` and `where`
/// narrow them: their field names and the types of their elements.
struct Probe {
    fields: Vec<String>,
    types: Vec<String>,
}

/// The fields of a `requirements` row, for a project with none yet.
const REQUIREMENT_FIELDS: [&str; 19] = [
    "element",
    "qualifiedName",
    "id",
    "title",
    "text",
    "type",
    "status",
    "owner",
    "rationale",
    "issues",
    "satisfiedBy",
    "verifiedBy",
    "satisfied",
    "verified",
    "file",
    "line",
    "kind",
    "priority",
    "verificationMethod",
];

impl Probe {
    fn new(spec: &str, rows: &[Row]) -> Self {
        let mut fields: Vec<String> = Vec::new();
        let mut types: Vec<String> = Vec::new();
        if spec == "requirements" || spec == "trace" {
            if spec == "trace" {
                fields.push("requirement".into());
            }
            fields.extend(REQUIREMENT_FIELDS.iter().map(|f| f.to_string()));
        }
        if let Some(t) = spec.strip_prefix("type:") {
            types.push(t.trim().to_string());
        }
        for r in rows {
            for (k, _) in &r.fields {
                if !fields.contains(k) {
                    fields.push(k.clone());
                }
            }
            if let Some(t) = field(r, "type").filter(|t| !t.is_empty()) {
                if !types.contains(&t) {
                    types.push(t);
                }
            }
        }
        Probe { fields, types }
    }

    fn has(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f == name)
    }
}

/// How cells of the column `name` (`name = expr`) are edited, if they
/// can be: only columns that copy a field of the row's own element.
fn column_edit(
    models: &[Model],
    spec: &str,
    probe: &Probe,
    name: &str,
    expr: Option<&str>,
) -> Option<ColumnEdit> {
    use ValueKind::{LongText, Text};
    // `description = doc` copies a field under another name.
    let f = match expr {
        None => name,
        Some(e) if view_edit::is_plain_identifier(e.trim()) && probe.has(e.trim()) => e.trim(),
        Some(_) => return None,
    };
    let plain = |s: FieldSpec, k: ValueKind| Some(ColumnEdit::new(&s, &FieldType::default(), k));
    let feature = |always: bool| feature_edit(models, probe, f, always);
    match spec {
        "requirements" | "trace" => match f {
            "id" => plain(FieldSpec::ShortName, Text),
            "title" | "element" | "requirement" => plain(FieldSpec::Name, Text),
            "text" => plain(FieldSpec::Doc, LongText),
            "status" | "owner" => metadata_edit(models, probe, "StatusInfo", f, true),
            // A `rationale` attribute when the requirements declare one,
            // else the standard @Rationale.
            "rationale" => feature_edit(models, probe, f, false)
                .filter(|_| declared_anywhere(models, probe, f))
                .or_else(|| {
                    metadata_edit(models, probe, "Rationale", "text", true).map(|mut e| {
                        e.kind = LongText;
                        e
                    })
                }),
            "qualifiedName" | "type" | "issues" | "satisfiedBy" | "verifiedBy" | "satisfied"
            | "verified" | "file" | "line" => None,
            _ => feature(["kind", "priority", "verificationMethod"].contains(&f)),
        },
        _ if spec.starts_with('@') => match f {
            "element" | "file" | "line" => None,
            _ => metadata_edit(models, probe, spec[1..].trim(), f, false),
        },
        _ if spec.starts_with("type:") => match f {
            "element" => plain(FieldSpec::Name, Text),
            "type" | "kind" | "parent" | "file" | "line" => None,
            _ => feature(false),
        },
        _ if spec.starts_with("kind:") => match f {
            "element" => plain(FieldSpec::Name, Text),
            "doc" => plain(FieldSpec::Doc, LongText),
            _ => None,
        },
        _ => None,
    }
}

fn declared_anywhere(models: &[Model], probe: &Probe, feature: &str) -> bool {
    probe
        .types
        .iter()
        .any(|t| view_edit::declared_feature(models, t, feature).is_some())
}

/// A body value `feature:<f>`: declared as an attribute by a row's type,
/// or already set on some row (or `always`, for the standard columns).
fn feature_edit(models: &[Model], probe: &Probe, f: &str, always: bool) -> Option<ColumnEdit> {
    let spec = FieldSpec::Feature(f.to_string());
    for t in &probe.types {
        if let Some(decl) = view_edit::declared_feature(models, t, f) {
            if decl.kind != "attribute" {
                return None;
            }
            let ty = view_edit::feature_type(models, t, f).unwrap_or_default();
            return Some(ColumnEdit::new(&spec, &ty, ValueKind::Text));
        }
    }
    if !always && !probe.has(f) {
        return None;
    }
    let ty = view_edit::feature_type_from_values(models, f).unwrap_or_default();
    Some(ColumnEdit::new(&spec, &ty, ValueKind::Text))
}

/// A metadata field `metadata:<meta>.<f>`, declared by the metadata def
/// or given on some row.
fn metadata_edit(
    models: &[Model],
    probe: &Probe,
    meta: &str,
    f: &str,
    always: bool,
) -> Option<ColumnEdit> {
    let spec = FieldSpec::Metadata {
        ty: meta.to_string(),
        field: f.to_string(),
    };
    if let Some(decl) = view_edit::declared_feature(models, meta, f) {
        if decl.kind != "attribute" {
            return None;
        }
        let ty = view_edit::metadata_field_type(models, meta, f).unwrap_or_default();
        return Some(ColumnEdit::new(&spec, &ty, ValueKind::Text));
    }
    if !always && !probe.has(f) {
        return None;
    }
    let ty = view_edit::metadata_type_from_values(models, meta, f).unwrap_or_default();
    Some(ColumnEdit::new(&spec, &ty, ValueKind::Text))
}

/// Numbers compare numerically, everything else as text.
fn cmp_cells(x: &str, y: &str) -> std::cmp::Ordering {
    match (x.parse::<f64>(), y.parse::<f64>()) {
        (Ok(nx), Ok(ny)) => nx.total_cmp(&ny),
        _ => x.cmp(y),
    }
}

// --- finding views and their specs -------------------------------------------

fn find_view<'a>(models: &'a [Model], idx: &Index, name: &str) -> Option<&'a ViewDef> {
    let wanted = split_path(name);
    let all = || {
        models
            .iter()
            .flat_map(|m| m.views.iter().map(move |v| (m, v)))
    };
    // A qualified name picks one view; a simple name the first view
    // with that name.
    if wanted.len() > 1 {
        if let Some((_, v)) =
            all().find(|(m, v)| idx.path_at(&m.file, v.span.start_byte).ends_with(&wanted))
        {
            return Some(v);
        }
    }
    let simple = wanted.last()?;
    all()
        .find(|(_, v)| unquote_name(&v.name) == simple)
        .map(|(_, v)| v)
}

/// The view def named `name` (simple or qualified reference).
fn find_view_def<'a>(models: &'a [Model], name: &str) -> Option<&'a ViewDef> {
    let simple = unquote_name(simple_name(name));
    models
        .iter()
        .flat_map(|m| &m.views)
        .find(|v| !v.is_usage && unquote_name(&v.name) == simple)
}

/// The `@TableRendering` values that apply to a view: those inherited
/// from the def it is typed by (or specializes), overridden field by
/// field by its own annotation.
fn table_spec(models: &[Model], view: &ViewDef, depth: usize) -> Option<Vec<(String, String)>> {
    if depth > 16 {
        return None;
    }
    let own = find_table_annotation(models, &view.name);
    let inherited = view
        .type_ref
        .as_deref()
        .and_then(|t| find_view_def(models, t))
        .filter(|d| !std::ptr::eq(*d, view))
        .and_then(|d| table_spec(models, d, depth + 1));
    match (inherited, own) {
        (None, None) => None,
        (Some(i), None) => Some(i),
        (None, Some(o)) => Some(o.clone()),
        (Some(mut i), Some(o)) => {
            for (k, v) in o {
                match i.iter_mut().find(|(ik, _)| ik == k) {
                    Some(slot) => slot.1 = v.clone(),
                    None => i.push((k.clone(), v.clone())),
                }
            }
            Some(i)
        }
    }
}

fn find_table_annotation<'a>(
    models: &'a [Model],
    view_name: &str,
) -> Option<&'a Vec<(String, String)>> {
    let name = unquote_name(view_name);
    models.iter().find_map(|m| {
        m.annotations
            .iter()
            .find(|a| {
                simple_name(&a.metadata_type) == "TableRendering"
                    && a.target.as_deref().map(unquote_name) == Some(name)
            })
            .map(|a| &a.values)
    })
}

/// The views whose clauses apply, most specific first: the view itself,
/// then the def it is typed by, then that def's supertypes.
fn view_chain<'a>(models: &'a [Model], view: &'a ViewDef) -> Vec<&'a ViewDef> {
    let mut chain = vec![view];
    let mut current = view;
    while let Some(next) = current
        .type_ref
        .as_deref()
        .and_then(|t| find_view_def(models, t))
    {
        if chain.iter().any(|v| std::ptr::eq(*v, next)) || chain.len() > 16 {
            break;
        }
        chain.push(next);
        current = next;
    }
    chain
}

// --- expose / filter ---------------------------------------------------------

/// One `expose` clause.
enum Expose {
    /// `expose A::B` — the element itself.
    Element(Vec<String>),
    /// `expose A::B::*` — its direct members.
    Members(Vec<String>),
    /// `expose A::B::**` — everything nested in it.
    Recursive(Vec<String>),
}

impl Expose {
    fn parse(text: &str) -> Self {
        let t = text.trim();
        if let Some(base) = t.strip_suffix("::**") {
            Expose::Recursive(split_path(base))
        } else if let Some(base) = t.strip_suffix("::*") {
            Expose::Members(split_path(base))
        } else {
            Expose::Element(split_path(t))
        }
    }

    /// Exposed names may be partially qualified (written relative to an
    /// import), so a path matches when it ends with the exposed name.
    fn matches(&self, scope: &Scope) -> bool {
        let path = &scope.path;
        let (owner, element) = if scope.named && !path.is_empty() {
            (&path[..path.len() - 1], Some(path.as_slice()))
        } else {
            (path.as_slice(), None)
        };
        match self {
            Expose::Element(x) => element.is_some_and(|p| p.ends_with(x)),
            Expose::Members(x) => owner.ends_with(x),
            Expose::Recursive(x) => {
                !x.is_empty() && (0..owner.len()).any(|i| owner[i..].starts_with(x))
            }
        }
    }
}

/// Keep the rows a view exposes and its filters accept. A view without
/// `expose` shows the whole model (the def-only behaviour).
fn apply_scope(
    models: &[Model],
    view: &ViewDef,
    rows: Vec<Row>,
    rows_spec: &str,
    warnings: &mut Vec<String>,
) -> Vec<Row> {
    let chain = view_chain(models, view);
    // The most specific non-empty expose list wins: a usage narrows what
    // its def exposes.
    let exposes: Vec<Expose> = chain
        .iter()
        .find(|v| !v.exposes.is_empty())
        .map(|v| v.exposes.iter().map(|e| Expose::parse(e)).collect())
        .unwrap_or_default();
    // Filters accumulate: every filter on the view and its defs applies.
    let filters: Vec<&String> = chain.iter().flat_map(|v| &v.filters).collect();
    if exposes.is_empty() && filters.is_empty() {
        return rows;
    }
    if rows.iter().any(|r| r.scopes.is_empty()) {
        warnings.push(format!(
            "rows from `{rows_spec}` have no owning element; expose/filter not applied to them"
        ));
    }
    let mut unsupported: Vec<String> = Vec::new();
    let kept = rows
        .into_iter()
        .filter(|r| {
            if r.scopes.is_empty() {
                return true;
            }
            let exposed = exposes.is_empty()
                || r.scopes
                    .iter()
                    .any(|s| exposes.iter().any(|e| e.matches(s)));
            exposed
                && filters
                    .iter()
                    .all(|f| filter_accepts(models, f, r, &mut unsupported))
        })
        .collect();
    unsupported.sort();
    unsupported.dedup();
    for u in unsupported {
        warnings.push(format!("filter `{u}` is not supported for tables; ignored"));
    }
    kept
}

/// A filter condition is a disjunction (`or`) of terms; a term is a
/// metaclass (`@SysML::PartUsage`), a metadata type the element is
/// annotated with (`@Fmea`), or `kind = part`. Other expressions are
/// reported and ignored.
fn filter_accepts(
    models: &[Model],
    filter: &str,
    row: &Row,
    unsupported: &mut Vec<String>,
) -> bool {
    let named: Vec<&Scope> = row.scopes.iter().filter(|s| s.named).collect();
    let mut understood = true;
    let accepted = filter.split(" or ").any(|term| {
        let term = term
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        if let Some(rest) = term.split_once("kind").map(|(_, r)| r) {
            let kind: String = rest
                .trim_start_matches([' ', '='])
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !kind.is_empty() {
                let kind = kind.to_lowercase();
                return named
                    .iter()
                    .any(|s| s.metaclass.to_lowercase().starts_with(&kind));
            }
        }
        let Some(meta) = term.strip_prefix('@') else {
            understood = false;
            return false;
        };
        if !meta
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':' || c == '\'')
        {
            understood = false;
            return false;
        }
        let meta = unquote_name(simple_name(meta));
        match meta {
            "Element" => true,
            "Usage" | "Definition" => named.iter().any(|s| s.metaclass.ends_with(meta)),
            m if m.ends_with("Usage") || m.ends_with("Definition") || m == "Package" => {
                named.iter().any(|s| s.metaclass == m)
            }
            m => named.iter().any(|s| {
                let Some(element) = s.path.last() else {
                    return false;
                };
                models.iter().flat_map(|md| &md.annotations).any(|a| {
                    simple_name(&a.metadata_type) == m
                        && a.target
                            .as_deref()
                            .is_some_and(|t| unquote_name(simple_name(t)) == element)
                })
            }),
        }
    });
    if !understood && !accepted {
        unsupported.push(filter.to_string());
        return true;
    }
    accepted
}

// --- the namespace index -------------------------------------------------------

/// A named element with its source extent, for answering "what is this
/// position inside of?" — the owning-namespace chain that `expose`
/// matches against.
struct NsEntry {
    start: usize,
    end: usize,
    name: String,
    metaclass: String,
}

struct Index {
    by_file: HashMap<String, Vec<NsEntry>>,
    /// Unquoted name or short name -> (path, metaclass); the shallowest
    /// declaration wins so a package member beats a nested namesake.
    names: HashMap<String, (Vec<String>, String)>,
}

impl Index {
    fn new(models: &[Model]) -> Self {
        let mut by_file: HashMap<String, Vec<NsEntry>> = HashMap::new();
        for m in models {
            let entries = by_file.entry(m.file.clone()).or_default();
            for d in &m.definitions {
                if d.name.is_empty() {
                    continue;
                }
                entries.push(NsEntry {
                    start: d.span.start_byte,
                    end: d.span.end_byte,
                    name: unquote_name(&d.name).to_string(),
                    metaclass: def_metaclass(d.kind),
                });
            }
            for u in &m.usages {
                if u.name.is_empty() || u.kind == "metadata" {
                    continue;
                }
                entries.push(NsEntry {
                    start: u.span.start_byte,
                    end: u.span.end_byte,
                    name: unquote_name(&u.name).to_string(),
                    metaclass: usage_metaclass(&u.kind),
                });
            }
            // Outer before inner: by start, then longest first.
            entries.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
        }
        let mut idx = Index {
            by_file,
            names: HashMap::new(),
        };
        let mut names: HashMap<String, (Vec<String>, String)> = HashMap::new();
        for m in models {
            let mut add = |key: &str, start: usize, metaclass: String| {
                let path = idx.path_at(&m.file, start);
                let better = names
                    .get(key)
                    .is_none_or(|(existing, _)| path.len() < existing.len());
                if better && !path.is_empty() {
                    names.insert(key.to_string(), (path, metaclass));
                }
            };
            for d in &m.definitions {
                add(
                    unquote_name(&d.name),
                    d.span.start_byte,
                    def_metaclass(d.kind),
                );
                if let Some(sn) = d.short_name.as_deref() {
                    add(unquote_name(sn), d.span.start_byte, def_metaclass(d.kind));
                }
            }
            for u in &m.usages {
                if u.name.is_empty() || u.kind == "metadata" {
                    continue;
                }
                add(
                    unquote_name(&u.name),
                    u.span.start_byte,
                    usage_metaclass(&u.kind),
                );
                if let Some(sn) = u.short_name.as_deref() {
                    add(
                        unquote_name(sn),
                        u.span.start_byte,
                        usage_metaclass(&u.kind),
                    );
                }
            }
        }
        idx.names = names;
        idx
    }

    /// Named elements containing `pos`, outermost first.
    fn containers(&self, file: &str, pos: usize) -> Vec<&NsEntry> {
        self.by_file
            .get(file)
            .map(|es| {
                es.iter()
                    .filter(|e| e.start <= pos && pos < e.end)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn path_at(&self, file: &str, pos: usize) -> Vec<String> {
        self.containers(file, pos)
            .into_iter()
            .map(|e| e.name.clone())
            .collect()
    }

    /// The scope of something declared at `pos`: the element itself when
    /// the innermost container is `own_name`, otherwise an anonymous
    /// member of the innermost container (a relationship, an annotation).
    fn scope_at(&self, file: &str, pos: usize, own_name: Option<&str>) -> Scope {
        let cs = self.containers(file, pos);
        let own = own_name.map(unquote_name).filter(|n| !n.is_empty());
        let named = match (cs.last(), own) {
            (Some(e), Some(n)) => e.name == n && e.start == pos,
            _ => false,
        };
        Scope {
            path: cs.iter().map(|e| e.name.clone()).collect(),
            named,
            metaclass: if named {
                cs.last().map(|e| e.metaclass.clone()).unwrap_or_default()
            } else {
                String::new()
            },
        }
    }

    /// Resolve a reference (`A::B`, `drone.battery`, `'Flight endurance'`)
    /// to the element's path: the first segment by name, the rest
    /// appended as written.
    fn resolve(&self, reference: &str) -> Option<Scope> {
        let segs = split_path(reference);
        let first = segs.first()?;
        let (base, metaclass) = self.names.get(first)?;
        let mut path = base.clone();
        path.extend(segs[1..].iter().cloned());
        Some(Scope {
            path,
            named: true,
            // A feature chain ends somewhere else; only a bare name
            // keeps the declaration's metaclass.
            metaclass: if segs.len() == 1 {
                metaclass.clone()
            } else {
                String::new()
            },
        })
    }
}

/// `A::B.c` -> ["A", "B", "c"], each segment unquoted.
pub(crate) fn split_path(reference: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = reference.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = !quoted;
                cur.push(c);
            }
            ':' if !quoted && chars.peek() == Some(&':') => {
                chars.next();
                out.push(std::mem::take(&mut cur));
            }
            '.' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter()
        .map(|s| unquote_name(s.trim()).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A qualified name in SysML form: segments that are not plain
/// identifiers are quoted (`SystemRequirements::'Flight endurance'`).
fn display_qualified(path: &[String]) -> String {
    path.iter()
        .map(|s| {
            let plain = s
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_alphanumeric() || c == '_');
            if plain {
                s.clone()
            } else {
                format!("'{s}'")
            }
        })
        .collect::<Vec<_>>()
        .join("::")
}

/// `part` -> `PartUsage`, `use case` -> `UseCaseUsage`.
fn usage_metaclass(kind: &str) -> String {
    format!("{}Usage", camel(kind))
}

/// `part def` -> `PartDefinition`, `package` -> `Package`.
fn def_metaclass(kind: DefKind) -> String {
    match kind.label().strip_suffix(" def") {
        Some(base) => format!("{}Definition", camel(base)),
        None => camel(kind.label()),
    }
}

fn camel(words: &str) -> String {
    words
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

// --- rows -----------------------------------------------------------------

/// Where a row sits in the model, for `expose` and `filter`.
#[derive(Debug, Clone)]
struct Scope {
    /// Owning-namespace chain; ends with the element when `named`.
    path: Vec<String>,
    /// True when the row *is* the element at the end of `path`; false
    /// when it is an anonymous member of `path` (a relationship).
    named: bool,
    /// SysML metaclass of the named element (`PartUsage`), if known.
    metaclass: String,
}

/// A row: ordered (field, value) pairs plus the places in the model it
/// belongs to (any of which may be exposed).
#[derive(Debug, Clone, Default)]
struct Row {
    fields: Vec<(String, String)>,
    scopes: Vec<Scope>,
}

impl Row {
    fn new(fields: Vec<(String, String)>) -> Self {
        Row {
            fields,
            scopes: Vec::new(),
        }
    }

    fn scoped(mut self, scope: Scope) -> Self {
        if !scope.path.is_empty() {
            self.scopes.push(scope);
        }
        self
    }

    fn scoped_opt(self, scope: Option<Scope>) -> Self {
        match scope {
            Some(s) => self.scoped(s),
            None => self,
        }
    }

    fn set(&mut self, name: &str, value: String) {
        match self.fields.iter_mut().find(|(k, _)| k == name) {
            Some(slot) => slot.1 = value,
            None => self.fields.push((name.to_string(), value)),
        }
    }

    fn has(&self, name: &str) -> bool {
        self.fields.iter().any(|(k, _)| k == name)
    }

    fn source(&self) -> RowSource {
        RowSource {
            element: self
                .scopes
                .iter()
                .find(|s| s.named)
                .map(|s| display_qualified(&s.path))
                .unwrap_or_default(),
            file: field(self, "file").unwrap_or_default(),
            line: field(self, "line")
                .and_then(|l| l.parse().ok())
                .unwrap_or(0),
        }
    }
}

fn field(row: &Row, name: &str) -> Option<String> {
    row.fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// Spans are already 1-based.
fn line_of(start_row: usize) -> String {
    start_row.to_string()
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
    idx: &Index,
    models: &[Model],
    targets: &[String],
    spec: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Row>, String> {
    // Rows come from the target files; `models` stays available for
    // resolution that must see the whole context (specialization
    // chains, imported types, trace links written elsewhere).
    let content = content_models(models, targets);
    let content = content.as_slice();
    let models: &[Model] = if models.is_empty() { content } else { models };
    if let Some(meta) = spec.strip_prefix('@') {
        return Ok(metadata_rows(idx, content, meta));
    }
    if let Some(ty) = spec.strip_prefix("type:") {
        return Ok(typed_usage_rows(idx, content, models, ty.trim()));
    }
    if let Some(kind) = spec.strip_prefix("kind:") {
        return Ok(kind_rows(idx, content, models, kind.trim()));
    }
    if let Some(rel) = spec.strip_prefix("relation:") {
        return relation_rows(idx, content, models, rel.trim());
    }
    if let Some(root) = spec.strip_prefix("composition:") {
        return Ok(composition_rows(idx, content, models, Some(root.trim())));
    }
    match spec {
        "composition" => Ok(composition_rows(idx, content, models, None)),
        "requirements" => Ok(requirement_rows(idx, content, models)),
        "trace" => Ok(trace_rows(idx, content, models)),
        "kindcounts" => Ok(kindcount_rows(content)),
        "uncertainty" => Ok(uncertainty_rows(idx, content, models, warnings)),
        other => Err(format!(
            "unknown row provider `{other}` (expected @Metadata, type:, kind:, \
             relation:, composition, requirements, trace, kindcounts, or uncertainty)"
        )),
    }
}

/// One row per annotation of a metadata type. The row belongs to the
/// annotated element and to the namespace the annotation is written in.
fn metadata_rows(idx: &Index, models: &[Model], meta_type: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for m in models {
        for a in &m.annotations {
            if simple_name(&a.metadata_type) != meta_type {
                continue;
            }
            let mut row = Row::new(vec![
                ("element".into(), a.target.clone().unwrap_or_default()),
                ("file".into(), m.file.clone()),
                ("line".into(), line_of(a.span.start_row)),
            ]);
            for (k, v) in &a.values {
                row.fields.push((k.clone(), unquote(v).to_string()));
            }
            let owner = idx.scope_at(&m.file, a.span.start_byte, None);
            let target = a.target.as_deref().unwrap_or("");
            // `@X { }` inside an element annotates that element; `@X about
            // a.b { }` annotates something elsewhere.
            let annotates_owner =
                owner.path.last().map(String::as_str) == Some(unquote_name(target));
            if annotates_owner {
                let cs = idx.containers(&m.file, a.span.start_byte);
                rows.push(row.scoped(Scope {
                    path: owner.path,
                    named: true,
                    metaclass: cs.last().map(|e| e.metaclass.clone()).unwrap_or_default(),
                }));
            } else {
                rows.push(
                    row.scoped_opt(idx.resolve(target).filter(|_| !target.is_empty()))
                        .scoped(owner),
                );
            }
        }
    }
    rows
}

/// Usages typed by `ty` or any definition specializing it.
fn typed_usage_rows(idx: &Index, content: &[Model], models: &[Model], ty: &str) -> Vec<Row> {
    // Rows come from the target files; `models` resolves the
    // specialization chain across the whole context.
    let mut rows = Vec::new();
    for m in content {
        for u in &m.usages {
            let Some(ref t) = u.type_ref else { continue };
            if u.name.is_empty() || !specializes_type(models, t, ty) {
                continue;
            }
            let mut row = Row::new(vec![
                ("element".into(), u.name.clone()),
                ("type".into(), simple_name(t).to_string()),
                ("kind".into(), u.kind.clone()),
                ("parent".into(), u.parent_def.clone().unwrap_or_default()),
                ("file".into(), m.file.clone()),
                ("line".into(), line_of(u.span.start_row)),
            ]);
            // Body values (`:>> description = "..."`) become fields.
            for c in body_children(m, u) {
                if let Some(ref v) = c.value_expr {
                    row.fields.push((c.name.clone(), unquote(v).to_string()));
                }
            }
            rows.push(row.scoped(idx.scope_at(&m.file, u.span.start_byte, Some(&u.name))));
        }
    }
    rows
}

/// Usages of one syntactic kind (`kind:port`). Besides the declaration
/// (owner, type, direction) each row says what the element is connected
/// to — the interface table of a port list.
///
/// `direction` is the declared one or, for a port, the one its port def
/// implies: the directions of its features, flipped when the port is
/// conjugated (`~PowerPort`), summarised as in, out or inout.
fn kind_rows(idx: &Index, content: &[Model], models: &[Model], kind: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for m in content {
        for u in &m.usages {
            if u.kind != kind || u.name.is_empty() {
                continue;
            }
            let owner = u.parent_def.clone().unwrap_or_default();
            let ty = u.type_ref.clone().unwrap_or_default();
            let shown_type = if u.is_conjugated && !ty.is_empty() {
                format!("~{ty}")
            } else {
                ty
            };
            let (ends, vias) = connected_to(idx, models, u, &owner);
            let row = Row::new(vec![
                ("element".into(), unquote_name(&u.name).to_string()),
                ("owner".into(), unquote_name(&owner).to_string()),
                ("parent".into(), owner.clone()),
                ("type".into(), shown_type),
                ("direction".into(), usage_direction(models, u)),
                ("conjugated".into(), bit(u.is_conjugated)),
                ("connectedTo".into(), ends.join(", ")),
                ("via".into(), vias.join(", ")),
                (
                    "doc".into(),
                    u.doc.as_deref().map(collapse_ws).unwrap_or_default(),
                ),
                ("file".into(), m.file.clone()),
                ("line".into(), line_of(u.span.start_row)),
            ]);
            rows.push(row.scoped(idx.scope_at(&m.file, u.span.start_byte, Some(&u.name))));
        }
    }
    rows
}

/// Declared direction, else the direction implied by the usage's type.
fn usage_direction(models: &[Model], u: &Usage) -> String {
    if let Some(d) = u.direction {
        return d.label().to_string();
    }
    let Some(t) = u.type_ref.as_deref() else {
        return String::new();
    };
    let (mut has_in, mut has_out) = (false, false);
    let mut current = simple_name(t).to_string();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        for m in models {
            for f in m.usages_in_def(&current) {
                match f.direction.map(|d| d.label()) {
                    Some("in") => has_in = true,
                    Some("out") => has_out = true,
                    Some(_) => (has_in, has_out) = (true, true),
                    None => {}
                }
            }
        }
        match models
            .iter()
            .find_map(|m| m.find_def(&current))
            .and_then(|d| d.super_type.as_deref())
        {
            Some(s) => current = simple_name(s).to_string(),
            None => break,
        }
    }
    if u.is_conjugated {
        std::mem::swap(&mut has_in, &mut has_out);
    }
    match (has_in, has_out) {
        (true, true) => "inout",
        (true, false) => "in",
        (false, true) => "out",
        (false, false) => "",
    }
    .to_string()
}

/// The far ends (as written) and the connection names of every
/// connection that ends on `u`. An end `x.p` reaches port `p` of owner
/// `D` when `x` is typed by `D` (or a specialization) in the context the
/// connection is written in; a bare end `p` when the connection is
/// written inside `D` itself.
fn connected_to(
    idx: &Index,
    models: &[Model],
    u: &Usage,
    owner: &str,
) -> (Vec<String>, Vec<String>) {
    let name = unquote_name(&u.name);
    let owner = unquote_name(owner);
    let mut ends: Vec<String> = Vec::new();
    let mut vias: Vec<String> = Vec::new();
    for m in models {
        for c in &m.connections {
            let mut context = idx.path_at(&m.file, c.span.start_byte);
            if let (Some(last), Some(cn)) = (context.last(), c.name.as_deref()) {
                if last == unquote_name(cn) {
                    context.pop();
                }
            }
            let context = context.last().map(String::as_str).unwrap_or("");
            for (end, other) in [(&c.source, &c.target), (&c.target, &c.source)] {
                if endpoint_reaches(models, end, name, owner, context) {
                    if !ends.contains(other) {
                        ends.push(other.clone());
                    }
                    let via = match (c.name.as_deref(), c.type_ref.as_deref()) {
                        (Some(n), _) => unquote_name(n).to_string(),
                        (None, Some(t)) => simple_name(t).to_string(),
                        (None, None) => String::new(),
                    };
                    if !via.is_empty() && !vias.contains(&via) {
                        vias.push(via);
                    }
                }
            }
        }
    }
    (ends, vias)
}

fn endpoint_reaches(models: &[Model], end: &str, name: &str, owner: &str, context: &str) -> bool {
    let segs = split_path(end);
    if segs.last().map(String::as_str) != Some(name) {
        return false;
    }
    if segs.len() == 1 {
        return !owner.is_empty() && context == owner;
    }
    let via = &segs[segs.len() - 2];
    if via == owner {
        return true;
    }
    // The usage the chain steps through: preferably one declared in the
    // connection's own context.
    let candidates: Vec<&Usage> = models
        .iter()
        .flat_map(|m| &m.usages)
        .filter(|x| unquote_name(&x.name) == via)
        .collect();
    let in_context = candidates
        .iter()
        .filter(|x| x.parent_def.as_deref().map(unquote_name) == Some(context))
        .collect::<Vec<_>>();
    let pool: Vec<&&Usage> = if in_context.is_empty() {
        candidates.iter().collect()
    } else {
        in_context
    };
    pool.iter()
        .any(|x| effective_type(models, x).is_some_and(|t| specializes_type(models, &t, owner)))
}

/// True when `type_name` is, or transitively specializes, `target`.
/// Resolution walks `models` (the whole include path), so a project type
/// declared against a library supertype still matches.
fn specializes_type(models: &[Model], type_name: &str, target: &str) -> bool {
    let target = unquote_name(simple_name(target));
    let mut current = unquote_name(simple_name(type_name)).to_string();
    let mut seen = HashSet::new();
    loop {
        if current == target {
            return true;
        }
        if !seen.insert(current.clone()) {
            return false;
        }
        let Some(def) = models.iter().find_map(|m| {
            m.definitions
                .iter()
                .find(|d| unquote_name(&d.name) == current)
        }) else {
            return false;
        };
        match def.super_type.as_deref() {
            Some(s) => current = unquote_name(simple_name(s)).to_string(),
            None => return false,
        }
    }
}

fn relation_rows(
    idx: &Index,
    content: &[Model],
    models: &[Model],
    rel: &str,
) -> Result<Vec<Row>, String> {
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
    match rel {
        "allocation" => {
            for m in content {
                for a in &m.allocations {
                    let row = Row::new(vec![
                        ("source".into(), a.source.clone()),
                        ("target".into(), a.target.clone()),
                        ("file".into(), m.file.clone()),
                        ("line".into(), line_of(a.span.start_row)),
                    ]);
                    rows.push(
                        row.scoped(idx.scope_at(&m.file, a.span.start_byte, None))
                            .scoped_opt(idx.resolve(&a.source)),
                    );
                }
            }
        }
        // Trace relationships belong to the namespace they are written in
        // and to their requirement, so a view exposing a requirements
        // package sees the links to its requirements wherever they are.
        "satisfy" | "verify" => {
            let links: Vec<(&Model, &str, String, &crate::model::Span)> = content
                .iter()
                .flat_map(|m| {
                    let sat = m.satisfactions.iter().map(move |s| {
                        (
                            m,
                            s.requirement.as_str(),
                            s.by.clone().unwrap_or_else(|| {
                                idx.path_at(&m.file, s.span.start_byte)
                                    .last()
                                    .cloned()
                                    .unwrap_or_default()
                            }),
                            &s.span,
                        )
                    });
                    let ver = m
                        .verifications
                        .iter()
                        .map(move |v| (m, v.requirement.as_str(), v.by.clone(), &v.span));
                    let (s, v): (Vec<_>, Vec<_>) = (sat.collect(), ver.collect());
                    if rel == "satisfy" {
                        s
                    } else {
                        v
                    }
                })
                .collect();
            for (m, requirement, by, span) in links {
                let (title, id) = requirement_names(models, requirement);
                let label = if id.is_empty() {
                    title.clone()
                } else {
                    format!("{id} {title}")
                };
                let row = Row::new(vec![
                    ("requirement".into(), label),
                    ("id".into(), id),
                    ("title".into(), title),
                    ("target".into(), requirement.to_string()),
                    ("by".into(), by),
                    ("file".into(), m.file.clone()),
                    ("line".into(), line_of(span.start_row)),
                ]);
                rows.push(
                    row.scoped(idx.scope_at(&m.file, span.start_byte, None))
                        .scoped_opt(idx.resolve(requirement)),
                );
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
                    let row = Row::new(vec![
                        ("connection".into(), c.name.clone().unwrap_or_default()),
                        (
                            "type".into(),
                            c.type_ref.as_deref().map(simple_name).unwrap_or("").into(),
                        ),
                        ("source".into(), c.source.clone()),
                        ("target".into(), c.target.clone()),
                        ("file".into(), m.file.clone()),
                        ("line".into(), line_of(c.span.start_row)),
                    ]);
                    rows.push(row.scoped(idx.scope_at(
                        &m.file,
                        c.span.start_byte,
                        c.name.as_deref(),
                    )));
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
                    let row = Row::new(vec![
                        ("succession".into(), u.name.clone()),
                        (
                            "type".into(),
                            u.type_ref.as_deref().map(simple_name).unwrap_or("").into(),
                        ),
                        ("source".into(), src.to_string()),
                        ("target".into(), tgt.to_string()),
                        ("parent".into(), u.parent_def.clone().unwrap_or_default()),
                        ("file".into(), m.file.clone()),
                        ("line".into(), line_of(u.span.start_row)),
                    ]);
                    rows.push(row.scoped(idx.scope_at(&m.file, u.span.start_byte, Some(&u.name))));
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

/// Title and `<id>` of the requirement a trace target refers to (the
/// target itself, unquoted, when it resolves to nothing).
fn requirement_names(models: &[Model], target: &str) -> (String, String) {
    use crate::model::target_matches;
    let found = models.iter().find_map(|m| {
        m.usages
            .iter()
            .filter(|u| u.kind == "requirement" && !u.name.is_empty())
            .map(|u| (u.name.as_str(), u.short_name.as_deref()))
            .chain(
                m.definitions
                    .iter()
                    .filter(|d| d.kind == DefKind::Requirement)
                    .map(|d| (d.name.as_str(), d.short_name.as_deref())),
            )
            .find(|(n, sn)| target_matches(target, n, *sn))
    });
    match found {
        Some((n, sn)) => (
            unquote_name(n).to_string(),
            sn.map(|s| unquote_name(s).to_string()).unwrap_or_default(),
        ),
        None => (
            split_path(target).last().cloned().unwrap_or_default(),
            String::new(),
        ),
    }
}

/// One row per requirement: every named `requirement` usage, and every
/// `requirement def` that is not merely the type of such usages (a
/// classifier like `DroneRequirement` is not itself a requirement to
/// satisfy).
///
/// Fields: element, qualifiedName, id (short name), title (name), text
/// (doc), type, every attribute value set in the body (`:>> kind =
/// RequirementKind::safety` gives kind = safety; lists are comma
/// separated), status and owner (`@StatusInfo`), rationale
/// (`@Rationale`, else a `rationale` attribute), issues (`@Issue`),
/// satisfiedBy / verifiedBy (comma lists, from every file), satisfied /
/// verified (1 or 0, so `where` can select gaps), file, line.
fn requirement_rows(idx: &Index, content: &[Model], models: &[Model]) -> Vec<Row> {
    use crate::model::target_matches;

    let (satisfied_set, verified_set) = crate::resolver::traced_requirement_defs(models.iter());
    let links = |name: &str, short: Option<&str>, verify: bool| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in models {
            let found: Vec<String> = if verify {
                m.verifications
                    .iter()
                    .filter(|v| target_matches(&v.requirement, name, short))
                    .map(|v| v.by.clone())
                    .collect()
            } else {
                m.satisfactions
                    .iter()
                    .filter(|s| target_matches(&s.requirement, name, short))
                    .map(|s| {
                        s.by.clone().unwrap_or_else(|| {
                            idx.path_at(&m.file, s.span.start_byte)
                                .last()
                                .cloned()
                                .unwrap_or_default()
                        })
                    })
                    .collect()
            };
            for f in found {
                if !f.is_empty() && !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        out
    };

    // Requirement defs that only classify requirement usages: the types
    // of usages and their supertypes.
    let mut classifiers: HashSet<String> = HashSet::new();
    for m in models {
        for u in &m.usages {
            if u.kind == "requirement" {
                if let Some(t) = u.type_ref.as_deref() {
                    let mut current = unquote_name(simple_name(t)).to_string();
                    while classifiers.insert(current.clone()) {
                        match models
                            .iter()
                            .find_map(|m| m.find_def(&current))
                            .and_then(|d| d.super_type.as_deref())
                        {
                            Some(s) => current = unquote_name(simple_name(s)).to_string(),
                            None => break,
                        }
                    }
                }
            }
        }
    }

    let mut rows = Vec::new();
    for m in content {
        let mut candidates: Vec<(usize, Row)> = Vec::new();
        for u in &m.usages {
            if u.kind != "requirement" || u.name.is_empty() {
                continue;
            }
            let short = u.short_name.as_deref();
            let sat = links(&u.name, short, false);
            let ver = links(&u.name, short, true);
            let traced = |set: &HashSet<String>| {
                set.contains(unquote_name(&u.name))
                    || short.is_some_and(|s| set.contains(unquote_name(s)))
            };
            let satisfied = !sat.is_empty() || traced(&satisfied_set);
            let verified = !ver.is_empty() || traced(&verified_set);
            let mut row = requirement_row(
                idx,
                models,
                m,
                &u.name,
                short,
                u.doc.as_deref(),
                u.type_ref.as_deref(),
                (u.span.start_byte, u.span.start_row),
                [sat, ver],
                [satisfied, verified],
            );
            for c in body_children(m, u) {
                if let (Some(v), false) = (c.value_expr.as_deref(), c.name.is_empty()) {
                    let key = unquote_name(&c.name);
                    // Body values fill fields the metadata left empty
                    // (a `rationale` attribute when there is no
                    // @Rationale) and add the rest (kind, priority, ...).
                    if field(&row, key).is_none_or(|f| f.is_empty()) {
                        row.set(key, display_value(v));
                    }
                }
            }
            candidates.push((u.span.start_byte, row));
        }
        for d in &m.definitions {
            if d.kind != DefKind::Requirement || classifiers.contains(unquote_name(&d.name)) {
                continue;
            }
            let short = d.short_name.as_deref();
            let sat = links(&d.name, short, false);
            let ver = links(&d.name, short, true);
            let satisfied = !sat.is_empty() || satisfied_set.contains(&d.name);
            let verified = !ver.is_empty() || verified_set.contains(&d.name);
            let mut row = requirement_row(
                idx,
                models,
                m,
                &d.name,
                short,
                d.doc.as_deref(),
                d.super_type.as_deref(),
                (d.span.start_byte, d.span.start_row),
                [sat, ver],
                [satisfied, verified],
            );
            for c in m.usages_in_def(&d.name) {
                if let (Some(v), false) = (c.value_expr.as_deref(), c.name.is_empty()) {
                    let key = unquote_name(&c.name);
                    if !row.has(key) {
                        row.set(key, display_value(v));
                    }
                }
            }
            candidates.push((d.span.start_byte, row));
        }
        // Declaration order within a file.
        candidates.sort_by_key(|(start, _)| *start);
        rows.extend(candidates.into_iter().map(|(_, r)| r));
    }
    // Columns the standard views ask for exist on every row.
    for row in &mut rows {
        for key in ["kind", "priority", "verificationMethod"] {
            if !row.has(key) {
                row.set(key, String::new());
            }
        }
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn requirement_row(
    idx: &Index,
    models: &[Model],
    m: &Model,
    name: &str,
    short: Option<&str>,
    doc: Option<&str>,
    ty: Option<&str>,
    (start_byte, start_row): (usize, usize),
    [sat, ver]: [Vec<String>; 2],
    [satisfied, verified]: [bool; 2],
) -> Row {
    let title = unquote_name(name).to_string();
    // Standard ModelingMetadata annotations on the requirement.
    let annotations: Vec<&crate::model::MetadataAnnotation> = models
        .iter()
        .flat_map(|md| &md.annotations)
        .filter(|a| {
            a.target
                .as_deref()
                .is_some_and(|t| unquote_name(simple_name(t)) == title)
        })
        .collect();
    let meta = |ty: &str, key: &str| -> String {
        annotations
            .iter()
            .filter(|a| simple_name(&a.metadata_type) == ty)
            .find_map(|a| a.values.iter().find(|(k, _)| k == key))
            .map(|(_, v)| display_value(v))
            .unwrap_or_default()
    };
    let issues: Vec<String> = annotations
        .iter()
        .filter(|a| simple_name(&a.metadata_type) == "Issue")
        .filter_map(|a| a.values.iter().find(|(k, _)| k == "text"))
        .map(|(_, v)| display_value(v))
        .collect();
    let scope = idx.scope_at(&m.file, start_byte, Some(name));
    Row::new(vec![
        ("element".into(), title.clone()),
        ("qualifiedName".into(), display_qualified(&scope.path)),
        (
            "id".into(),
            short
                .map(|s| unquote_name(s).to_string())
                .unwrap_or_default(),
        ),
        ("title".into(), title),
        ("text".into(), doc.map(collapse_ws).unwrap_or_default()),
        (
            "type".into(),
            ty.map(|t| simple_name(t).to_string()).unwrap_or_default(),
        ),
        ("status".into(), meta("StatusInfo", "status")),
        ("owner".into(), meta("StatusInfo", "owner")),
        ("rationale".into(), meta("Rationale", "text")),
        ("issues".into(), issues.join("; ")),
        ("satisfiedBy".into(), sat.join(", ")),
        ("verifiedBy".into(), ver.join(", ")),
        ("satisfied".into(), bit(satisfied)),
        ("verified".into(), bit(verified)),
        ("file".into(), m.file.clone()),
        ("line".into(), line_of(start_row)),
    ])
    .scoped(scope)
}

/// Requirement coverage: the `requirements` rows with `satisfied` and
/// `verified` spelled yes/no and the title as `requirement`. Uses the
/// same project-wide resolution as the W002/W003 checks — satisfy targets
/// resolve through usages and close over specialization, so satisfying
/// `Derived :> Base` satisfies `Base` here too.
fn trace_rows(idx: &Index, content: &[Model], models: &[Model]) -> Vec<Row> {
    requirement_rows(idx, content, models)
        .into_iter()
        .map(|mut r| {
            let title = field(&r, "title").unwrap_or_default();
            r.fields.insert(0, ("requirement".into(), title));
            for key in ["satisfied", "verified"] {
                let yes = field(&r, key).as_deref() == Some("1");
                r.set(key, if yes { "yes" } else { "no" }.into());
            }
            r
        })
        .collect()
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
            Row::new(vec![
                ("kind".into(), k.clone()),
                (
                    "definitions".into(),
                    defs.get(&k).copied().unwrap_or(0).to_string(),
                ),
                (
                    "usages".into(),
                    usages.get(&k).copied().unwrap_or(0).to_string(),
                ),
            ])
        })
        .collect()
}

/// One row per uncertainty analysis case, evaluated (worst-case + RSS).
fn uncertainty_rows(
    idx: &Index,
    content: &[Model],
    models: &[Model],
    warnings: &mut Vec<String>,
) -> Vec<Row> {
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
                let row = Row::new(vec![
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
                rows.push(row.scoped_opt(idx.resolve(&name)));
            }
            Err(e) => warnings.push(format!("case `{name}`: {e}")),
        }
    }
    rows
}

// --- composition (BOM) ----------------------------------------------------------

/// A node of the composition tree: a definition, or a usage (whose body
/// may add parts its type does not have — `part drone : DeliveryDrone {
/// part battery : Battery; }`).
#[derive(Clone, Copy)]
enum CompNode<'a> {
    Def(&'a str),
    Use(&'a Model, &'a Usage),
}

/// The composition tree under `root` (a definition or a usage), or under
/// every top-level element when no root is named: one row per part or
/// item usage, a bill of materials. Quantity comes from the usage's
/// multiplicity and `extended` multiplies it down the tree, so a part
/// used 2x inside an assembly used 3x reports 6.
///
/// Only `part` and `item` usages are walked. Connections and flows are
/// structure, not content — including them is what made a "BOM" list
/// connectors alongside parts.
///
/// Structure only: which attributes matter (mass, cost, supplier) is the
/// model's business, so a view names them as columns and they are read
/// off the usage's body and then its type.
fn composition_rows(
    idx: &Index,
    content: &[Model],
    models: &[Model],
    root: Option<&str>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut walker = Walker {
        idx,
        models,
        rows: &mut rows,
    };
    match root {
        Some(root) => {
            let simple = split_path(root).last().cloned().unwrap_or_default();
            let def = models.iter().find_map(|m| {
                m.definitions
                    .iter()
                    .find(|d| unquote_name(&d.name) == simple)
                    .map(|d| (m, d))
            });
            if let Some((m, d)) = def {
                let path = walker.idx.path_at(&m.file, d.span.start_byte);
                walker.walk(CompNode::Def(&d.name), &path, "", 0, 1, &mut Vec::new());
            } else if let Some((m, u)) = models.iter().find_map(|m| {
                m.usages
                    .iter()
                    .find(|u| {
                        matches!(u.kind.as_str(), "part" | "item")
                            && unquote_name(&u.name) == simple
                    })
                    .map(|u| (m, u))
            }) {
                let path = walker.idx.path_at(&m.file, u.span.start_byte);
                walker.walk(CompNode::Use(m, u), &path, "", 0, 1, &mut Vec::new());
            }
        }
        None => {
            // No root named: walk from every package, part def and item def
            // that nothing uses as a type, i.e. the tops of the composition
            // forest. A package's top-level parts (`part drone : ...`) are
            // roots of their own trees.
            let used: HashSet<&str> = models
                .iter()
                .flat_map(|m| m.usages.iter())
                .filter_map(|u| u.type_ref.as_deref().map(simple_name))
                .collect();
            for m in content {
                for d in &m.definitions {
                    if !matches!(d.kind, DefKind::Package | DefKind::Part | DefKind::Item)
                        || used.contains(d.name.as_str())
                    {
                        continue;
                    }
                    let path = walker.idx.path_at(&m.file, d.span.start_byte);
                    walker.walk(CompNode::Def(&d.name), &path, "", 0, 1, &mut Vec::new());
                }
            }
        }
    }
    rows
}

struct Walker<'a, 'r> {
    idx: &'a Index,
    models: &'a [Model],
    rows: &'r mut Vec<Row>,
}

impl<'a> Walker<'a, '_> {
    /// Part and item usages directly inside `node`.
    fn children(&self, node: CompNode<'a>) -> Vec<(&'a Model, &'a Usage)> {
        let is_content =
            |u: &Usage| matches!(u.kind.as_str(), "part" | "item") && !u.name.is_empty();
        match node {
            CompNode::Def(name) => self
                .models
                .iter()
                .flat_map(|m| {
                    m.usages_in_def(name)
                        .into_iter()
                        .filter(|u| is_content(u))
                        .map(move |u| (m, u))
                })
                .collect(),
            CompNode::Use(m, u) => {
                let mut out: Vec<(&Model, &Usage)> = body_children(m, u)
                    .into_iter()
                    .filter(|c| is_content(c))
                    .map(|c| (m, c))
                    .collect();
                // A usage subsetting another usage has that usage's parts.
                if let Some((rm, ru)) = referenced_usage(self.models, u) {
                    let own: HashSet<&str> = out.iter().map(|(_, c)| c.name.as_str()).collect();
                    let inherited: Vec<_> = self
                        .children(CompNode::Use(rm, ru))
                        .into_iter()
                        .filter(|(_, c)| !own.contains(c.name.as_str()))
                        .collect();
                    out.extend(inherited);
                } else if let Some(t) = u.type_ref.as_deref() {
                    let ty = simple_name(t);
                    if let Some(def) = self.models.iter().find_map(|md| md.find_def(ty)) {
                        // A body part redefining a type part replaces it.
                        let own: HashSet<&str> = out.iter().map(|(_, c)| c.name.as_str()).collect();
                        let inherited: Vec<_> = self
                            .children(CompNode::Def(&def.name))
                            .into_iter()
                            .filter(|(_, c)| !own.contains(c.name.as_str()))
                            .collect();
                        out.extend(inherited);
                    }
                }
                out
            }
        }
    }

    fn walk(
        &mut self,
        node: CompNode<'a>,
        path: &[String],
        dotted: &str,
        depth: usize,
        mult: u32,
        ancestors: &mut Vec<String>,
    ) {
        use crate::sim::resolve::quantity_from_multiplicity;

        // Cycle guard: a type that contains itself would recurse forever.
        let key = match node {
            CompNode::Def(n) => format!("def:{n}"),
            CompNode::Use(m, u) => format!("use:{}:{}", m.file, u.span.start_byte),
        };
        if ancestors.contains(&key) || depth > 64 {
            return;
        }
        ancestors.push(key);
        let parent = match node {
            CompNode::Def(n) => unquote_name(n).to_string(),
            CompNode::Use(_, u) => unquote_name(&u.name).to_string(),
        };
        for (m, u) in self.children(node) {
            let name = unquote_name(&u.name).to_string();
            let ty = u.type_ref.as_deref().map(simple_name).unwrap_or("");
            let qty = quantity_from_multiplicity(u);
            let extended = mult.saturating_mul(qty);
            let child_dotted = if dotted.is_empty() {
                name.clone()
            } else {
                format!("{dotted}.{name}")
            };
            let mut child_path = path.to_vec();
            child_path.push(name.clone());
            let type_def = (!ty.is_empty())
                .then(|| self.models.iter().find_map(|md| md.find_def(ty)))
                .flatten();
            let doc = u
                .doc
                .as_deref()
                .or_else(|| referenced_usage(self.models, u).and_then(|(_, r)| r.doc.as_deref()))
                .or_else(|| type_def.and_then(|d| d.doc.as_deref()))
                .map(collapse_ws)
                .unwrap_or_default();
            let mut row = Row::new(vec![
                ("element".into(), name),
                ("type".into(), ty.to_string()),
                ("parent".into(), parent.clone()),
                ("path".into(), child_dotted.clone()),
                ("depth".into(), depth.to_string()),
                ("quantity".into(), qty.to_string()),
                ("extended".into(), extended.to_string()),
                ("doc".into(), doc),
                ("file".into(), m.file.clone()),
                ("line".into(), line_of(u.span.start_row)),
            ]);
            // Attribute values: the usage's own (`:>> mass = 2 [kg]`),
            // then its type's, so a view can ask for `mass` or `cost` as a
            // column without this code knowing which attributes exist.
            for a in body_children(m, u) {
                if a.kind == "attribute" && !a.name.is_empty() {
                    if let Some(v) = a.value_expr.as_deref() {
                        if !row.has(&a.name) {
                            row.fields
                                .push((a.name.clone(), unquote(v.trim()).to_string()));
                        }
                    }
                }
            }
            if !ty.is_empty() {
                for am in self.models {
                    for a in am.usages_in_def(ty) {
                        if a.kind == "attribute" && !a.name.is_empty() {
                            if let Some(v) = a.value_expr.as_deref() {
                                if !row.has(&a.name) {
                                    row.fields
                                        .push((a.name.clone(), unquote(v.trim()).to_string()));
                                }
                            }
                        }
                    }
                }
            }
            self.rows.push(row.scoped(Scope {
                path: child_path.clone(),
                named: true,
                metaclass: usage_metaclass(&u.kind),
            }));
            self.walk(
                CompNode::Use(m, u),
                &child_path,
                &child_dotted,
                depth + 1,
                extended,
                ancestors,
            );
        }
        ancestors.pop();
    }
}

// --- pivot ------------------------------------------------------------------

fn pivot_table(
    view: &str,
    rows: &[Row],
    row_field: &str,
    col_field: &str,
    sort: Option<String>,
) -> RenderedTable {
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
    // Numeric-aware ordering, descending rows by default (severity 10 on
    // top); `sortBy` naming the row field orders rows that way instead.
    match sort.as_deref().map(str::trim) {
        Some(s) if s == row_field => row_keys.sort_by(|a, b| cmp_cells(a, b)),
        Some(s) if s.strip_prefix('-').map(str::trim) == Some(row_field) => {
            row_keys.sort_by(|a, b| cmp_cells(b, a))
        }
        _ => row_keys.sort_by(|a, b| cmp_cells(b, a)),
    }
    col_keys.sort_by(|a, b| cmp_cells(a, b));

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
        ..Default::default()
    }
}

// --- helpers ----------------------------------------------------------------

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

fn bit(b: bool) -> String {
    if b { "1" } else { "0" }.to_string()
}

/// Doc text as one line: comment line breaks — and the `*` that starts
/// each continuation line of a block comment — are layout, not content.
fn collapse_ws(s: &str) -> String {
    s.lines()
        .map(|l| {
            let l = l.trim();
            match l.strip_prefix('*') {
                Some(rest) if !rest.starts_with('*') => rest,
                _ => l,
            }
        })
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The definition a usage's features come from: its type, or — for a
/// usage that subsets or references another usage (`part drone :>
/// Architecture::drone`) — that usage's type.
fn effective_type(models: &[Model], u: &Usage) -> Option<String> {
    let mut current = u.type_ref.as_deref().or(u.subsets.as_deref())?;
    for _ in 0..8 {
        let name = unquote_name(simple_name(current));
        if models
            .iter()
            .any(|m| m.definitions.iter().any(|d| unquote_name(&d.name) == name))
        {
            return Some(name.to_string());
        }
        let next = models
            .iter()
            .flat_map(|m| &m.usages)
            .filter(|x| !std::ptr::eq(*x, u) && unquote_name(&x.name) == name)
            .find_map(|x| x.type_ref.as_deref().or(x.subsets.as_deref()))?;
        current = next;
    }
    None
}

/// The usage a usage subsets or references when that is not a definition
/// (`part drone :> Architecture::drone` refers to the usage `drone`).
fn referenced_usage<'a>(models: &'a [Model], u: &Usage) -> Option<(&'a Model, &'a Usage)> {
    let target = u.type_ref.as_deref().or(u.subsets.as_deref())?;
    let name = unquote_name(simple_name(target));
    if models.iter().any(|m| m.find_def(name).is_some()) {
        return None;
    }
    models.iter().find_map(|m| {
        m.usages
            .iter()
            .find(|x| {
                !std::ptr::eq(*x, u)
                    && matches!(x.kind.as_str(), "part" | "item")
                    && unquote_name(&x.name) == name
            })
            .map(|x| (m, x))
    })
}

/// A value as a person reads it: strings unquoted, enum literals and
/// references by their simple name, lists comma-separated —
/// `(VerificationMethod::test, VerificationMethod::analysis)` reads
/// `test, analysis`.
fn display_value(v: &str) -> String {
    let v = v.trim();
    if v.starts_with('"') {
        return unescape(unquote(v));
    }
    if let Some(inner) = v.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<String> = inner.split(',').map(display_value).collect();
        if parts.iter().all(|p| !p.is_empty()) {
            return parts.join(", ");
        }
    }
    // A name path (`Kind::high`, `Kind::'use'`) reads as its last name.
    let is_segment = |s: &str| {
        let plain = s
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_');
        let quoted = s
            .strip_prefix('\'')
            .and_then(|t| t.strip_suffix('\''))
            .is_some_and(|t| !t.is_empty() && !t.contains('\''));
        plain || quoted
    };
    let segs: Vec<&str> = v.split("::").collect();
    if segs.iter().all(|s| is_segment(s)) {
        return unquote_name(segs[segs.len() - 1]).to_string();
    }
    v.to_string()
}

fn row_env(row: &Row) -> crate::sim::expr::Env {
    use crate::sim::expr::{Env, Value};
    let mut env = Env::new();
    for (k, v) in &row.fields {
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
    fn columns_say_how_they_are_edited() {
        let src = r#"
            package P {
                metadata def Fmea {
                    attribute failureMode : String;
                    attribute severity : Integer;
                    ref target : Part[0..1];
                }
                metadata def TableRendering;
                occurrence def Hazard { attribute description : String[0..1]; }
                occurrence fire : Hazard { :>> description = "Fire"; }
                view def Log {
                    @TableRendering {
                        rows = "type:Hazard";
                        columns = "element; description; type; line";
                    }
                }
                view def Sheet {
                    @TableRendering {
                        rows = "@Fmea";
                        columns = "element; failureMode; severity; target; rpn = severity * 2";
                    }
                }
                part p { @Fmea { failureMode = "x"; severity = 3; } }
            }
        "#;
        let hazards = vec![parse_file("t.sysml", src)];
        let fields = |t: &RenderedTable| -> Vec<Option<String>> {
            t.editable
                .iter()
                .map(|e| e.as_ref().map(|e| e.field.clone()))
                .collect()
        };
        let log = render_view(&hazards, &[], "Log").unwrap();
        assert_eq!(
            fields(&log),
            [
                Some("name".to_string()),
                Some("feature:description".to_string()),
                None,
                None
            ]
        );
        assert!(log.available_fields.contains(&"parent".to_string()));
        let sheet = render_view(&hazards, &[], "Sheet").unwrap();
        assert_eq!(
            fields(&sheet),
            [
                None,
                Some("metadata:Fmea.failureMode".to_string()),
                Some("metadata:Fmea.severity".to_string()),
                None,
                None
            ]
        );
        assert_eq!(sheet.editable[2].as_ref().unwrap().kind, ValueKind::Number);
        let matrix = render_view(&models(), &[], "Matrix").unwrap();
        assert!(matrix.editable.iter().all(Option::is_none));
        assert_eq!(matrix.editable.len(), matrix.columns.len());
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

    fn col<'a>(t: &'a RenderedTable, name: &str) -> Vec<&'a str> {
        let i = t
            .columns
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| panic!("no column {name} in {:?}", t.columns));
        t.rows.iter().map(|r| r[i].as_str()).collect()
    }

    const SCOPED_FMEA: &str = r#"
        package Lib {
            metadata def TableRendering;
            metadata def Fmea;
            view def Worksheet {
                @TableRendering {
                    rows = "@Fmea";
                    columns = "element; failureMode; severity";
                    sortBy = "-severity";
                }
            }
        }
        package Power {
            part def Battery {
                @Fmea { failureMode = "Thermal runaway"; severity = 9; }
            }
            package Cells {
                part def Cell { @Fmea { failureMode = "Short"; severity = 7; } }
            }
        }
        package Motion {
            part def Motor { @Fmea { failureMode = "Seizure"; severity = 8; } }
        }
        package Views {
            view power : Lib::Worksheet { expose Power::*; }
            view powerDeep : Lib::Worksheet { expose Power::**; }
            view highOnly : Lib::Worksheet {
                doc /* Only the worst. */
                @TableRendering { where = "severity >= 9"; }
            }
            view motorOnly : Lib::Worksheet { expose Motion::Motor; }
        }
    "#;

    /// A view usage inherits its def's table and its own `expose`
    /// narrows the rows: `::*` keeps direct members, `::**` everything
    /// nested, a bare name the element itself.
    #[test]
    fn view_usages_inherit_the_table_and_expose_narrows_rows() {
        let ms = vec![parse_file("s.sysml", SCOPED_FMEA)];
        let all = render_view(&ms, &[], "Worksheet").unwrap();
        assert_eq!(all.rows.len(), 3, "{all:?}");

        let power = render_view(&ms, &[], "power").unwrap();
        assert_eq!(power.view, "power");
        assert_eq!(col(&power, "failureMode"), vec!["Thermal runaway"]);
        assert!(power.warnings.is_empty(), "{:?}", power.warnings);

        let deep = render_view(&ms, &[], "powerDeep").unwrap();
        assert_eq!(col(&deep, "failureMode"), vec!["Thermal runaway", "Short"]);

        let motor = render_view(&ms, &[], "motorOnly").unwrap();
        assert_eq!(col(&motor, "failureMode"), vec!["Seizure"]);

        // Qualified names find the usage too.
        let q = render_view(&ms, &[], "Views::power").unwrap();
        assert_eq!(q.rows.len(), 1);
    }

    /// The usage's own @TableRendering overrides single fields of the
    /// def's — here adding a `where`.
    #[test]
    fn view_usage_annotation_overrides_inherited_fields() {
        let ms = vec![parse_file("s.sysml", SCOPED_FMEA)];
        let t = render_view(&ms, &[], "highOnly").unwrap();
        assert_eq!(t.columns, vec!["element", "failureMode", "severity"]);
        assert_eq!(col(&t, "failureMode"), vec!["Thermal runaway"]);
    }

    #[test]
    fn list_views_reports_defs_and_usages() {
        let ms = vec![parse_file("s.sysml", SCOPED_FMEA)];
        let views = list_views(&ms);
        let def = views.iter().find(|v| v.name == "Worksheet").unwrap();
        assert!(!def.is_usage && def.has_table);
        assert_eq!(def.qualified_name.as_deref(), Some("Lib::Worksheet"));
        let high = views.iter().find(|v| v.name == "highOnly").unwrap();
        assert!(high.is_usage && high.has_table);
        assert_eq!(high.definition.as_deref(), Some("Worksheet"));
        assert_eq!(high.doc.as_deref(), Some("Only the worst."));
        assert_eq!(high.qualified_name.as_deref(), Some("Views::highOnly"));
    }

    #[test]
    fn view_usage_of_a_diagram_def_has_no_table() {
        let src = "package P { view def GeneralView; view v : GeneralView { expose P::*; } }";
        let ms = vec![parse_file("v.sysml", src)];
        let v = list_views(&ms).into_iter().find(|v| v.name == "v").unwrap();
        assert!(v.is_usage && !v.has_table);
        let err = render_view(&ms, &[], "v").unwrap_err();
        assert!(err.contains("typed by `GeneralView`"), "{err}");
    }

    const REQUIREMENTS: &str = r#"
        package Reqs {
            enum def RequirementKind { enum performance; enum safety; }
            enum def VerificationMethod { enum test; enum analysis; }
            requirement def Base {
                attribute kind : RequirementKind[0..1];
                attribute verificationMethod : VerificationMethod[0..*];
            }
            requirement <'R-1'> 'Flight endurance' : Base {
                doc /* The drone shall fly for 25 minutes
                     * with a 1 kg payload. */
                :>> kind = RequirementKind::performance;
                :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
                @StatusInfo { status = StatusKind::done; owner = "J. Hale"; }
                @Rationale { text = "A delivery takes 10 minutes."; }
                @Issue { text = "Headwind not modelled."; }
            }
            requirement <'R-2'> 'Geofencing' : Base {
                doc /* The drone shall stay inside the geofence. */
                :>> kind = RequirementKind::safety;
            }
            requirement <'R-3'> 'Noise' : Base {
                doc /* Quiet in hover. */
            }
            view def Gaps {
                @TableRendering {
                    rows = "requirements";
                    columns = "id; title; satisfied; verified";
                    where = "satisfied == 0 or verified == 0";
                    sortBy = "id";
                }
            }
            view def All {
                @TableRendering {
                    rows = "requirements";
                    columns = "id; title; text; kind; verificationMethod; status; owner; rationale; issues; satisfiedBy; verifiedBy; satisfied; verified; type; line";
                    sortBy = "id";
                }
            }
        }
    "#;

    /// Satisfy and verify live in other files and still count, by name
    /// or by `<id>`.
    const TRACE_LINKS: &str = r#"
        package Design {
            part drone;
            satisfy Reqs::'Flight endurance' by drone;
            satisfy 'R-2' by drone.controller;
            verification flightTest { objective { verify 'Flight endurance'; } }
        }
    "#;

    fn requirement_models() -> Vec<Model> {
        vec![
            parse_file("reqs.sysml", REQUIREMENTS),
            parse_file("design.sysml", TRACE_LINKS),
        ]
    }

    #[test]
    fn requirement_rows_carry_attributes_status_and_links() {
        let ms = requirement_models();
        // Only the requirements file is content; links come from the
        // whole project.
        let t = render_view(&ms, &["reqs.sysml".to_string()], "All").unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(
            col(&t, "id"),
            vec!["R-1", "R-2", "R-3"],
            "Base is a classifier"
        );
        let r1: BTreeMap<&str, &str> = t
            .columns
            .iter()
            .map(String::as_str)
            .zip(t.rows[0].iter().map(String::as_str))
            .collect();
        assert_eq!(r1["title"], "Flight endurance");
        assert_eq!(
            r1["text"],
            "The drone shall fly for 25 minutes with a 1 kg payload."
        );
        assert_eq!(r1["kind"], "performance");
        assert_eq!(r1["verificationMethod"], "analysis, test");
        assert_eq!(r1["status"], "done");
        assert_eq!(r1["owner"], "J. Hale");
        assert_eq!(r1["rationale"], "A delivery takes 10 minutes.");
        assert_eq!(r1["issues"], "Headwind not modelled.");
        assert_eq!(r1["satisfiedBy"], "drone");
        assert_eq!(r1["verifiedBy"], "flightTest");
        assert_eq!((r1["satisfied"], r1["verified"]), ("1", "1"));
        assert_eq!(r1["type"], "Base");
        assert_eq!(r1["line"], "9");
        assert_eq!(col(&t, "satisfiedBy")[1], "drone.controller");
        assert_eq!(
            col(&t, "kind")[2],
            "",
            "missing attributes are empty, not absent"
        );
        assert_eq!(
            t.sources[0],
            RowSource {
                element: "Reqs::'Flight endurance'".into(),
                file: "reqs.sysml".into(),
                line: 9,
            }
        );
    }

    #[test]
    fn where_selects_requirement_gaps() {
        let ms = requirement_models();
        let t = render_view(&ms, &["reqs.sysml".to_string()], "Gaps").unwrap();
        assert_eq!(col(&t, "id"), vec!["R-2", "R-3"]);
        assert_eq!(col(&t, "satisfied"), vec!["1", "0"]);
        assert_eq!(col(&t, "verified"), vec!["0", "0"]);
    }

    /// `trace` keeps its yes/no spelling and now covers requirement
    /// usages, not only defs.
    #[test]
    fn trace_rows_cover_requirement_usages() {
        let mut src = REQUIREMENTS.to_string();
        src = src.replace(
            "view def All {",
            "view def Rtm { @TableRendering { rows = \"trace\"; columns = \"requirement; id; satisfied; verified\"; } }\n view def All {",
        );
        let ms = vec![
            parse_file("reqs.sysml", &src),
            parse_file("design.sysml", TRACE_LINKS),
        ];
        let t = render_view(&ms, &[], "Rtm").unwrap();
        assert_eq!(
            t.rows,
            vec![
                vec!["Flight endurance", "R-1", "yes", "yes"],
                vec!["Geofencing", "R-2", "yes", "no"],
                vec!["Noise", "R-3", "no", "no"],
            ]
        );
    }

    #[test]
    fn satisfy_rows_name_the_requirement_and_pivot_in_order() {
        let mut src = REQUIREMENTS.to_string();
        src = src.replace(
            "view def All {",
            "view def Tm { @TableRendering { rows = \"relation:satisfy\"; pivot = \"requirement; by\"; sortBy = \"requirement\"; } }\n view def All {",
        );
        let ms = vec![
            parse_file("reqs.sysml", &src),
            parse_file("design.sysml", TRACE_LINKS),
        ];
        let t = render_view(&ms, &[], "Tm").unwrap();
        assert_eq!(
            t.columns,
            vec!["requirement\\by", "drone", "drone.controller"]
        );
        assert_eq!(
            t.rows,
            vec![
                vec!["R-1 Flight endurance", "1", ""],
                vec!["R-2 Geofencing", "", "1"],
            ]
        );
    }

    /// Ports: owner, conjugated type, the direction their port def
    /// implies, and what they connect to through which connection.
    #[test]
    fn port_rows_show_direction_and_connections() {
        let src = r#"
            package P {
                metadata def TableRendering;
                item def Power;
                port def PowerPort { out item power : Power; }
                part def Battery { port powerOut : PowerPort; }
                part def Load { port powerIn : ~PowerPort; port spare : PowerPort; }
                interface def Cable { end a : PowerPort; end b : ~PowerPort; }
                part system {
                    part battery : Battery;
                    part load : Load;
                    interface cable : Cable connect battery.powerOut to load.powerIn;
                }
                view def Ports {
                    @TableRendering {
                        rows = "kind:port";
                        columns = "owner; element; type; direction; connectedTo; via";
                    }
                }
            }
        "#;
        let ms = vec![parse_file("p.sysml", src)];
        let t = render_view(&ms, &[], "Ports").unwrap();
        assert_eq!(
            t.rows,
            vec![
                vec![
                    "Battery",
                    "powerOut",
                    "PowerPort",
                    "out",
                    "load.powerIn",
                    "cable"
                ],
                vec![
                    "Load",
                    "powerIn",
                    "~PowerPort",
                    "in",
                    "battery.powerOut",
                    "cable"
                ],
                vec!["Load", "spare", "PowerPort", "out", "", ""],
            ],
            "{t:?}"
        );
    }

    /// The BOM walks into a usage's own body (`part drone : D { part
    /// ... }`) as well as its type, starting from a package's top-level
    /// parts when no root is named, and a view usage can narrow it.
    #[test]
    fn composition_walks_usage_bodies_from_package_roots() {
        let src = r#"
            package Arch {
                metadata def TableRendering;
                part def Rotor { part motor : Motor; }
                part def Motor;
                part def Drone;
                part drone : Drone {
                    part rotors : Rotor[4];
                    part battery : Battery;
                }
                part def Battery;
                view def Bom {
                    @TableRendering {
                        rows = "composition";
                        columns = "path; quantity; extended";
                        sortBy = "path";
                    }
                }
                view rotorsOnly : Bom { expose Arch::drone::rotors::**; }
            }
        "#;
        let ms = vec![parse_file("b.sysml", src)];
        let t = render_view(&ms, &[], "Bom").unwrap();
        assert_eq!(
            t.rows,
            vec![
                vec!["drone", "1", "1"],
                vec!["drone.battery", "1", "1"],
                vec!["drone.rotors", "4", "4"],
                vec!["drone.rotors.motor", "1", "4"],
            ]
        );
        let t = render_view(&ms, &[], "rotorsOnly").unwrap();
        assert_eq!(col(&t, "path"), vec!["drone.rotors.motor"]);
    }

    /// `filter @SysML::<Metaclass>` keeps rows whose element is of that
    /// kind; `filter @Meta` rows whose element carries the metadata.
    #[test]
    fn filters_select_metaclasses_and_metadata() {
        let src = r#"
            package P {
                metadata def TableRendering;
                metadata def Critical;
                part def Box { part a : A; item b : B; part c : A { @Critical { level = 1; } } }
                part def A; item def B;
                view def Bom {
                    @TableRendering { rows = "composition:Box"; columns = "element"; }
                }
                view items : Bom { filter @SysML::ItemUsage; }
                view critical : Bom { filter @Critical; }
                view either : Bom { filter @SysML::ItemUsage or @Critical; }
            }
        "#;
        let ms = vec![parse_file("f.sysml", src)];
        let get = |v: &str| {
            let t = render_view(&ms, &[], v).unwrap();
            assert!(t.warnings.is_empty(), "{:?}", t.warnings);
            t.rows.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>()
        };
        assert_eq!(get("items"), vec!["b"]);
        assert_eq!(get("critical"), vec!["c"]);
        assert_eq!(get("either"), vec!["b", "c"]);
    }

    #[test]
    fn collapse_ws_drops_comment_layout() {
        assert_eq!(collapse_ws("one\n * two\n *  three"), "one two three");
        assert_eq!(display_value("(K::a, K::b)"), "a, b");
        assert_eq!(display_value("\"x\""), "x");
        assert_eq!(display_value("25 [min]"), "25 [min]");
        assert_eq!(display_value("Kind::'use'"), "use");
    }
}
