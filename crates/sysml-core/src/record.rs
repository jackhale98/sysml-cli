//! Operational records stored as TOML files that diff cleanly in git.
//!
//! Records use a three-section envelope format: `[meta]` for identity and
//! provenance, `[refs]` for qualified-name references into the SysML model,
//! and `[data]` for domain-specific content.  All maps use [`BTreeMap`] so
//! that serialized output is deterministic and produces minimal diffs.

use std::collections::BTreeMap;
use std::fmt;
use std::time::SystemTime;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when working with records.
#[derive(Debug)]
pub enum RecordError {
    /// A required section or key is missing.
    MissingField(String),
    /// A value could not be parsed as the expected type.
    InvalidValue { key: String, detail: String },
    /// The TOML text is syntactically malformed.
    ParseError(String),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(name) => write!(f, "missing required field: {name}"),
            Self::InvalidValue { key, detail } => {
                write!(f, "invalid value for '{key}': {detail}")
            }
            Self::ParseError(msg) => write!(f, "TOML parse error: {msg}"),
        }
    }
}

impl std::error::Error for RecordError {}

// ---------------------------------------------------------------------------
// RecordValue
// ---------------------------------------------------------------------------

/// A dynamically-typed value that can appear in the `[data]` section.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RecordValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<RecordValue>),
    Table(BTreeMap<String, RecordValue>),
}

impl fmt::Display for RecordValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Integer(n) => write!(f, "{n}"),
            Self::Float(v) => {
                // Ensure there is always a decimal point so it round-trips as float.
                if v.fract() == 0.0 {
                    write!(f, "{v:.1}")
                } else {
                    write!(f, "{v}")
                }
            }
            Self::Bool(b) => write!(f, "{b}"),
            Self::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Self::Table(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RecordMeta & RecordEnvelope
// ---------------------------------------------------------------------------

/// Identity and provenance for a record.
#[derive(Debug, Clone, Serialize)]
pub struct RecordMeta {
    pub id: String,
    pub tool: String,
    pub record_type: String,
    /// ISO 8601 timestamp (e.g. `2026-03-09T14:30:00Z`).
    pub created: String,
    pub author: String,
}

/// A complete record in the three-section envelope format.
#[derive(Debug, Clone, Serialize)]
pub struct RecordEnvelope {
    pub meta: RecordMeta,
    /// Model references keyed by role name, each holding a list of qualified
    /// names.
    pub refs: BTreeMap<String, Vec<String>>,
    /// Domain-specific content.
    pub data: BTreeMap<String, RecordValue>,
}

// ---------------------------------------------------------------------------
// TOML serializer (hand-written — covers our known schema)
// ---------------------------------------------------------------------------

impl RecordEnvelope {
    /// Serialize to a human-readable TOML string with `[meta]`, `[refs]`, and
    /// `[data]` sections.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();

        // -- [meta] --
        out.push_str("[meta]\n");
        push_kv_string(&mut out, "id", &self.meta.id);
        push_kv_string(&mut out, "tool", &self.meta.tool);
        push_kv_string(&mut out, "record_type", &self.meta.record_type);
        push_kv_string(&mut out, "created", &self.meta.created);
        push_kv_string(&mut out, "author", &self.meta.author);

        // -- [refs] --
        out.push('\n');
        out.push_str("[refs]\n");
        for (key, names) in &self.refs {
            push_kv_string_array(&mut out, key, names);
        }

        // -- [data] --
        out.push('\n');
        out.push_str("[data]\n");
        write_data_section(&mut out, &self.data, "data");

        out
    }

    /// Parse a TOML string in the expected envelope format.
    pub fn from_toml_str(s: &str) -> Result<Self, RecordError> {
        let root = parse_toml_tables(s)?;

        let meta_tbl = root
            .get("meta")
            .and_then(|v| match v {
                TomlNode::Table(t) => Some(t),
                _ => None,
            })
            .ok_or_else(|| RecordError::MissingField("meta".into()))?;

        let meta = RecordMeta {
            id: require_string(meta_tbl, "id")?,
            tool: require_string(meta_tbl, "tool")?,
            record_type: require_string(meta_tbl, "record_type")?,
            created: require_string(meta_tbl, "created")?,
            author: require_string(meta_tbl, "author")?,
        };

        let refs_tbl = root
            .get("refs")
            .and_then(|v| match v {
                TomlNode::Table(t) => Some(t),
                _ => None,
            })
            .ok_or_else(|| RecordError::MissingField("refs".into()))?;

        let mut refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (key, node) in refs_tbl {
            match node {
                TomlNode::Array(arr) => {
                    let mut names = Vec::new();
                    for item in arr {
                        match item {
                            TomlNode::String(s) => names.push(s.clone()),
                            other => {
                                return Err(RecordError::InvalidValue {
                                    key: key.clone(),
                                    detail: format!("expected string in array, got {other:?}"),
                                })
                            }
                        }
                    }
                    refs.insert(key.clone(), names);
                }
                other => {
                    return Err(RecordError::InvalidValue {
                        key: key.clone(),
                        detail: format!("expected array, got {other:?}"),
                    })
                }
            }
        }

        let data_tbl = root
            .get("data")
            .and_then(|v| match v {
                TomlNode::Table(t) => Some(t),
                _ => None,
            })
            .ok_or_else(|| RecordError::MissingField("data".into()))?;

        let data = toml_table_to_record_data(data_tbl)?;

        Ok(Self { meta, refs, data })
    }
}

