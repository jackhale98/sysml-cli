//! What a table cell may be edited to, and how its value is written.
//!
//! A rendered view says, per column, which piece of SysML text its cells
//! come from — a [`FieldSpec`] — and what kind of value it holds
//! ([`ValueKind`], with the enum literals when the field is typed by an
//! `enum def`). `codegen::field_edit` turns an edited cell back into a
//! minimal text patch of that field.
//!
//! Field specs:
//!
//!   name                    the element's name (`'Flight endurance'`)
//!   shortName               the short name (`<'SYS-001'>`)
//!   doc                     the first `doc /* ... */` of the body
//!   feature:priority        a body value, `:>> priority = Priority::high;`
//!   metadata:StatusInfo.status
//!                           a field of a metadata annotation on the
//!                           element, `@StatusInfo { status = ...; }`
//!
//! Types resolve against the project's models: the element's type and
//! its supertypes declare the feature (`attribute priority : Priority`),
//! and the feature's type is an `enum def` (enum, or enumList when its
//! multiplicity allows several values), a string, a number or a boolean.
//! Metadata defs from the standard library (`StatusInfo`, `Rationale`,
//! `Issue`) resolve against the embedded library when the project does
//! not declare them.

use std::fmt;

use crate::model::{simple_name, unquote_name, DefKind, Definition, Model, Usage};

/// Which piece of an element's text a table cell edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSpec {
    /// The declared name.
    Name,
    /// The short name, `<'ID'>`.
    ShortName,
    /// The first doc comment of the body.
    Doc,
    /// A value set in the body: `:>> name = value;`.
    Feature(String),
    /// A field of a metadata annotation: `@Type { field = value; }`.
    Metadata { ty: String, field: String },
}

impl FieldSpec {
    /// Parse `name`, `shortName`, `doc`, `feature:x` or `metadata:T.x`.
    pub fn parse(spec: &str) -> Result<FieldSpec, String> {
        let spec = spec.trim();
        let bad = || {
            format!(
                "unknown field `{spec}` (expected name, shortName, doc, feature:<name> \
                 or metadata:<Type>.<field>)"
            )
        };
        Ok(match spec {
            "name" => FieldSpec::Name,
            "shortName" => FieldSpec::ShortName,
            "doc" => FieldSpec::Doc,
            _ => {
                if let Some(f) = spec.strip_prefix("feature:") {
                    let f = unquote_name(f.trim());
                    if f.is_empty() {
                        return Err(bad());
                    }
                    FieldSpec::Feature(f.to_string())
                } else if let Some(m) = spec.strip_prefix("metadata:") {
                    let (ty, field) = m.rsplit_once('.').ok_or_else(bad)?;
                    let (ty, field) = (unquote_name(ty.trim()), unquote_name(field.trim()));
                    if ty.is_empty() || field.is_empty() {
                        return Err(bad());
                    }
                    FieldSpec::Metadata {
                        ty: ty.to_string(),
                        field: field.to_string(),
                    }
                } else {
                    return Err(bad());
                }
            }
        })
    }
}

impl fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldSpec::Name => f.write_str("name"),
            FieldSpec::ShortName => f.write_str("shortName"),
            FieldSpec::Doc => f.write_str("doc"),
            FieldSpec::Feature(n) => write!(f, "feature:{n}"),
            FieldSpec::Metadata { ty, field } => write!(f, "metadata:{ty}.{field}"),
        }
    }
}

/// How a cell is edited and how its value is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    /// One line of text; a string literal in SysML.
    Text,
    /// Free text over several lines (a doc comment, a rationale).
    LongText,
    /// One enum literal, written `Type::literal`.
    Enum,
    /// A number, written as typed.
    Number,
    /// Several enum literals, written `(Type::a, Type::b)`.
    EnumList,
}

/// The resolved type of a field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FieldType {
    pub kind: Option<ValueKind>,
    /// The enum def that qualifies a literal (`Priority`), if any.
    pub enum_type: Option<String>,
    /// Enum literals: (display name, name as written — `'use'`).
    pub literals: Vec<(String, String)>,
}

