//! Names, escaped names and qualified names (KerML 1.0, 8.2.2.3 and 8.3.2.1).

use std::borrow::Cow;

/// Whether `name` has the lexical form of a KerML `BASIC_NAME`:
/// `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_basic_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `escapedName()`: the name as-is if it is a basic name, otherwise an
/// `UNRESTRICTED_NAME` — surrounded by single quotes with special characters
/// escaped (KerML 8.2.2.3, Table 4).
pub fn escape_name(name: &str) -> Cow<'_, str> {
    if is_basic_name(name) {
        return Cow::Borrowed(name);
    }
    let mut out = String::with_capacity(name.len() + 2);
    out.push('\'');
    for c in name.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('\'');
    Cow::Owned(out)
}

/// Inverse of [`escape_name`]: strip the quotes of an unrestricted name and
/// resolve its escape sequences. Basic names are returned unchanged.
pub fn unescape_name(text: &str) -> Cow<'_, str> {
    let Some(inner) = text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    else {
        return Cow::Borrowed(text);
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\'') => out.push('\''),
            Some('"') => out.push('"'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    Cow::Owned(out)
}

/// Where an element's owning namespace sits, for deriving `qualifiedName`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwningNamespace<'a> {
    /// The element has no owning namespace.
    None,
    /// The owning namespace is a root namespace (it has no owner).
    Root,
    /// The owning namespace is nested, with this qualified name (or none).
    Nested(Option<&'a str>),
}

/// `qualifiedName` (KerML 8.3.2.1.2, `deriveElementQualifiedName`).
///
/// `escaped` is the element's `escapedName()`. `first_with_name` is false
/// when an earlier owned member of the same namespace has the same non-null
/// name, in which case the qualified name is null.
pub fn qualified_name(
    owner: OwningNamespace<'_>,
    escaped: Option<&str>,
    first_with_name: bool,
) -> Option<String> {
    if !first_with_name {
        return None;
    }
    match owner {
        OwningNamespace::None => None,
        OwningNamespace::Root => escaped.map(str::to_owned),
        OwningNamespace::Nested(qualifier) => Some(format!("{}::{}", qualifier?, escaped?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_names() {
        assert!(is_basic_name("Parts"));
        assert!(is_basic_name("_x1"));
        assert!(!is_basic_name("1x"));
        assert!(!is_basic_name(""));
        assert!(!is_basic_name("Quantities and Units"));
        assert!(!is_basic_name("°C"));
    }

    #[test]
    fn escaping_round_trips() {
        for name in [
            "Parts",
            "Quantities and Units",
            "it's",
            "a\\b",
            "tab\there",
            "°C",
        ] {
            assert_eq!(unescape_name(&escape_name(name)), name);
        }
        assert_eq!(escape_name("it's"), "'it\\'s'");
        assert_eq!(escape_name("ISQ base"), "'ISQ base'");
    }

    #[test]
    fn qualified_names() {
        assert_eq!(
            qualified_name(OwningNamespace::Root, Some("Parts"), true).as_deref(),
            Some("Parts")
        );
        assert_eq!(
            qualified_name(OwningNamespace::Nested(Some("Parts")), Some("Part"), true).as_deref(),
            Some("Parts::Part")
        );
        assert_eq!(
            qualified_name(OwningNamespace::Nested(None), Some("x"), true),
            None
        );
        assert_eq!(
            qualified_name(OwningNamespace::Nested(Some("P")), None, true),
            None
        );
        assert_eq!(
            qualified_name(OwningNamespace::Nested(Some("P")), Some("x"), false),
            None
        );
        assert_eq!(qualified_name(OwningNamespace::None, Some("x"), true), None);
    }
}