// ---------------------------------------------------------------------------
// TOML serialization helpers
// ---------------------------------------------------------------------------

/// Write `key = "value"` with proper escaping.
fn push_kv_string(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    push_escaped(out, value);
    out.push_str("\"\n");
}

/// Write `key = ["a", "b"]`.
fn push_kv_string_array(out: &mut String, key: &str, items: &[String]) {
    out.push_str(key);
    out.push_str(" = [");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('"');
        push_escaped(out, item);
        out.push('"');
    }
    out.push_str("]\n");
}

/// Escape a string value for TOML basic strings.
fn push_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
}

/// Serialize a `RecordValue` as an inline TOML value.
fn push_value(out: &mut String, val: &RecordValue) {
    match val {
        RecordValue::String(s) => {
            out.push('"');
            push_escaped(out, s);
            out.push('"');
        }
        RecordValue::Integer(n) => {
            out.push_str(&n.to_string());
        }
        RecordValue::Float(v) => {
            if v.fract() == 0.0 {
                out.push_str(&format!("{v:.1}"));
            } else {
                out.push_str(&v.to_string());
            }
        }
        RecordValue::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
        }
        RecordValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_value(out, item);
            }
            out.push(']');
        }
        RecordValue::Table(map) => {
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(" = ");
                push_value(out, v);
            }
            out.push('}');
        }
    }
}