impl FieldType {
    fn of(kind: ValueKind) -> Self {
        FieldType {
            kind: Some(kind),
            ..Default::default()
        }
    }

    /// The literal names a person picks from.
    pub fn options(&self) -> Option<Vec<String>> {
        (!self.literals.is_empty()).then(|| self.literals.iter().map(|(d, _)| d.clone()).collect())
    }
}

/// One column's edit metadata in a rendered table.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ColumnEdit {
    /// The field spec, e.g. `feature:priority`.
    pub field: String,
    pub kind: ValueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

impl ColumnEdit {
    pub fn new(spec: &FieldSpec, ty: &FieldType, default: ValueKind) -> Self {
        ColumnEdit {
            field: spec.to_string(),
            kind: ty.kind.unwrap_or(default),
            options: ty.options(),
        }
    }
}

// --- resolution -----------------------------------------------------------------

/// A definition by (simple, possibly quoted) name: the project's first,
/// else the standard library's.
pub fn find_definition(models: &[Model], name: &str) -> Option<Definition> {
    let want = unquote_name(simple_name(name.trim()));
    if want.is_empty() {
        return None;
    }
    models
        .iter()
        .flat_map(|m| &m.definitions)
        .find(|d| unquote_name(&d.name) == want)
        .cloned()
        .or_else(|| stdlib_model_defining(want).and_then(|m| def_in(&m, want)))
}

fn def_in(m: &Model, want: &str) -> Option<Definition> {
    m.definitions
        .iter()
        .find(|d| unquote_name(&d.name) == want)
        .cloned()
}

/// The parsed standard library file that defines `name`, if any. Only
/// the one file is parsed, not the whole library, and the answer is
/// remembered.
fn stdlib_model_defining(name: &str) -> Option<Model> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Model>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(name).cloned()) {
        return hit;
    }
    let found = search_stdlib(name);
    if let Ok(mut c) = cache.lock() {
        c.insert(name.to_string(), found.clone());
    }
    found
}

fn search_stdlib(name: &str) -> Option<Model> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    for (path, text) in crate::stdlib::stdlib_files() {
        let needle = format!("def {name}");
        let found = text.match_indices(&needle).any(|(i, _)| {
            !text[i + needle.len()..]
                .chars()
                .next()
                .is_some_and(is_ident)
        });
        if found {
            let m = crate::parser::parse_file(path, text);
            if def_in(&m, name).is_some() {
                return Some(m);
            }
        }
    }
    None
}

/// Every model to search for a def's members: the project's, plus the
/// standard library file the def came from.
fn members_of(models: &[Model], def: &Definition) -> Vec<Usage> {
    let name = unquote_name(&def.name);
    let in_models: Vec<Usage> = models
        .iter()
        .flat_map(|m| &m.usages)
        .filter(|u| u.parent_def.as_deref().map(unquote_name) == Some(name))
        .cloned()
        .collect();
    if !in_models.is_empty() || models.iter().any(|m| def_in(m, name).is_some()) {
        return in_models;
    }
    stdlib_model_defining(name)
        .map(|m| {
            m.usages
                .into_iter()
                .filter(|u| u.parent_def.as_deref().map(unquote_name) == Some(name))
                .collect()
        })
        .unwrap_or_default()
}

/// The declaration of feature `feature` in `owner_type` or its
/// supertypes (the first one with a type).
pub fn declared_feature(models: &[Model], owner_type: &str, feature: &str) -> Option<Usage> {
    let mut current = find_definition(models, owner_type)?;
    let mut seen = Vec::new();
    loop {
        let name = unquote_name(&current.name).to_string();
        if seen.contains(&name) || seen.len() > 16 {
            return None;
        }
        seen.push(name);
        let hit = members_of(models, &current)
            .into_iter()
            .find(|u| unquote_name(&u.name) == feature && u.type_ref.is_some());
        if hit.is_some() {
            return hit;
        }
        current = find_definition(models, current.super_type.as_deref()?)?;
    }
}

