//! Project interchange files (`.kpar`, KerML 1.0, 10.3).
//!
//! A ZIP archive holding the model interchange files of one project plus,
//! at its top level, `.project.json` ([`ProjectInfo`]) and `.meta.json`
//! ([`ProjectMeta`]). Unknown JSON fields are preserved so a read/write
//! round trip loses nothing written by other tools.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const PROJECT_FILE: &str = ".project.json";
pub const META_FILE: &str = ".meta.json";
/// Checksum algorithm we write; the one OMG uses for the standard libraries.
pub const SHA256: &str = "SHA256";

/// `.project.json`: the `InterchangeProject` (Table 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Mandatory per Table 12; kept optional so incomplete archives still load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainer: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topic: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage: Vec<ProjectUsage>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An `InterchangeProjectUsage`: a project this one depends on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    /// IRI of the used project (ideally resolving to its `.kpar`).
    pub resource: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_constraint: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `.meta.json` (Table 13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// Top-level element name (and short name) → archive path of the file
    /// holding the root namespace that contains it.
    pub index: BTreeMap<String, String>,
    /// Creation time, ISO 8601.
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metamodel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub includes_derived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub includes_implied: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub checksum: BTreeMap<String, Checksums>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The checksum entry for one file. KerML 10.3 describes a list of
/// checksums, while the OMG-published library archives use a single
/// object; both are read, and the single form is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Checksums {
    One(Checksum),
    Many(Vec<Checksum>),
}

impl Checksums {
    pub fn iter(&self) -> impl Iterator<Item = &Checksum> {
        match self {
            Checksums::One(c) => std::slice::from_ref(c).iter(),
            Checksums::Many(cs) => cs.iter(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checksum {
    pub value: String,
    pub algorithm: String,
}

/// An in-memory project interchange file.
#[derive(Debug, Clone, PartialEq)]
pub struct Kpar {
    pub info: ProjectInfo,
    pub meta: ProjectMeta,
    /// Model interchange files by archive path (forward slashes).
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub enum KparError {
    Zip(zip::result::ZipError),
    Io(std::io::Error),
    Json {
        file: &'static str,
        error: serde_json::Error,
    },
    Missing(&'static str),
}

impl fmt::Display for KparError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KparError::Zip(e) => write!(f, "invalid archive: {e}"),
            KparError::Io(e) => write!(f, "{e}"),
            KparError::Json { file, error } => write!(f, "invalid {file}: {error}"),
            KparError::Missing(file) => write!(f, "archive has no top-level {file}"),
        }
    }
}

impl std::error::Error for KparError {}

impl From<zip::result::ZipError> for KparError {
    fn from(e: zip::result::ZipError) -> Self {
        KparError::Zip(e)
    }
}

impl From<std::io::Error> for KparError {
    fn from(e: std::io::Error) -> Self {
        KparError::Io(e)
    }
}

/// A file whose recorded checksum does not match its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChecksumProblem {
    Mismatch {
        path: String,
    },
    MissingFile {
        path: String,
    },
    /// An algorithm we cannot compute; the file is not verified.
    Unsupported {
        path: String,
        algorithm: String,
    },
}

impl Kpar {
    /// Read a project interchange file.
    pub fn read(bytes: &[u8]) -> Result<Self, KparError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut info = None;
        let mut meta = None;
        let mut files = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name()?.trim_start_matches("./").to_owned();
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;
            match name.as_str() {
                PROJECT_FILE => info = Some(parse_json(PROJECT_FILE, &data)?),
                META_FILE => meta = Some(parse_json(META_FILE, &data)?),
                _ => {
                    files.insert(name, data);
                }
            }
        }
        Ok(Kpar {
            info: info.ok_or(KparError::Missing(PROJECT_FILE))?,
            meta: meta.ok_or(KparError::Missing(META_FILE))?,
            files,
        })
    }