/// Write data entries.  Sub-tables are emitted as TOML sub-table headers
/// (`[data.subkey]`) so the output stays readable.
fn write_data_section(out: &mut String, map: &BTreeMap<String, RecordValue>, prefix: &str) {
    // First pass: scalar / array / inline-table values.
    for (k, v) in map {
        if matches!(v, RecordValue::Table(_)) {
            continue;
        }
        out.push_str(k);
        out.push_str(" = ");
        push_value(out, v);
        out.push('\n');
    }

    // Second pass: sub-tables get their own header.
    for (k, v) in map {
        if let RecordValue::Table(inner) = v {
            out.push('\n');
            out.push_str(&format!("[{prefix}.{k}]\n"));
            write_data_section(out, inner, &format!("{prefix}.{k}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal TOML parser (covers the envelope schema)
// ---------------------------------------------------------------------------

/// Intermediate AST produced by the TOML parser.
#[derive(Debug, Clone)]
enum TomlNode {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<TomlNode>),
    Table(BTreeMap<String, TomlNode>),
}

/// Parse a TOML string into a tree of `TomlNode` tables.
fn parse_toml_tables(input: &str) -> Result<BTreeMap<String, TomlNode>, RecordError> {
    let mut root: BTreeMap<String, TomlNode> = BTreeMap::new();
    let mut current_path: Vec<String> = Vec::new();

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = strip_comment(raw_line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Table header: [foo] or [foo.bar]
        if line.starts_with('[') && !line.starts_with("[[") {
            let header = line.trim_start_matches('[').trim_end_matches(']').trim();
            if header.is_empty() {
                return Err(RecordError::ParseError(format!(
                    "empty table header at line {}",
                    line_no + 1
                )));
            }
            current_path = header.split('.').map(|s| s.trim().to_string()).collect();
            // Ensure every segment along the path exists as a table.
            ensure_table_path(&mut root, &current_path);
            continue;
        }

        // Key = value
        if let Some(eq_pos) = find_top_level_eq(line) {
            let key = line[..eq_pos].trim().to_string();
            let val_str = line[eq_pos + 1..].trim();
            let value = parse_value(val_str, line_no)?;

            let table = get_table_mut(&mut root, &current_path).ok_or_else(|| {
                RecordError::ParseError(format!(
                    "cannot find table for path {:?} at line {}",
                    current_path,
                    line_no + 1
                ))
            })?;
            table.insert(key, value);
            continue;
        }

        return Err(RecordError::ParseError(format!(
            "unrecognized syntax at line {}: {line}",
            line_no + 1
        )));
    }

    Ok(root)
}

/// Strip an end-of-line comment (respecting quoted strings).
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut prev_escape = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' && !prev_escape {
            in_string = !in_string;
        }
        if ch == '#' && !in_string {
            return &line[..i];
        }
        prev_escape = ch == '\\' && !prev_escape;
    }
    line
}

/// Find the position of the first `=` that is not inside quotes.
fn find_top_level_eq(line: &str) -> Option<usize> {
    let mut in_string = false;
    let mut prev_escape = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' && !prev_escape {
            in_string = !in_string;
        }
        if ch == '=' && !in_string {
            return Some(i);
        }
        prev_escape = ch == '\\' && !prev_escape;
    }
    None
}

/// Parse an inline TOML value.
fn parse_value(s: &str, line_no: usize) -> Result<TomlNode, RecordError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(RecordError::ParseError(format!(
            "empty value at line {}",
            line_no + 1
        )));
    }

    // Boolean
    if s == "true" {
        return Ok(TomlNode::Bool(true));
    }
    if s == "false" {
        return Ok(TomlNode::Bool(false));
    }

    // String
    if s.starts_with('"') {
        return parse_basic_string(s, line_no).map(TomlNode::String);
    }

    // Array
    if s.starts_with('[') {
        return parse_array(s, line_no);
    }

    // Inline table
    if s.starts_with('{') {
        return parse_inline_table(s, line_no);
    }

    // Number: try integer first, then float.
    if let Ok(n) = s.parse::<i64>() {
        return Ok(TomlNode::Integer(n));
    }
    if let Ok(v) = s.parse::<f64>() {
        return Ok(TomlNode::Float(v));
    }

    Err(RecordError::ParseError(format!(
        "cannot parse value '{s}' at line {}",
        line_no + 1
    )))
}

/// Parse a basic (double-quoted) TOML string, handling escape sequences.
fn parse_basic_string(s: &str, line_no: usize) -> Result<String, RecordError> {
    // s starts with '"'
    if s.len() < 2 || !s.ends_with('"') {
        return Err(RecordError::ParseError(format!(
            "unterminated string at line {}",
            line_no + 1
        )));
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    return Err(RecordError::ParseError(format!(
                        "unknown escape '\\{other}' at line {}",
                        line_no + 1
                    )));
                }
                None => {
                    return Err(RecordError::ParseError(format!(
                        "trailing backslash at line {}",
                        line_no + 1
                    )));
                }
            }
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

/// Parse an inline array `[a, b, c]`.
fn parse_array(s: &str, line_no: usize) -> Result<TomlNode, RecordError> {
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err(RecordError::ParseError(format!(
            "malformed array at line {}",
            line_no + 1
        )));
    }
    let inner = s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return Ok(TomlNode::Array(Vec::new()));
    }

    let parts = split_top_level(inner, ',');
    let mut items = Vec::new();
    for part in &parts {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            items.push(parse_value(trimmed, line_no)?);
        }
    }
    Ok(TomlNode::Array(items))
}