/// The type of feature `feature` of an element typed by `owner_type`.
/// `None` when nothing declares it; a declaration that is not an
/// attribute (a `ref`, a `part`) is not an editable value either.
pub fn feature_type(models: &[Model], owner_type: &str, feature: &str) -> Option<FieldType> {
    let decl = declared_feature(models, owner_type, feature)?;
    if decl.kind != "attribute" {
        return None;
    }
    Some(classify_declaration(models, &decl))
}

/// The type of `field` of metadata def `meta`.
pub fn metadata_field_type(models: &[Model], meta: &str, field: &str) -> Option<FieldType> {
    feature_type(models, meta, field)
}

fn classify_declaration(models: &[Model], decl: &Usage) -> FieldType {
    // The parser keeps `[0..*]` as lower 0 with no upper bound, and `[n]`
    // as lower n, so `[1..*]` reads like `[1]`.
    let many = decl.multiplicity.as_ref().is_some_and(|m| {
        let above_one = |s: &str| s == "*" || s.parse::<u64>().is_ok_and(|n| n > 1);
        match (m.lower.as_deref(), m.upper.as_deref()) {
            (_, Some(u)) => above_one(u),
            (None, None) => true,
            (Some(l), None) => l == "0" || above_one(l),
        }
    });
    let ty = decl.type_ref.as_deref().unwrap_or("");
    classify_type(models, ty, many)
}

const NUMERIC: [&str; 9] = [
    "Real",
    "Integer",
    "Natural",
    "Positive",
    "Rational",
    "Number",
    "Complex",
    "NumericalValue",
    "ScalarQuantityValue",
];

/// What values of type `ty` look like.
pub fn classify_type(models: &[Model], ty: &str, many: bool) -> FieldType {
    let mut current = unquote_name(simple_name(ty)).to_string();
    for _ in 0..16 {
        if current == "String" {
            return FieldType::of(ValueKind::Text);
        }
        if current == "Boolean" {
            return FieldType {
                kind: Some(ValueKind::Enum),
                enum_type: None,
                literals: ["true", "false"]
                    .iter()
                    .map(|s| (s.to_string(), s.to_string()))
                    .collect(),
            };
        }
        if NUMERIC.contains(&current.as_str()) {
            return FieldType::of(ValueKind::Number);
        }
        let Some(def) = find_definition(models, &current) else {
            // ISQ quantity types (`MassValue`, `DurationValue`) are numbers
            // with units.
            if current.ends_with("Value") {
                return FieldType::of(ValueKind::Number);
            }
            return FieldType::default();
        };
        if def.kind == DefKind::Enum {
            return FieldType {
                kind: Some(if many {
                    ValueKind::EnumList
                } else {
                    ValueKind::Enum
                }),
                enum_type: Some(def.name.clone()),
                literals: def
                    .enum_members
                    .iter()
                    .map(|m| (unquote_name(&m.name).to_string(), m.name.clone()))
                    .collect(),
            };
        }
        match def.super_type.as_deref() {
            Some(s) => current = unquote_name(simple_name(s)).to_string(),
            None => return FieldType::default(),
        }
    }
    FieldType::default()
}

/// What a value written in the source says about its type: a string, a
/// number, an enum literal (`Kind::x`, giving the qualifier) or a list of
/// them.
pub fn classify_value_text(models: &[Model], text: &str) -> FieldType {
    let t = text.trim();
    if t.starts_with('"') {
        return FieldType::of(ValueKind::Text);
    }
    if t == "true" || t == "false" {
        return classify_type(models, "Boolean", false);
    }
    let number = t.split('[').next().unwrap_or("").trim();
    if !number.is_empty() && number.parse::<f64>().is_ok() {
        return FieldType::of(ValueKind::Number);
    }
    let (list, first) = match t.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        Some(inner) => (true, inner.split(',').next().unwrap_or("").trim()),
        None => (false, t),
    };
    match first.rsplit_once("::") {
        Some((qualifier, _)) if is_name_path(first) => {
            let mut ty = classify_type(models, qualifier, list);
            if ty.kind.is_none() || ty.enum_type.is_none() {
                ty = FieldType::default();
            }
            ty.kind = Some(if list {
                ValueKind::EnumList
            } else {
                ValueKind::Enum
            });
            // Keep the qualifier as written (`EngineeringBasics::Priority`).
            ty.enum_type = Some(qualifier.to_string());
            ty
        }
        _ => FieldType::default(),
    }
}