    /// Write the archive. Entries are stored uncompressed in path order with
    /// fixed timestamps, so equal projects produce identical bytes.
    pub fn write(&self) -> Result<Vec<u8>, KparError> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        let info = serde_json::to_vec(&self.info).expect("project info serializes");
        let meta = serde_json::to_vec(&self.meta).expect("project meta serializes");
        for (path, data) in [(PROJECT_FILE, &info), (META_FILE, &meta)]
            .into_iter()
            .chain(self.files.iter().map(|(p, d)| (p.as_str(), d)))
        {
            writer.start_file(path, options)?;
            writer.write_all(data)?;
        }
        Ok(writer.finish()?.into_inner())
    }

    /// Record a SHA256 checksum for every model file.
    pub fn compute_checksums(&mut self) {
        self.meta.checksum = self
            .files
            .iter()
            .map(|(path, data)| {
                let checksum = Checksum {
                    value: sha256_hex(data),
                    algorithm: SHA256.to_owned(),
                };
                (path.clone(), Checksums::One(checksum))
            })
            .collect();
    }

    /// Check recorded checksums against file contents.
    pub fn verify_checksums(&self) -> Vec<ChecksumProblem> {
        let mut problems = Vec::new();
        for (path, checksums) in &self.meta.checksum {
            let Some(data) = self.files.get(path) else {
                problems.push(ChecksumProblem::MissingFile { path: path.clone() });
                continue;
            };
            for checksum in checksums.iter() {
                if checksum.algorithm.eq_ignore_ascii_case(SHA256)
                    || checksum.algorithm.eq_ignore_ascii_case("SHA-256")
                {
                    if !checksum.value.eq_ignore_ascii_case(&sha256_hex(data)) {
                        problems.push(ChecksumProblem::Mismatch { path: path.clone() });
                    }
                } else {
                    problems.push(ChecksumProblem::Unsupported {
                        path: path.clone(),
                        algorithm: checksum.algorithm.clone(),
                    });
                }
            }
        }
        problems
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(
    file: &'static str,
    data: &[u8],
) -> Result<T, KparError> {
    serde_json::from_slice(data).map_err(|error| KparError::Json { file, error })
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Kpar {
        let mut kpar = Kpar {
            info: ProjectInfo {
                name: "Drone".into(),
                description: Some("Example".into()),
                version: Some("0.1.0".into()),
                license: None,
                maintainer: vec![],
                website: None,
                topic: vec![],
                usage: vec![ProjectUsage {
                    resource: "https://www.omg.org/spec/SysML/20250201/Systems-Library.kpar".into(),
                    version_constraint: Some("2.0.0".into()),
                    extra: Map::new(),
                }],
                extra: Map::new(),
            },
            meta: ProjectMeta {
                index: BTreeMap::from([("Drone".into(), "Drone.sysml".into())]),
                created: "2026-10-21T00:00:00Z".into(),
                metamodel: Some("https://www.omg.org/spec/SysML/20250201".into()),
                includes_derived: None,
                includes_implied: None,
                checksum: BTreeMap::new(),
                extra: Map::new(),
            },
            files: BTreeMap::from([("Drone.sysml".into(), b"package Drone;\n".to_vec())]),
        };
        kpar.compute_checksums();
        kpar
    }

    #[test]
    fn round_trips_and_is_deterministic() {
        let kpar = sample();
        let bytes = kpar.write().unwrap();
        assert_eq!(bytes, kpar.write().unwrap());
        let back = Kpar::read(&bytes).unwrap();
        assert_eq!(back, kpar);
        assert!(back.verify_checksums().is_empty());
    }

    #[test]
    fn detects_tampering() {
        let mut kpar = sample();
        kpar.files
            .insert("Drone.sysml".into(), b"package Other;\n".to_vec());
        assert_eq!(
            kpar.verify_checksums(),
            vec![ChecksumProblem::Mismatch {
                path: "Drone.sysml".into()
            }]
        );
    }

    #[test]
    fn reads_list_form_checksums_and_keeps_unknown_fields() {
        let meta: ProjectMeta = serde_json::from_str(
            r#"{"index":{},"created":"x","checksum":{"a.sysml":[{"value":"00","algorithm":"SHA1"}]},"vendor":1}"#,
        )
        .unwrap();
        assert!(matches!(meta.checksum["a.sysml"], Checksums::Many(_)));
        assert_eq!(meta.extra["vendor"], 1);
    }

    #[test]
    fn missing_metadata_is_an_error() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("a.sysml", zip::write::SimpleFileOptions::default())
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(matches!(
            Kpar::read(&bytes),
            Err(KparError::Missing(PROJECT_FILE))
        ));
    }
}
