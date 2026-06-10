//! Qualified name representation for SysML v2 model elements.
//!
//! A qualified name is a `::` separated path such as `Package::SubPackage::Element`.
//! It uniquely identifies an element within a model namespace hierarchy.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A qualified name consisting of one or more segments separated by `::`.
///
/// # Examples
///
/// ```
/// use sysml_core::qualified_name::QualifiedName;
///
/// let qn = QualifiedName::parse("Vehicle::Chassis::Wheel");
/// assert_eq!(qn.depth(), 3);
/// assert_eq!(qn.leaf(), Some("Wheel"));
/// assert_eq!(qn.to_string(), "Vehicle::Chassis::Wheel");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct QualifiedName {
    segments: Vec<String>,
}

impl QualifiedName {
    /// Create a qualified name from a vector of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    pub fn new(segments: Vec<String>) -> Self {
        assert!(
            !segments.is_empty(),
            "QualifiedName must have at least one segment"
        );
        Self { segments }
    }

    /// Create a qualified name from an iterator of string-like segments.
    ///
    /// # Panics
    ///
    /// Panics if the iterator yields no elements.
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        Self::new(segments)
    }

    /// Parse a `::` separated string into a qualified name.
    ///
    /// Whitespace around each segment is trimmed. Empty segments are ignored
    /// so that inputs like `"A::::B"` or `"::A"` degrade gracefully.
    ///
    /// # Panics
    ///
    /// Panics if the input contains no non-empty segments.
    pub fn parse(input: &str) -> Self {
        let segments: Vec<String> = input
            .split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        Self::new(segments)
    }

    /// The individual segments of this qualified name.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The number of segments.
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// The final (leaf) segment, or `None` if somehow empty (should not happen
    /// via public constructors).
    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// The parent qualified name (everything except the last segment), or
    /// `None` if this name has only one segment.
    pub fn parent(&self) -> Option<QualifiedName> {
        if self.segments.len() <= 1 {
            return None;
        }
        Some(QualifiedName {
            segments: self.segments[..self.segments.len() - 1].to_vec(),
        })
    }

    /// Returns `true` if this name is a direct or transitive child of `other`.
    ///
    /// `A::B::C` is a child of `A::B` and of `A`, but not of `A::B::C` itself.
    pub fn is_child_of(&self, other: &QualifiedName) -> bool {
        self.segments.len() > other.segments.len()
            && self.segments[..other.segments.len()] == other.segments[..]
    }

    /// Append a segment, returning a new qualified name.
    pub fn push(&self, segment: impl Into<String>) -> QualifiedName {
        let mut segments = self.segments.clone();
        segments.push(segment.into());
        QualifiedName { segments }
    }

    /// Concatenate two qualified names, returning a new one.
    pub fn join(&self, other: &QualifiedName) -> QualifiedName {
        let mut segments = self.segments.clone();
        segments.extend_from_slice(&other.segments);
        QualifiedName { segments }
    }

    /// Convert to a path-safe string by replacing `::` with `__`.
    pub fn to_path_safe(&self) -> String {
        self.segments.join("__")
    }

    /// Match against an import pattern with wildcard support.
    ///
    /// - `Package::*` matches any **direct** child of `Package` (depth +1),
    ///   e.g. `Package::Element` but not `Package::Sub::Element`.
    /// - `Package::**` matches any **recursive** descendant of `Package`,
    ///   e.g. both `Package::Element` and `Package::Sub::Element`.
    /// - A pattern without wildcards matches only an exact qualified name.
    ///
    /// The wildcard must appear as the final segment.
    pub fn matches_wildcard(&self, pattern: &str) -> bool {
        // Split the pattern and trim.
        let parts: Vec<&str> = pattern
            .split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return false;
        }

        let last = *parts.last().unwrap();

        match last {
            "**" => {
                let prefix = &parts[..parts.len() - 1];
                // Must be strictly longer than the prefix (i.e. an actual descendant).
                self.segments.len() > prefix.len()
                    && self.segments.iter().zip(prefix.iter()).all(|(a, b)| a == b)
            }
            "*" => {
                let prefix = &parts[..parts.len() - 1];
                // Exactly one level deeper than the prefix.
                self.segments.len() == prefix.len() + 1
                    && self.segments.iter().zip(prefix.iter()).all(|(a, b)| a == b)
            }
            _ => {
                // Exact match — no wildcard.
                self.segments.len() == parts.len()
                    && self
                        .segments
                        .iter()
                        .zip(parts.iter())
                        .all(|(a, b)| a.as_str() == *b)
            }
        }
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined = self.segments.join("::");
        f.write_str(&joined)
    }
}

