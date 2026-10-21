//! Print the concrete syntax tree of a file (or stdin), for grammar work.
//!
//!     cargo run -p sysml-syntax --example cst -- path/to/file.sysml

use std::io::Read;

fn main() {
    let text = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(path).expect("readable file"),
        None => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).unwrap();
            text
        }
    };
    let tree = sysml_syntax::parse(&text);
    let mut out = String::new();
    print(tree.root_node(), None, text.as_bytes(), 0, &mut out);
    print!("{out}");
}

fn print(
    node: sysml_syntax::tree_sitter::Node,
    field: Option<&str>,
    src: &[u8],
    depth: usize,
    out: &mut String,
) {
    let text = node.utf8_text(src).unwrap_or("");
    let short: String = text
        .chars()
        .take(50)
        .collect::<String>()
        .replace('\n', "\\n");
    let field = field.map(|f| format!("{f}: ")).unwrap_or_default();
    if node.is_named() {
        out.push_str(&format!(
            "{}{field}{} «{short}»\n",
            "  ".repeat(depth),
            node.kind()
        ));
    } else {
        out.push_str(&format!("{}{field}'{}'\n", "  ".repeat(depth), node.kind()));
    }
    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        let name = node.field_name_for_child(i as u32);
        print(child, name, src, depth + 1, out);
    }
}
