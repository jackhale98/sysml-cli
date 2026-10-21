//! Build the standard library (and optionally a project directory) and
//! print timings and unresolved references.
//!
//!     cargo run --release -p sysml-semantic --example semantic_stats -- [dir]

use std::time::Instant;

use sysml_semantic::{Library, Model, SourceFile};

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("sysml" | "kerml")
            ) {
                out.push(p);
            }
        }
    }
}

fn main() {
    let t = Instant::now();
    let lib = Library::standard();
    let s = lib.stats();
    println!(
        "library: {} elements, {} refs, total {:.0} ms (parse {:.0}, lower {:.0}, resolve {:.0}); {} unresolved",
        s.elements,
        s.references,
        t.elapsed().as_secs_f64() * 1000.0,
        s.parse_ms,
        s.lower_ms,
        s.resolve_ms,
        lib.unresolved().len()
    );
    let verbose = std::env::var("VERBOSE").is_ok();
    if verbose {
        let model = Model::build(lib.clone(), &[]);
        for d in lib.unresolved().iter().take(200) {
            let f = model.file_path(d.span.file());
            println!("  {f}:{} {}", d.span.start, d.message);
        }
    }
    if let Some(dir) = std::env::args().nth(1) {
        let mut paths = Vec::new();
        collect(std::path::Path::new(&dir), &mut paths);
        paths.sort();
        let texts: Vec<(String, String)> = paths
            .iter()
            .map(|p| {
                (
                    p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned(),
                    std::fs::read_to_string(p).unwrap(),
                )
            })
            .collect();
        let files: Vec<SourceFile> = texts
            .iter()
            .map(|(p, t)| SourceFile { path: p, text: t })
            .collect();
        let t = Instant::now();
        let model = Model::build(lib.clone(), &files);
        let s = model.stats();
        println!(
            "project: {} files, {} elements, {:.1} ms (parse {:.1}, lower {:.1}, resolve {:.1}); {} unresolved",
            files.len(),
            s.elements,
            t.elapsed().as_secs_f64() * 1000.0,
            s.parse_ms,
            s.lower_ms,
            s.resolve_ms,
            model.unresolved().len()
        );
        for d in model
            .unresolved()
            .iter()
            .take(if verbose { 500 } else { 40 })
        {
            let f = model.file_path(d.span.file());
            let text = &texts.iter().find(|(p, _)| p == f).unwrap().1;
            let line = text[..d.span.start as usize].matches('\n').count() + 1;
            println!("  {f}:{line} {}", d.message);
        }
    }
}