/// Parse an inline table `{key = val, ...}`.
fn parse_inline_table(s: &str, line_no: usize) -> Result<TomlNode, RecordError> {
    if !s.starts_with('{') || !s.ends_with('}') {
        return Err(RecordError::ParseError(format!(
            "malformed inline table at line {}",
            line_no + 1
        )));
    }
    let inner = s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return Ok(TomlNode::Table(BTreeMap::new()));
    }

    let parts = split_top_level(inner, ',');
    let mut table = BTreeMap::new();
    for part in &parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(eq_pos) = find_top_level_eq(trimmed) {
            let key = trimmed[..eq_pos].trim().to_string();
            let val_str = trimmed[eq_pos + 1..].trim();
            table.insert(key, parse_value(val_str, line_no)?);
        } else {
            return Err(RecordError::ParseError(format!(
                "missing '=' in inline table entry '{trimmed}' at line {}",
                line_no + 1
            )));
        }
    }
    Ok(TomlNode::Table(table))
}

/// Split a string by `delim` while respecting quotes, brackets, and braces.
fn split_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut prev_escape = false;

    for ch in s.chars() {
        if ch == '"' && !prev_escape {
            in_string = !in_string;
        }
        if !in_string {
            if ch == '[' || ch == '{' {
                depth += 1;
            } else if ch == ']' || ch == '}' {
                depth -= 1;
            } else if ch == delim && depth == 0 {
                parts.push(std::mem::take(&mut current));
                prev_escape = false;
                continue;
            }
        }
        prev_escape = ch == '\\' && !prev_escape;
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Ensure a nested table path exists in the root, creating empty tables as
/// needed.
fn ensure_table_path(root: &mut BTreeMap<String, TomlNode>, path: &[String]) {
    let mut current = root;
    for segment in path {
        let entry = current
            .entry(segment.clone())
            .or_insert_with(|| TomlNode::Table(BTreeMap::new()));
        current = match entry {
            TomlNode::Table(t) => t,
            _ => return, // path conflict — bail silently
        };
    }
}

/// Navigate to a mutable reference for the table at the given path.
fn get_table_mut<'a>(
    root: &'a mut BTreeMap<String, TomlNode>,
    path: &[String],
) -> Option<&'a mut BTreeMap<String, TomlNode>> {
    if path.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in path {
        let node = current.get_mut(segment)?;
        current = match node {
            TomlNode::Table(t) => t,
            _ => return None,
        };
    }
    Some(current)
}

/// Convert a parsed TOML table into `RecordValue` data.
fn toml_table_to_record_data(
    table: &BTreeMap<String, TomlNode>,
) -> Result<BTreeMap<String, RecordValue>, RecordError> {
    let mut out = BTreeMap::new();
    for (key, node) in table {
        out.insert(key.clone(), toml_node_to_record_value(node)?);
    }
    Ok(out)
}

/// Convert a single `TomlNode` into a `RecordValue`.
fn toml_node_to_record_value(node: &TomlNode) -> Result<RecordValue, RecordError> {
    match node {
        TomlNode::String(s) => Ok(RecordValue::String(s.clone())),
        TomlNode::Integer(n) => Ok(RecordValue::Integer(*n)),
        TomlNode::Float(v) => Ok(RecordValue::Float(*v)),
        TomlNode::Bool(b) => Ok(RecordValue::Bool(*b)),
        TomlNode::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(toml_node_to_record_value).collect();
            Ok(RecordValue::Array(items?))
        }
        TomlNode::Table(tbl) => {
            let inner = toml_table_to_record_data(tbl)?;
            Ok(RecordValue::Table(inner))
        }
    }
}

/// Extract a required string from a parsed TOML table.
fn require_string(table: &BTreeMap<String, TomlNode>, key: &str) -> Result<String, RecordError> {
    match table.get(key) {
        Some(TomlNode::String(s)) => Ok(s.clone()),
        Some(other) => Err(RecordError::InvalidValue {
            key: key.into(),
            detail: format!("expected string, got {other:?}"),
        }),
        None => Err(RecordError::MissingField(key.into())),
    }
}