/// `A::B::'c'`: name segments only.
fn is_name_path(s: &str) -> bool {
    s.split("::").all(|seg| {
        let seg = seg.trim();
        is_plain_identifier(seg)
            || seg
                .strip_prefix('\'')
                .and_then(|x| x.strip_suffix('\''))
                .is_some_and(|x| !x.is_empty())
    })
}

/// The type of a feature as the project's existing values use it, when no
/// declaration says: the first `:>> feature = ...` written anywhere.
pub fn feature_type_from_values(models: &[Model], feature: &str) -> Option<FieldType> {
    models
        .iter()
        .flat_map(|m| &m.usages)
        .filter(|u| unquote_name(&u.name) == feature)
        .find_map(|u| u.value_expr.as_deref())
        .map(|v| classify_value_text(models, v))
        .filter(|t| t.kind.is_some())
}

/// Same, for a metadata field: the first value given to it in an
/// annotation of that type.
pub fn metadata_type_from_values(models: &[Model], meta: &str, field: &str) -> Option<FieldType> {
    models
        .iter()
        .flat_map(|m| &m.annotations)
        .filter(|a| unquote_name(simple_name(&a.metadata_type)) == meta)
        .find_map(|a| a.values.iter().find(|(k, _)| k == field).map(|(_, v)| v))
        .map(|v| classify_value_text(models, v))
        .filter(|t| t.kind.is_some())
}

// --- names ----------------------------------------------------------------------

/// SysML v2 reserved words (KerML/SysML textual notation); a name that
/// is one must be written quoted.
const RESERVED: &[&str] = &[
    "about",
    "abstract",
    "accept",
    "action",
    "actor",
    "after",
    "alias",
    "all",
    "allocate",
    "allocation",
    "analysis",
    "and",
    "as",
    "assert",
    "assign",
    "assume",
    "at",
    "attribute",
    "bind",
    "binding",
    "by",
    "calc",
    "case",
    "comment",
    "concern",
    "connect",
    "connection",
    "constant",
    "constraint",
    "crosses",
    "decide",
    "def",
    "default",
    "defined",
    "dependency",
    "derived",
    "do",
    "doc",
    "else",
    "end",
    "entry",
    "enum",
    "event",
    "exhibit",
    "exit",
    "expose",
    "false",
    "filter",
    "first",
    "flow",
    "for",
    "fork",
    "frame",
    "from",
    "hastype",
    "if",
    "implies",
    "import",
    "in",
    "include",
    "individual",
    "inout",
    "interface",
    "istype",
    "item",
    "join",
    "language",
    "library",
    "locale",
    "loop",
    "merge",
    "message",
    "meta",
    "metadata",
    "nonunique",
    "not",
    "null",
    "objective",
    "occurrence",
    "of",
    "or",
    "ordered",
    "out",
    "package",
    "parallel",
    "part",
    "perform",
    "port",
    "private",
    "protected",
    "public",
    "redefines",
    "ref",
    "references",
    "render",
    "rendering",
    "rep",
    "require",
    "requirement",
    "return",
    "satisfy",
    "send",
    "snapshot",
    "specializes",
    "stakeholder",
    "standard",
    "state",
    "subject",
    "subsets",
    "succession",
    "terminate",
    "then",
    "timeslice",
    "to",
    "transition",
    "true",
    "until",
    "use",
    "variant",
    "variation",
    "verification",
    "verify",
    "via",
    "view",
    "viewpoint",
    "when",
    "while",
    "xor",
];

