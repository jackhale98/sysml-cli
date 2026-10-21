//! Every project interchange file published with the OMG release must load,
//! verify its checksums, and survive a write/read round trip.

use std::path::Path;

use sysml_interchange::Kpar;

#[test]
fn omg_library_archives_load_verify_and_round_trip() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sysml-v2-release/sysml.library.kpar");
    if !dir.is_dir() {
        eprintln!(
            "skipping: {} not found (init the sysml-v2-release submodule)",
            dir.display()
        );
        return;
    }
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("kpar") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let kpar = Kpar::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            kpar.verify_checksums().is_empty(),
            "{}: {:?}",
            path.display(),
            kpar.verify_checksums()
        );
        for file in kpar.meta.index.values() {
            assert!(
                kpar.files.contains_key(file),
                "{}: index names missing {file}",
                path.display()
            );
        }
        let back = Kpar::read(&kpar.write().unwrap()).unwrap();
        assert_eq!(back, kpar, "{}", path.display());
        count += 1;
    }
    assert_eq!(count, 10, "expected the 10 OMG library archives");
}
