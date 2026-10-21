//! Concrete syntax of SysML v2 and KerML.
//!
//! This crate owns the binding to the `tree-sitter-sysml` grammar (compiled
//! from C in the build script, also for `wasm32-unknown-unknown`) and the
//! embedded sources of the OMG standard library, so that every other crate —
//! the summary model in `sysml-core`, the semantic model in
//! `sysml-semantic`, the language server and the browser engine — parses
//! with exactly the same grammar and sees the same library text.

pub use tree_sitter;

use tree_sitter::{Language, Parser, Tree};

extern "C" {
    fn tree_sitter_sysml() -> Language;
}

/// The tree-sitter language for SysML v2 / KerML textual notation.
pub fn language() -> Language {
    // SAFETY: `tree_sitter_sysml` is the generated, side-effect free
    // constructor of the statically linked parser.
    unsafe { tree_sitter_sysml() }
}

/// A parser configured with [`language`].
pub fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&language())
        .expect("tree-sitter-sysml is ABI compatible with the tree-sitter crate");
    parser
}

/// Parse `text` into a concrete syntax tree.
pub fn parse(text: &str) -> Tree {
    parser()
        .parse(text, None)
        .expect("parsing without a timeout or cancellation flag always yields a tree")
}

/// The grammar's syntax highlighting query (`queries/highlights.scm`).
pub const HIGHLIGHTS_QUERY: &str = include_str!(concat!(env!("OUT_DIR"), "/highlights.scm"));

include!(concat!(env!("OUT_DIR"), "/stdlib_files.rs"));

/// The OMG standard library sources (`sysml-v2-release/sysml.library`), as
/// `(path relative to the library root, text)` pairs sorted by path. Empty
/// when the release submodule was not available at build time.
pub fn stdlib_files() -> &'static [(&'static str, &'static str)] {
    STDLIB_FILES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_package() {
        let tree = parse("package P { part def A; }");
        assert_eq!(tree.root_node().kind(), "source_file");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn highlight_query_is_embedded() {
        assert!(HIGHLIGHTS_QUERY.contains("keyword"));
    }
}
