//! A small lexer for SysML v2 / KerML text.
//!
//! The tree-sitter model records whole-element spans; editor features need
//! the exact position of every name. The lexer yields identifier, quoted
//! name, number, string and punctuation tokens with byte offsets, and keeps
//! comments aside so names in comments and strings are never mistaken for
//! references.

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    /// A basic name: `Battery`, `_x1`.
    Ident,
    /// A reserved word: `part`, `def`, `import`.
    Keyword,
    /// An unrestricted name: `'Battery pack'`. `closed` is false for an
    /// unterminated one (still being typed).
    Quoted {
        closed: bool,
    },
    Number,
    /// A string literal `"..."`.
    Str,
    /// Punctuation such as `::`, `:>`, `.`, `{`.
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokKind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    /// True for a name: an identifier or an unrestricted (quoted) name.
    pub fn is_name(&self) -> bool {
        matches!(self.kind, TokKind::Ident | TokKind::Quoted { .. })
    }

    pub fn is_punct(&self, source: &str, p: &str) -> bool {
        self.kind == TokKind::Punct && self.text(source) == p
    }

    pub fn is_keyword(&self, source: &str, k: &str) -> bool {
        self.kind == TokKind::Keyword && self.text(source) == k
    }
}

/// A comment: `/* ... */` (block) or `// ...` (line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    pub start: usize,
    pub end: usize,
    pub block: bool,
}

/// The tokens and comments of one source text.
#[derive(Debug, Clone, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub comments: Vec<Comment>,
}

impl Lexed {
    /// Index of the first token that ends after `offset` — the token at or
    /// after the offset.
    pub fn token_after(&self, offset: usize) -> usize {
        self.tokens.partition_point(|t| t.end <= offset)
    }

    /// The comment containing `offset` (strictly inside it).
    pub fn comment_at(&self, offset: usize) -> Option<&Comment> {
        let i = self.comments.partition_point(|c| c.end < offset);
        self.comments
            .get(i)
            .filter(|c| c.start < offset && offset <= c.end)
            .filter(|c| offset < c.end || !c.block)
    }
}

/// SysML v2 and KerML reserved words.
pub const KEYWORDS: &[&str] = &[
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
    "assoc",
    "assume",
    "at",
    "attribute",
    "behavior",
    "bind",
    "binding",
    "by",
    "calc",
    "case",
    "chains",
    "class",
    "classifier",
    "comment",
    "composite",
    "concern",
    "conjugate",
    "conjugates",
    "conjugation",
    "connect",
    "connection",
    "connector",
    "const",
    "constant",
    "constraint",
    "crosses",
    "datatype",
    "decide",
    "def",
    "default",
    "defined",
    "dependency",
    "derived",
    "differences",
    "disjoining",
    "disjoint",
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
    "expr",
    "false",
    "feature",
    "featured",
    "featuring",
    "filter",
    "first",
    "flow",
    "for",
    "fork",
    "frame",
    "from",
    "function",
    "hastype",
    "if",
    "implies",
    "import",
    "in",
    "include",
    "individual",
    "inout",
    "interaction",
    "interface",
    "intersects",
    "inv",
    "inverse",
    "inverting",
    "istype",
    "item",
    "join",
    "language",
    "library",
    "locale",
    "loop",
    "member",
    "merge",
    "message",
    "meta",
    "metaclass",
    "metadata",
    "multiplicity",
    "namespace",
    "new",
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
    "portion",
    "predicate",
    "private",
    "protected",
    "public",
    "redefines",
    "redefinition",
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
    "specialization",
    "specializes",
    "stakeholder",
    "standard",
    "state",
    "step",
    "struct",
    "subclassifier",
    "subject",
    "subset",
    "subsets",
    "subtype",
    "succession",
    "terminate",
    "then",
    "timeslice",
    "to",
    "transition",
    "true",
    "type",
    "typed",
    "typing",
    "unions",
    "until",
    "use",
    "var",
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

pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.binary_search(&word).is_ok()
}

/// Multi-character punctuation, longest first.
const PUNCT: &[&str] = &[
    "===", "!==", "::>", ":>>", "::", ":>", ":=", "..", "->", "==", "!=", "<=", ">=", "**",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True when `name` is a basic name that needs no quotes.
pub fn is_basic_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !is_keyword(name)
}

