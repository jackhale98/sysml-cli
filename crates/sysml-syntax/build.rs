//! Compiles the tree-sitter-sysml parser and embeds the grammar's highlight
//! query and the OMG standard library sources.

use std::path::{Path, PathBuf};

fn main() {
    // Build scripts run from the crate directory (crates/sysml-syntax/).
    let candidates = [
        "../../tree-sitter-sysml/src",    // submodule in workspace root
        "../../../tree-sitter-sysml/src", // sibling of workspace root
    ];
    let grammar_dir = candidates
        .iter()
        .map(Path::new)
        .find(|p| p.join("parser.c").exists())
        .unwrap_or_else(|| {
            panic!(
                "tree-sitter-sysml grammar not found. Tried:\n{}",
                candidates
                    .iter()
                    .map(|c| format!("  - {c}/parser.c"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });

    let mut build = cc::Build::new();
    build
        .include(grammar_dir)
        .file(grammar_dir.join("parser.c"))
        .warnings(false);
    // wasm32-unknown-unknown has no libc; tree-sitter-language ships the
    // minimal headers the generated parser needs.
    if let Ok(headers) = std::env::var("DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS") {
        build.include(headers);
    }
    build.compile("tree-sitter-sysml");
    println!(
        "cargo:rerun-if-changed={}",
        grammar_dir.join("parser.c").display()
    );

    let out_dir = std::env::var("OUT_DIR").unwrap();

    // The grammar's highlight query.
    let highlights = grammar_dir.join("../queries/highlights.scm");
    let query = std::fs::read_to_string(&highlights).unwrap_or_default();
    std::fs::write(Path::new(&out_dir).join("highlights.scm"), query).unwrap();
    println!("cargo:rerun-if-changed={}", highlights.display());

    // The SysML v2 standard library sources.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let stdlib_dir = Path::new(&manifest_dir)
        .join("../../sysml-v2-release/sysml.library")
        .canonicalize();
    let dest = Path::new(&out_dir).join("stdlib_files.rs");
    if let Ok(stdlib_dir) = stdlib_dir {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_library_files(&stdlib_dir, &mut files);
        files.sort();
        let mut code = String::from("static STDLIB_FILES: &[(&str, &str)] = &[\n");
        for file in &files {
            let relative = file.strip_prefix(&stdlib_dir).unwrap();
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            let abs_path = file.to_string_lossy().replace('\\', "/");
            code.push_str(&format!("    ({rel_str:?}, include_str!({abs_path:?})),\n"));
        }
        code.push_str("];\n");
        std::fs::write(&dest, code).unwrap();
        println!("cargo:rerun-if-changed={}", stdlib_dir.display());
    } else {
        // No stdlib directory (submodule not initialised): an empty library
        // keeps the build working.
        std::fs::write(&dest, "static STDLIB_FILES: &[(&str, &str)] = &[];\n").unwrap();
    }
}

fn collect_library_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_library_files(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "sysml" || ext == "kerml" {
                    files.push(path);
                }
            }
        }
    }
}
