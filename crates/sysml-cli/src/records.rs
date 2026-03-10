/// Record I/O utilities for reading and writing TOML records.

use std::io;
use std::path::{Path, PathBuf};

use sysml_core::record::{RecordEnvelope, record_filename};

/// Write a record to the records directory.
pub fn write_record(record: &RecordEnvelope, records_dir: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(records_dir)?;
    let filename = record_filename(&record.meta.id);
    let path = records_dir.join(&filename);
    let content = record.to_toml_string();
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Read all records from a directory.
pub fn read_records(records_dir: &Path) -> Vec<RecordEnvelope> {
    let mut records = Vec::new();
    if let Ok(entries) = std::fs::read_dir(records_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(record) = RecordEnvelope::from_toml_str(&content) {
                        records.push(record);
                    }
                }
            }
        }
    }
    records
}

/// Find a record by ID prefix.
pub fn find_record(records_dir: &Path, id_prefix: &str) -> Option<RecordEnvelope> {
    read_records(records_dir)
        .into_iter()
        .find(|r| r.meta.id.starts_with(id_prefix))
}

/// Resolve the records directory for the current project.
pub fn resolve_records_dir() -> PathBuf {
    // Look for .sysml/ directory, fall back to .sysml/records/
    let sysml_dir = PathBuf::from(".sysml");
    if sysml_dir.exists() {
        sysml_dir.join("records")
    } else {
        PathBuf::from("records")
    }
}