// ---------------------------------------------------------------------------
// ID generation & helpers
// ---------------------------------------------------------------------------

/// Generate an ISO 8601 UTC timestamp from the system clock.
///
/// Returns a string like `2026-03-09T14:30:00Z`.  Uses only `std` so there
/// is no dependency on `chrono` or `time`.
pub fn now_iso8601() -> String {
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Break epoch seconds into calendar components (UTC).
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Convert days since 1970-01-01 to year-month-day.
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm adapted from Howard Hinnant's `civil_from_days`.
    let z = days as i64 + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as u64, m, d)
}

/// Generate a record ID in the format
/// `{tool}-{record_type}-{timestamp}-{author}-{hash}`.
///
/// The hash is a 4-hex-character fingerprint derived from all fields so that
/// concurrent records by the same author are unlikely to collide.
pub fn generate_record_id(tool: &str, record_type: &str, author: &str) -> String {
    let ts = now_iso8601();
    // Compact timestamp for the filename portion: remove punctuation.
    let ts_compact: String = ts.chars().filter(|c| c.is_ascii_digit()).collect();

    let hash = short_hash(&format!("{tool}{record_type}{ts}{author}"));
    format!("{tool}-{record_type}-{ts_compact}-{author}-{hash}")
}

/// Compute a 4-hex-character hash of `input` using a simple FNV-1a-style
/// algorithm (no external crate needed).
fn short_hash(input: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in input.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
    }
    // Take the bottom 16 bits for a 4-hex-char representation.
    format!("{:04x}", h as u16)
}

/// Return the canonical filename for a record ID.
pub fn record_filename(id: &str) -> String {
    format!("{id}.toml")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal test envelope.
    fn sample_envelope() -> RecordEnvelope {
        let mut refs = BTreeMap::new();
        refs.insert(
            "requirements".into(),
            vec!["Vehicle::Braking::StopDistance".into()],
        );
        refs.insert(
            "verified_by".into(),
            vec![
                "Vehicle::Tests::BrakeTest".into(),
                "Vehicle::Tests::ABSTest".into(),
            ],
        );

        let mut data = BTreeMap::new();
        data.insert("status".into(), RecordValue::String("pass".into()));
        data.insert("duration_ms".into(), RecordValue::Integer(1250));
        data.insert("coverage".into(), RecordValue::Float(0.87));
        data.insert("automated".into(), RecordValue::Bool(true));
        data.insert(
            "tags".into(),
            RecordValue::Array(vec![
                RecordValue::String("safety".into()),
                RecordValue::String("braking".into()),
            ]),
        );

        let mut metrics = BTreeMap::new();
        metrics.insert("assertions".into(), RecordValue::Integer(42));
        metrics.insert("failures".into(), RecordValue::Integer(0));
        data.insert("metrics".into(), RecordValue::Table(metrics));

        RecordEnvelope {
            meta: RecordMeta {
                id: "coverage-vv-20260309T143000Z-alice-ab12".into(),
                tool: "coverage".into(),
                record_type: "vv".into(),
                created: "2026-03-09T14:30:00Z".into(),
                author: "alice".into(),
            },
            refs,
            data,
        }
    }

    #[test]
    fn round_trip_toml() {
        let envelope = sample_envelope();
        let toml_str = envelope.to_toml_string();

        // Verify structural markers are present.
        assert!(toml_str.contains("[meta]"));
        assert!(toml_str.contains("[refs]"));
        assert!(toml_str.contains("[data]"));
        assert!(toml_str.contains("[data.metrics]"));

        // Round-trip.
        let parsed = RecordEnvelope::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.meta.id, envelope.meta.id);
        assert_eq!(parsed.meta.tool, envelope.meta.tool);
        assert_eq!(parsed.meta.record_type, envelope.meta.record_type);
        assert_eq!(parsed.meta.created, envelope.meta.created);
        assert_eq!(parsed.meta.author, envelope.meta.author);
        assert_eq!(parsed.refs, envelope.refs);
        assert_eq!(parsed.data, envelope.data);
    }

    #[test]
    fn serialization_is_deterministic() {
        let envelope = sample_envelope();
        let a = envelope.to_toml_string();
        let b = envelope.to_toml_string();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_empty_refs_and_data() {
        let toml = "\
[meta]
id = \"test-check-20260309-bob-0000\"
tool = \"test\"
record_type = \"check\"
created = \"2026-03-09T00:00:00Z\"
author = \"bob\"

[refs]

[data]
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.meta.author, "bob");
        assert!(env.refs.is_empty());
        assert!(env.data.is_empty());
    }

    #[test]
    fn parse_error_missing_meta() {
        let toml = "[refs]\n[data]\n";
        let err = RecordEnvelope::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, RecordError::MissingField(_)));
    }

    #[test]
    fn parse_error_missing_field() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"

