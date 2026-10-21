//! Build a library from the standard library files whose path contains one
//! of the given substrings; print unresolved references.
use sysml_semantic::{Library, Model, SourceFile};

fn main() {
    let filters: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<SourceFile> = sysml_syntax::stdlib_files()
        .iter()
        .filter(|(p, _)| filters.is_empty() || filters.iter().any(|f| p.contains(f.as_str())))
        .map(|(path, text)| SourceFile { path, text })
        .collect();
    let lib = std::sync::Arc::new(Library::build(&files));
    println!(
        "{} files, {} unresolved",
        files.len(),
        lib.unresolved().len()
    );
    let model = Model::build(lib.clone(), &[]);
    if let Ok(q) = std::env::var("DUMP") {
        match model.resolve_global(&q) {
            Some(e) => println!("{}", model.dump(e)),
            None => println!("{q} not found"),
        }
    }
    for d in lib.unresolved().iter().take(60) {
        let f = model.file_path(d.span.file());
        let text = files.iter().find(|s| s.path == f).unwrap().text;
        let line = text[..d.span.start as usize].matches('\n').count() + 1;
        println!("  {f}:{line} {}", d.message);
    }
}