/// The text to write for `name`: as is when it is a basic name or already
/// quoted, otherwise as an unrestricted name (`'Battery pack'`).
pub fn quote_name(name: &str) -> String {
    if is_basic_name(name) || (name.len() >= 2 && name.starts_with('\'') && name.ends_with('\'')) {
        return name.to_owned();
    }
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

/// Strip the quotes (and escapes) of an unrestricted name.
pub fn unquote(name: &str) -> String {
    match name.strip_prefix('\'') {
        Some(rest) => {
            let inner = rest.strip_suffix('\'').unwrap_or(rest);
            let mut out = String::with_capacity(inner.len());
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
        None => name.to_owned(),
    }
}

/// Tokenize `source`.
pub fn lex(source: &str) -> Lexed {
    let bytes = source.as_bytes();
    let mut out = Lexed::default();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if source[i..].starts_with("//") {
            let end = source[i..].find('\n').map_or(source.len(), |n| i + n);
            out.comments.push(Comment {
                start,
                end,
                block: false,
            });
            i = end;
            continue;
        }
        if source[i..].starts_with("/*") {
            let end = source[i + 2..]
                .find("*/")
                .map_or(source.len(), |n| i + 2 + n + 2);
            out.comments.push(Comment {
                start,
                end,
                block: true,
            });
            i = end;
            continue;
        }
        let c = source[i..].chars().next().unwrap_or('\0');
        let (kind, end) = if b == b'\'' || b == b'"' {
            let mut j = i + 1;
            let mut closed = false;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    x if x == b => {
                        j += 1;
                        closed = true;
                        break;
                    }
                    b'\n' if b == b'\'' => break,
                    _ => j += 1,
                }
            }
            let j = j.min(bytes.len());
            if b == b'"' {
                (TokKind::Str, j)
            } else {
                (TokKind::Quoted { closed }, j)
            }
        } else if is_ident_start(c) {
            let end = source[i..]
                .char_indices()
                .find(|&(_, ch)| !is_ident_char(ch))
                .map_or(source.len(), |(n, _)| i + n);
            let kind = if is_keyword(&source[i..end]) {
                TokKind::Keyword
            } else {
                TokKind::Ident
            };
            (kind, end)
        } else if b.is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() {
                let x = bytes[j];
                let fraction = x == b'.' && bytes.get(j + 1).is_some_and(u8::is_ascii_digit);
                if x.is_ascii_digit() || x == b'_' || fraction {
                    j += 1;
                } else if (x == b'e' || x == b'E')
                    && (bytes.get(j + 1).is_some_and(u8::is_ascii_digit)
                        || (matches!(bytes.get(j + 1), Some(b'+' | b'-'))
                            && bytes.get(j + 2).is_some_and(u8::is_ascii_digit)))
                {
                    j += 2;
                } else {
                    break;
                }
            }
            (TokKind::Number, j)
        } else {
            let len = PUNCT
                .iter()
                .find(|p| source[i..].starts_with(*p))
                .map_or(c.len_utf8(), |p| p.len());
            (TokKind::Punct, i + len)
        };
        out.tokens.push(Token { kind, start, end });
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(source: &str) -> Vec<&str> {
        lex(source).tokens.iter().map(|t| t.text(source)).collect()
    }

    #[test]
    fn keywords_are_sorted() {
        let mut sorted = KEYWORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, KEYWORDS);
    }

    #[test]
    fn tokens_skip_comments_and_keep_names() {
        let source = "part def 'Battery pack' :> Component { doc /* Battery */ // x\n attribute m :>> ISQ::mass = 1.5e3 [kg]; }";
        assert_eq!(
            texts(source),
            [
                "part",
                "def",
                "'Battery pack'",
                ":>",
                "Component",
                "{",
                "doc",
                "attribute",
                "m",
                ":>>",
                "ISQ",
                "::",
                "mass",
                "=",
                "1.5e3",
                "[",
                "kg",
                "]",
                ";",
                "}"
            ]
        );
        let lexed = lex(source);
        assert_eq!(lexed.comments.len(), 2);
        assert_eq!(lexed.tokens[2].kind, TokKind::Quoted { closed: true });
        assert_eq!(lexed.tokens[0].kind, TokKind::Keyword);
        assert!(lexed
            .comment_at(source.find("Battery */").unwrap())
            .is_some());
    }

    #[test]
    fn unterminated_quoted_name_stops_at_line_end() {
        let lexed = lex("part 'Batt\npart x;");
        assert_eq!(lexed.tokens[1].kind, TokKind::Quoted { closed: false });
        assert_eq!(lexed.tokens.len(), 5);
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_name("Battery"), "Battery");
        assert_eq!(quote_name("Battery pack"), "'Battery pack'");
        assert_eq!(quote_name("part"), "'part'");
        assert_eq!(quote_name("it's"), "'it\\'s'");
        assert_eq!(unquote("'it\\'s'"), "it's");
        assert_eq!(unquote("plain"), "plain");
    }
}