[refs]

[data]
";
        let err = RecordEnvelope::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, RecordError::MissingField(_)));
    }

    #[test]
    fn parse_inline_table_in_data() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]

[data]
info = {alpha = 1, beta = \"two\"}
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        match env.data.get("info").unwrap() {
            RecordValue::Table(t) => {
                assert_eq!(t.get("alpha"), Some(&RecordValue::Integer(1)));
                assert_eq!(t.get("beta"), Some(&RecordValue::String("two".into())));
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn parse_sub_table_header_in_data() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]

[data]
top = 1

[data.nested]
inner = \"hello\"
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.data.get("top"), Some(&RecordValue::Integer(1)));
        match env.data.get("nested").unwrap() {
            RecordValue::Table(t) => {
                assert_eq!(t.get("inner"), Some(&RecordValue::String("hello".into())));
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn string_escaping_round_trip() {
        let mut data = BTreeMap::new();
        data.insert(
            "note".into(),
            RecordValue::String("line1\nline2\ttab \"quoted\" \\back".into()),
        );

        let envelope = RecordEnvelope {
            meta: RecordMeta {
                id: "test-id".into(),
                tool: "t".into(),
                record_type: "r".into(),
                created: "2026-01-01T00:00:00Z".into(),
                author: "a".into(),
            },
            refs: BTreeMap::new(),
            data,
        };

        let toml = envelope.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(
            parsed.data.get("note"),
            Some(&RecordValue::String(
                "line1\nline2\ttab \"quoted\" \\back".into()
            ))
        );
    }

    #[test]
    fn record_value_display() {
        assert_eq!(RecordValue::String("hi".into()).to_string(), "hi");
        assert_eq!(RecordValue::Integer(42).to_string(), "42");
        assert_eq!(RecordValue::Float(1.5).to_string(), "1.5");
        assert_eq!(RecordValue::Float(2.0).to_string(), "2.0");
        assert_eq!(RecordValue::Bool(true).to_string(), "true");

        let arr = RecordValue::Array(vec![RecordValue::Integer(1), RecordValue::Integer(2)]);
        assert_eq!(arr.to_string(), "[1, 2]");

        let mut m = BTreeMap::new();
        m.insert("x".into(), RecordValue::Integer(10));
        let tbl = RecordValue::Table(m);
        assert_eq!(tbl.to_string(), "{x = 10}");
    }

    #[test]
    fn record_error_display() {
        let e = RecordError::MissingField("meta".into());
        assert_eq!(e.to_string(), "missing required field: meta");

        let e = RecordError::InvalidValue {
            key: "x".into(),
            detail: "bad".into(),
        };
        assert_eq!(e.to_string(), "invalid value for 'x': bad");

        let e = RecordError::ParseError("oops".into());
        assert_eq!(e.to_string(), "TOML parse error: oops");
    }

    #[test]
    fn generate_id_format() {
        let id = generate_record_id("lint", "check", "alice");
        // Should start with tool-type-
        assert!(id.starts_with("lint-check-"));
        // Should end with author-4hexchars
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 4);
        assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));
        assert!(id.contains("alice"));
    }

    #[test]
    fn record_filename_format() {
        assert_eq!(
            record_filename("lint-check-20260309-alice-ab12"),
            "lint-check-20260309-alice-ab12.toml"
        );
    }

    #[test]
    fn now_iso8601_format() {
        let ts = now_iso8601();
        // Basic structure: YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn short_hash_deterministic() {
        let a = short_hash("hello");
        let b = short_hash("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 4);

        // Different inputs produce different hashes (probabilistically).
        let c = short_hash("world");
        assert_ne!(a, c);
    }

    #[test]
    fn parse_booleans_and_floats() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]

