use std::path::Path;

fn main() {
    // Locate tree-sitter-sysml queries directory.
    // Build scripts run from crates/sysml-lsp/, so we check:
    //   - ../../tree-sitter-sysml/queries  (submodule in workspace root)
    //   - ../../../tree-sitter-sysml/queries  (sibling of workspace root, CI layout)
    let candidates = [
        "../../tree-sitter-sysml/queries",
        "../../../tree-sitter-sysml/queries",
    ];

    let queries_dir = candidates
        .iter()
        .map(Path::new)
        .find(|p| p.join("highlights.scm").exists());

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("highlights.scm");

    if let Some(dir) = queries_dir {
        let src = dir.join("highlights.scm");
        std::fs::copy(&src, &dest).expect("failed to copy highlights.scm");
        println!("cargo:rerun-if-changed={}", src.display());
    } else {
        // Fallback: write an empty query so the build doesn't fail
        // (semantic tokens will just produce no output)
        std::fs::write(&dest, "").expect("failed to write fallback highlights.scm");
        eprintln!(
            "cargo:warning=tree-sitter-sysml queries not found; semantic tokens will be empty"
        );
    }
}
