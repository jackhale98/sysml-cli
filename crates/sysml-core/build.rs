use std::path::{Path, PathBuf};

fn main() {
    // Look for tree-sitter-sysml grammar source relative to workspace root.
    // Build scripts run from the crate directory (crates/sysml-core/),
    // so we look up two levels to the workspace root.
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
                    .map(|c| format!("  - {}/parser.c", c))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });

    cc::Build::new()
        .include(grammar_dir)
        .file(grammar_dir.join("parser.c"))
        .warnings(false)
        .compile("tree-sitter-sysml");

    println!("cargo:rerun-if-changed={}", grammar_dir.join("parser.c").display());

    // Embed the SysML v2 standard library files.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let stdlib_dir = Path::new(&manifest_dir)
        .join("../../sysml-v2-release/sysml.library")
        .canonicalize();

    if let Ok(stdlib_dir) = stdlib_dir {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_library_files(&stdlib_dir, &mut files);
        files.sort();

        let mut code = String::new();
        code.push_str("pub static STDLIB_FILES: &[(&str, &str)] = &[\n");
        for file in &files {
            let relative = file.strip_prefix(&stdlib_dir).unwrap();
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            let abs_path = file.to_string_lossy().replace('\\', "/");
            code.push_str(&format!(
                "    (\"{}\", include_str!(\"{}\")),\n",
                rel_str, abs_path
            ));
        }
        code.push_str("];\n");

        let out_dir = std::env::var("OUT_DIR").unwrap();
        let dest = Path::new(&out_dir).join("stdlib_files.rs");
        std::fs::write(&dest, code).unwrap();

        println!("cargo:rerun-if-changed={}", stdlib_dir.display());
    } else {
        // No stdlib directory found — generate an empty array so the build
        // still succeeds (e.g. on CI without the submodule).
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let dest = Path::new(&out_dir).join("stdlib_files.rs");
        std::fs::write(&dest, "pub static STDLIB_FILES: &[(&str, &str)] = &[];\n").unwrap();
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