[data]
flag = true
neg_flag = false
ratio = 0.5
big = 100
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.data.get("flag"), Some(&RecordValue::Bool(true)));
        assert_eq!(env.data.get("neg_flag"), Some(&RecordValue::Bool(false)));
        assert_eq!(env.data.get("ratio"), Some(&RecordValue::Float(0.5)));
        assert_eq!(env.data.get("big"), Some(&RecordValue::Integer(100)));
    }

    #[test]
    fn parse_nested_arrays() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]
sources = [\"A::B\", \"C::D\"]

[data]
numbers = [1, 2, 3]
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(
            env.refs.get("sources"),
            Some(&vec!["A::B".into(), "C::D".into()])
        );
        assert_eq!(
            env.data.get("numbers"),
            Some(&RecordValue::Array(vec![
                RecordValue::Integer(1),
                RecordValue::Integer(2),
                RecordValue::Integer(3),
            ]))
        );
    }

    #[test]
    fn parse_with_comments() {
        let toml = "\
# This is a record file
[meta]
id = \"x\" # inline comment
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]
# no refs yet

[data]
value = 42 # the answer
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.meta.id, "x");
        assert_eq!(env.data.get("value"), Some(&RecordValue::Integer(42)));
    }

    #[test]
    fn parse_empty_array() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]
empty = []

[data]
items = []
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.refs.get("empty"), Some(&vec![]));
        assert_eq!(env.data.get("items"), Some(&RecordValue::Array(vec![])));
    }

    #[test]
    fn parse_negative_integer() {
        let toml = "\
[meta]
id = \"x\"
tool = \"t\"
record_type = \"r\"
created = \"2026-01-01T00:00:00Z\"
author = \"a\"

[refs]

[data]
offset = -10
";
        let env = RecordEnvelope::from_toml_str(toml).unwrap();
        assert_eq!(env.data.get("offset"), Some(&RecordValue::Integer(-10)));
    }

    #[test]
    fn serialize_then_parse_preserves_types() {
        let mut data = BTreeMap::new();
        data.insert("s".into(), RecordValue::String("text".into()));
        data.insert("i".into(), RecordValue::Integer(-5));
        data.insert("f".into(), RecordValue::Float(1.5));
        data.insert("b".into(), RecordValue::Bool(false));
        data.insert(
            "a".into(),
            RecordValue::Array(vec![
                RecordValue::Bool(true),
                RecordValue::String("x".into()),
            ]),
        );

        let env = RecordEnvelope {
            meta: RecordMeta {
                id: "t-r-20260101-u-0000".into(),
                tool: "t".into(),
                record_type: "r".into(),
                created: "2026-01-01T00:00:00Z".into(),
                author: "u".into(),
            },
            refs: BTreeMap::new(),
            data,
        };

        let toml = env.to_toml_string();
        let parsed = RecordEnvelope::from_toml_str(&toml).unwrap();
        assert_eq!(parsed.data, env.data);
    }

    #[test]
    fn days_to_ymd_known_dates() {
        // 1970-01-01 = day 0
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2000-01-01 = day 10957
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
        // 2026-03-09 = day 20521
        assert_eq!(days_to_ymd(20521), (2026, 3, 9));
    }

    #[test]
    fn serde_json_compatibility() {
        let envelope = sample_envelope();
        // Should serialize to JSON without error via serde.
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"coverage\""));
    }

    #[test]
    fn parse_error_bad_syntax() {
        let toml = "[meta]\nthis is not valid toml\n";
        let err = RecordEnvelope::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, RecordError::ParseError(_)));
    }

    #[test]
    fn parse_error_unterminated_string() {
        let toml = "\
[meta]
id = \"unterminated
";
        let err = RecordEnvelope::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, RecordError::ParseError(_)));
    }
}
