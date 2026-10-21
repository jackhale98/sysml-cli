//! Project archives for the browser: read a `.kpar` or any ZIP of a
//! project into text files, and write a project back out as a `.kpar`.

use std::collections::BTreeMap;

use serde::Serialize;
use sysml_interchange::kpar::{Kpar, ProjectInfo, ProjectMeta, META_FILE, PROJECT_FILE};

use crate::editor::SourceFile;

#[derive(Serialize, Debug)]
pub struct ReadArchive {
    pub info: ProjectInfo,
    pub files: Vec<SourceFile>,
    /// Entries that are not UTF-8 text (images, binaries), left out.
    pub skipped: Vec<String>,
    /// Checksum problems recorded in `.meta.json`, if any.
    pub warnings: Vec<String>,
}

/// Read a `.kpar` or a plain ZIP of a project.
pub fn read(bytes: &[u8], fallback_name: &str) -> Result<ReadArchive, String> {
    let kpar = Kpar::read_lenient(bytes, fallback_name).map_err(|e| e.to_string())?;
    let warnings = kpar
        .verify_checksums()
        .into_iter()
        .map(|p| format!("{p:?}"))
        .collect();
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for (path, data) in kpar.files {
        match String::from_utf8(data) {
            Ok(text) => files.push(SourceFile { path, text }),
            Err(_) => skipped.push(path),
        }
    }
    Ok(ReadArchive {
        info: kpar.info,
        files,
        skipped,
        warnings,
    })
}

/// Write the project as a `.kpar`: `.project.json`, a `.meta.json` whose
/// index maps each top-level element to its file, SHA256 checksums, and
/// the files themselves. `created` is an ISO 8601 time.
pub fn write(
    info: ProjectInfo,
    files: Vec<SourceFile>,
    created: String,
) -> Result<Vec<u8>, String> {
    let mut meta = ProjectMeta::created(created);
    let mut stored = BTreeMap::new();
    for file in files {
        if file.path == PROJECT_FILE || file.path == META_FILE {
            continue;
        }
        if file.path.ends_with(".sysml") || file.path.ends_with(".kerml") {
            let model = sysml_core::parser::parse_file(&file.path, &file.text);
            for def in model.definitions.iter().filter(|d| d.parent_def.is_none()) {
                meta.index
                    .entry(def.name.clone())
                    .or_insert_with(|| file.path.clone());
            }
        }
        stored.insert(file.path, file.text.into_bytes());
    }
    let mut kpar = Kpar {
        info,
        meta,
        files: stored,
    };
    kpar.compute_checksums();
    kpar.write().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            text: text.into(),
        }
    }

    #[test]
    fn round_trips_a_project() {
        let bytes = write(
            ProjectInfo::named("Demo".into()),
            vec![
                file("model/A.sysml", "package A { part def B; }"),
                file("README.md", "# Demo"),
            ],
            "2026-10-21T00:00:00Z".into(),
        )
        .unwrap();
        let read = read(&bytes, "fallback").unwrap();
        assert_eq!(read.info.name, "Demo");
        assert_eq!(read.files.len(), 2);
        assert!(read.warnings.is_empty());
        let kpar = Kpar::read(&bytes).unwrap();
        assert_eq!(
            kpar.meta.index.get("A").map(String::as_str),
            Some("model/A.sysml")
        );
    }

    #[test]
    fn reads_a_plain_zip_with_a_top_folder() {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("drone-main/model/A.sysml", options)
            .unwrap();
        writer.write_all(b"package A;").unwrap();
        writer.start_file("drone-main/logo.png", options).unwrap();
        writer.write_all(&[0xff, 0xfe, 0x00]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let read = read(&bytes, "fallback").unwrap();
        assert_eq!(read.info.name, "drone-main");
        assert_eq!(read.files[0].path, "model/A.sysml");
        assert_eq!(read.skipped, vec!["logo.png".to_string()]);
    }
}