/// `abc_1`: a name that needs no quotes (reserved words aside).
pub fn is_plain_identifier(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// A name as SysML text: bare when it can be, else `'quoted'` with `'`
/// and `\` escaped.
pub fn format_name(name: &str) -> String {
    if is_plain_identifier(name) && !RESERVED.contains(&name) {
        name.to_string()
    } else {
        quote_name(name)
    }
}

/// A name always quoted: `'SYS-001'`.
pub fn quote_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('\'');
    for c in name.chars() {
        if c == '\'' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// A SysML string literal for `text`.
pub fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    const LIB: &str = "package L {
        enum def Priority { enum high; enum medium; enum low; }
        enum def Method { enum test; enum 'use'; }
        attribute def Rating :> Integer;
        requirement def Base {
            attribute priority : Priority[0..1];
            attribute methods : Method[0..*];
            attribute score : Rating;
            attribute note : String[0..1];
            attribute ok : Boolean;
            ref other : Base[0..1];
        }
        requirement def Derived :> Base;
    }";

    fn models() -> Vec<Model> {
        vec![parse_file("l.sysml", LIB)]
    }

    #[test]
    fn specs_round_trip() {
        for s in [
            "name",
            "shortName",
            "doc",
            "feature:priority",
            "metadata:StatusInfo.status",
        ] {
            assert_eq!(FieldSpec::parse(s).unwrap().to_string(), s);
        }
        assert!(FieldSpec::parse("feature:").is_err());
        assert!(FieldSpec::parse("metadata:StatusInfo").is_err());
        assert!(FieldSpec::parse("title").is_err());
    }

    #[test]
    fn feature_types_follow_the_supertypes() {
        let m = models();
        let p = feature_type(&m, "Derived", "priority").unwrap();
        assert_eq!(p.kind, Some(ValueKind::Enum));
        assert_eq!(p.enum_type.as_deref(), Some("Priority"));
        assert_eq!(p.options().unwrap(), ["high", "medium", "low"]);
        let l = feature_type(&m, "Derived", "methods").unwrap();
        assert_eq!(l.kind, Some(ValueKind::EnumList));
        assert_eq!(l.literals[1], ("use".into(), "'use'".into()));
        assert_eq!(
            feature_type(&m, "Derived", "score").unwrap().kind,
            Some(ValueKind::Number)
        );
        assert_eq!(
            feature_type(&m, "Derived", "note").unwrap().kind,
            Some(ValueKind::Text)
        );
        assert_eq!(
            feature_type(&m, "Derived", "ok")
                .unwrap()
                .options()
                .unwrap(),
            ["true", "false"]
        );
        assert!(feature_type(&m, "Derived", "other").is_none());
        assert!(feature_type(&m, "Derived", "missing").is_none());
    }

    #[test]
    fn standard_metadata_resolves_from_the_library() {
        let t = metadata_field_type(&[], "StatusInfo", "status").unwrap();
        assert_eq!(t.kind, Some(ValueKind::Enum));
        assert_eq!(t.enum_type.as_deref(), Some("StatusKind"));
        assert_eq!(
            t.options().unwrap(),
            ["open", "tbd", "tbr", "tbc", "done", "closed"]
        );
        assert_eq!(
            metadata_field_type(&[], "StatusInfo", "owner")
                .unwrap()
                .kind,
            Some(ValueKind::Text)
        );
    }

    #[test]
    fn values_reveal_their_type() {
        let m = models();
        let t = classify_value_text(&m, "(Method::test, Method::'use')");
        assert_eq!(t.kind, Some(ValueKind::EnumList));
        assert_eq!(t.enum_type.as_deref(), Some("Method"));
        assert_eq!(t.literals.len(), 2);
        assert_eq!(
            classify_value_text(&m, "X::y").enum_type.as_deref(),
            Some("X")
        );
        assert_eq!(
            classify_value_text(&m, "3.5 [kg]").kind,
            Some(ValueKind::Number)
        );
        assert_eq!(classify_value_text(&m, "\"a\"").kind, Some(ValueKind::Text));
        assert_eq!(classify_value_text(&m, "a.b + 1").kind, None);
    }

    #[test]
    fn names_are_quoted_when_needed() {
        assert_eq!(format_name("abc"), "abc");
        assert_eq!(format_name("use"), "'use'");
        assert_eq!(format_name("Flight range"), "'Flight range'");
        assert_eq!(format_name("it's"), "'it\\'s'");
        assert_eq!(string_literal("a \"b\"\\"), "\"a \\\"b\\\"\\\\\"");
    }
}