impl FromStr for QualifiedName {
    type Err = QualifiedNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segments: Vec<String> = s
            .split("::")
            .map(str::trim)
            .filter(|seg| !seg.is_empty())
            .map(String::from)
            .collect();

        if segments.is_empty() {
            return Err(QualifiedNameError::Empty);
        }
        Ok(QualifiedName { segments })
    }
}

/// Error returned when parsing an empty or whitespace-only string into a
/// [`QualifiedName`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualifiedNameError {
    /// The input string contained no valid segments.
    Empty,
}

impl fmt::Display for QualifiedNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QualifiedNameError::Empty => {
                write!(f, "qualified name must have at least one segment")
            }
        }
    }
}

impl std::error::Error for QualifiedNameError {}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------ //
    //  Construction
    // ------------------------------------------------------------------ //

    #[test]
    fn new_single_segment() {
        let qn = QualifiedName::new(vec!["Element".into()]);
        assert_eq!(qn.segments(), &["Element"]);
    }

    #[test]
    fn new_multiple_segments() {
        let qn = QualifiedName::new(vec!["A".into(), "B".into(), "C".into()]);
        assert_eq!(qn.segments(), &["A", "B", "C"]);
    }

    #[test]
    #[should_panic(expected = "at least one segment")]
    fn new_empty_panics() {
        QualifiedName::new(vec![]);
    }

    #[test]
    fn from_segments_str_refs() {
        let qn = QualifiedName::from_segments(["X", "Y"]);
        assert_eq!(qn.to_string(), "X::Y");
    }

    #[test]
    fn from_segments_strings() {
        let qn = QualifiedName::from_segments(vec!["A".to_string(), "B".to_string()]);
        assert_eq!(qn.to_string(), "A::B");
    }

    #[test]
    #[should_panic(expected = "at least one segment")]
    fn from_segments_empty_panics() {
        QualifiedName::from_segments(Vec::<String>::new());
    }

    // ------------------------------------------------------------------ //
    //  Parsing
    // ------------------------------------------------------------------ //

    #[test]
    fn parse_simple() {
        let qn = QualifiedName::parse("Vehicle::Chassis::Wheel");
        assert_eq!(qn.segments(), &["Vehicle", "Chassis", "Wheel"]);
    }

    #[test]
    fn parse_single() {
        let qn = QualifiedName::parse("Root");
        assert_eq!(qn.segments(), &["Root"]);
    }

    #[test]
    fn parse_trims_whitespace() {
        let qn = QualifiedName::parse(" A :: B :: C ");
        assert_eq!(qn.segments(), &["A", "B", "C"]);
    }

    #[test]
    fn parse_ignores_empty_segments() {
        let qn = QualifiedName::parse("A::::B");
        assert_eq!(qn.segments(), &["A", "B"]);
    }

    #[test]
    fn parse_leading_separator() {
        let qn = QualifiedName::parse("::A::B");
        assert_eq!(qn.segments(), &["A", "B"]);
    }

    #[test]
    #[should_panic(expected = "at least one segment")]
    fn parse_empty_panics() {
        QualifiedName::parse("");
    }

    #[test]
    #[should_panic(expected = "at least one segment")]
    fn parse_only_separators_panics() {
        QualifiedName::parse("::::");
    }

    // ------------------------------------------------------------------ //
    //  FromStr
    // ------------------------------------------------------------------ //

    #[test]
    fn from_str_ok() {
        let qn: QualifiedName = "Pkg::Elem".parse().unwrap();
        assert_eq!(qn.segments(), &["Pkg", "Elem"]);
    }

    #[test]
    fn from_str_empty_err() {
        let result: Result<QualifiedName, _> = "".parse();
        assert_eq!(result.unwrap_err(), QualifiedNameError::Empty);
    }

    #[test]
    fn from_str_only_separators_err() {
        let result: Result<QualifiedName, _> = "::::".parse();
        assert_eq!(result.unwrap_err(), QualifiedNameError::Empty);
    }

    // ------------------------------------------------------------------ //
    //  Accessors
    // ------------------------------------------------------------------ //

    #[test]
    fn depth() {
        assert_eq!(QualifiedName::parse("A").depth(), 1);
        assert_eq!(QualifiedName::parse("A::B::C").depth(), 3);
    }

    #[test]
    fn leaf() {
        assert_eq!(QualifiedName::parse("A::B::C").leaf(), Some("C"));
        assert_eq!(QualifiedName::parse("Root").leaf(), Some("Root"));
    }

    #[test]
    fn parent_exists() {
        let qn = QualifiedName::parse("A::B::C");
        let parent = qn.parent().unwrap();
        assert_eq!(parent.to_string(), "A::B");
    }

    #[test]
    fn parent_of_single_is_none() {
        assert!(QualifiedName::parse("Root").parent().is_none());
    }

    #[test]
    fn parent_chain() {
        let qn = QualifiedName::parse("A::B::C");
        let p1 = qn.parent().unwrap();
        assert_eq!(p1.to_string(), "A::B");
        let p2 = p1.parent().unwrap();
        assert_eq!(p2.to_string(), "A");
        assert!(p2.parent().is_none());
    }

    // ------------------------------------------------------------------ //
    //  is_child_of
    // ------------------------------------------------------------------ //

    #[test]
    fn is_child_of_direct_parent() {
        let child = QualifiedName::parse("A::B::C");
        let parent = QualifiedName::parse("A::B");
        assert!(child.is_child_of(&parent));
    }

    #[test]
    fn is_child_of_transitive_ancestor() {
        let child = QualifiedName::parse("A::B::C::D");
        let ancestor = QualifiedName::parse("A");
        assert!(child.is_child_of(&ancestor));
    }

    #[test]
    fn is_not_child_of_self() {
        let qn = QualifiedName::parse("A::B");
        assert!(!qn.is_child_of(&qn));
    }

    #[test]
    fn is_not_child_of_unrelated() {
        let a = QualifiedName::parse("A::B");
        let b = QualifiedName::parse("X::Y");
        assert!(!a.is_child_of(&b));
    }

    #[test]
    fn is_not_child_of_longer() {
        let short = QualifiedName::parse("A");
        let long = QualifiedName::parse("A::B");
        assert!(!short.is_child_of(&long));
    }

    // ------------------------------------------------------------------ //
    //  push / join
    // ------------------------------------------------------------------ //

    #[test]
    fn push_segment() {
        let qn = QualifiedName::parse("A::B");
        let extended = qn.push("C");
        assert_eq!(extended.to_string(), "A::B::C");
        // Original unchanged.
        assert_eq!(qn.to_string(), "A::B");
    }

    #[test]
    fn join_two() {
        let a = QualifiedName::parse("A::B");
        let b = QualifiedName::parse("C::D");
        let joined = a.join(&b);
        assert_eq!(joined.to_string(), "A::B::C::D");
    }

    // ------------------------------------------------------------------ //
    //  Display / to_path_safe
    // ------------------------------------------------------------------ //

    #[test]
    fn display() {
        let qn = QualifiedName::parse("Vehicle::Chassis::Wheel");
        assert_eq!(format!("{qn}"), "Vehicle::Chassis::Wheel");
    }

    #[test]
    fn display_single() {
        let qn = QualifiedName::parse("Root");
        assert_eq!(format!("{qn}"), "Root");
    }

    #[test]
    fn to_path_safe() {
        let qn = QualifiedName::parse("A::B::C");
        assert_eq!(qn.to_path_safe(), "A__B__C");
    }

    #[test]
    fn to_path_safe_single() {
        let qn = QualifiedName::parse("Root");
        assert_eq!(qn.to_path_safe(), "Root");
    }

    // ------------------------------------------------------------------ //
    //  Trait impls: Hash, Eq, Ord
    // ------------------------------------------------------------------ //

    #[test]
    fn equality() {
        let a = QualifiedName::parse("A::B");
        let b = QualifiedName::parse("A::B");
        assert_eq!(a, b);
    }

    #[test]
    fn inequality() {
        let a = QualifiedName::parse("A::B");
        let b = QualifiedName::parse("A::C");
        assert_ne!(a, b);
    }

    #[test]
    fn ordering() {
        let a = QualifiedName::parse("A::B");
        let b = QualifiedName::parse("A::C");
        let c = QualifiedName::parse("B::A");
        let mut v = vec![c.clone(), a.clone(), b.clone()];
        v.sort();
        assert_eq!(v, vec![a, b, c]);
    }

    #[test]
    fn hash_consistency() {
        use std::collections::HashSet;
        let a = QualifiedName::parse("X::Y");
        let b = QualifiedName::parse("X::Y");
        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        assert_eq!(set.len(), 1);
    }

    // ------------------------------------------------------------------ //
    //  Serde round-trip
    // ------------------------------------------------------------------ //

    #[test]
    fn serde_round_trip() {
        let qn = QualifiedName::parse("A::B::C");
        let json = serde_json::to_string(&qn).unwrap();
        let back: QualifiedName = serde_json::from_str(&json).unwrap();
        assert_eq!(qn, back);
    }

    // ------------------------------------------------------------------ //
    //  Wildcard matching
    // ------------------------------------------------------------------ //

    #[test]
    fn wildcard_star_matches_direct_child() {
        let qn = QualifiedName::parse("Package::Element");
        assert!(qn.matches_wildcard("Package::*"));
    }

    #[test]
    fn wildcard_star_does_not_match_deeper() {
        let qn = QualifiedName::parse("Package::Sub::Element");
        assert!(!qn.matches_wildcard("Package::*"));
    }

    #[test]
    fn wildcard_star_does_not_match_prefix_itself() {
        let qn = QualifiedName::parse("Package");
        assert!(!qn.matches_wildcard("Package::*"));
    }

    #[test]
    fn wildcard_globstar_matches_direct_child() {
        let qn = QualifiedName::parse("Package::Element");
        assert!(qn.matches_wildcard("Package::**"));
    }

    #[test]
    fn wildcard_globstar_matches_deep_child() {
        let qn = QualifiedName::parse("Package::Sub::Deep::Element");
        assert!(qn.matches_wildcard("Package::**"));
    }

    #[test]
    fn wildcard_globstar_does_not_match_prefix_itself() {
        let qn = QualifiedName::parse("Package");
        assert!(!qn.matches_wildcard("Package::**"));
    }

    #[test]
    fn wildcard_exact_match() {
        let qn = QualifiedName::parse("A::B::C");
        assert!(qn.matches_wildcard("A::B::C"));
    }

    #[test]
    fn wildcard_exact_no_match() {
        let qn = QualifiedName::parse("A::B::C");
        assert!(!qn.matches_wildcard("A::B::D"));
    }

    #[test]
    fn wildcard_exact_different_depth() {
        let qn = QualifiedName::parse("A::B");
        assert!(!qn.matches_wildcard("A::B::C"));
    }

    #[test]
    fn wildcard_empty_pattern() {
        let qn = QualifiedName::parse("A");
        assert!(!qn.matches_wildcard(""));
    }

    #[test]
    fn wildcard_star_at_root() {
        let qn = QualifiedName::parse("TopLevel");
        assert!(qn.matches_wildcard("*"));
    }

    #[test]
    fn wildcard_globstar_at_root() {
        let qn = QualifiedName::parse("Any::Depth::At::All");
        assert!(qn.matches_wildcard("**"));
    }

    #[test]
    fn wildcard_globstar_at_root_single_segment() {
        let qn = QualifiedName::parse("Single");
        assert!(qn.matches_wildcard("**"));
    }

    #[test]
    fn wildcard_star_wrong_prefix() {
        let qn = QualifiedName::parse("Other::Element");
        assert!(!qn.matches_wildcard("Package::*"));
    }

    #[test]
    fn wildcard_pattern_with_whitespace() {
        let qn = QualifiedName::parse("A::B");
        assert!(qn.matches_wildcard(" A :: * "));
    }

    // ------------------------------------------------------------------ //
    //  QualifiedNameError Display
    // ------------------------------------------------------------------ //

    #[test]
    fn error_display() {
        let err = QualifiedNameError::Empty;
        assert_eq!(
            err.to_string(),
            "qualified name must have at least one segment"
        );
    }
}
