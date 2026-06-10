//! Project discovery — locate and load a `.sysml/config.toml` by walking
//! up the directory tree from a starting path.

use std::path::{Path, PathBuf};

use crate::config::ProjectConfig;

/// Maximum number of parent directories to traverse when searching for a
/// project root. Acts as a safety limit to avoid unbounded traversal.
const MAX_WALK_DEPTH: usize = 50;

/// The directory name that marks a project root.
const PROJECT_DIR: &str = ".sysml";

/// The config filename within the project directory.
const CONFIG_FILE: &str = "config.toml";

/// Walk up from `start` looking for a `.sysml/config.toml` file.
///
/// Returns the project root directory (the parent of `.sysml/`) together
/// with the parsed [`ProjectConfig`]. If no config file is found within
/// [`MAX_WALK_DEPTH`] parent levels, returns `None`.
///
/// If `start` is a file rather than a directory, the search begins at
/// its parent directory.
pub fn discover_project(start: &Path) -> Option<(PathBuf, ProjectConfig)> {
    let start_dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };

    let mut current = start_dir.to_path_buf();

    for _ in 0..MAX_WALK_DEPTH {
        let config_path = current.join(PROJECT_DIR).join(CONFIG_FILE);
        if config_path.is_file() {
            let config = ProjectConfig::load(&config_path).ok()?;
            return Some((current, config));
        }

        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break, // reached filesystem root
        }
    }

    None
}

/// Walk up from `start` looking for a `.sysml/config.toml` file, returning
/// only the project root directory path.
///
/// This is a convenience wrapper around [`discover_project`] for callers
/// that don't need the parsed config.
pub fn discover_project_root(start: &Path) -> Option<PathBuf> {
    discover_project(start).map(|(root, _)| root)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a temporary project tree with a `.sysml/config.toml`.
    fn setup_project(dir: &Path, toml: &str) {
        let sysml_dir = dir.join(PROJECT_DIR);
        fs::create_dir_all(&sysml_dir).unwrap();
        fs::write(sysml_dir.join(CONFIG_FILE), toml).unwrap();
    }

    #[test]
    fn discover_at_project_root() {
        let tmp = tempdir();
        setup_project(&tmp, "[project]\nname = \"Root\"\n");

        let (root, cfg) = discover_project(&tmp).unwrap();
        assert_eq!(root, tmp);
        assert_eq!(cfg.project.name, "Root");
    }

    #[test]
    fn discover_from_subdirectory() {
        let tmp = tempdir();
        setup_project(&tmp, "[project]\nname = \"Sub\"\n");

        let sub = tmp.join("model").join("subsystem");
        fs::create_dir_all(&sub).unwrap();

        let (root, cfg) = discover_project(&sub).unwrap();
        assert_eq!(root, tmp);
        assert_eq!(cfg.project.name, "Sub");
    }

    #[test]
    fn discover_from_file_path() {
        let tmp = tempdir();
        setup_project(&tmp, "[project]\nname = \"File\"\n");

        let file = tmp.join("model").join("main.sysml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "package Main {}").unwrap();

        let (root, cfg) = discover_project(&file).unwrap();
        assert_eq!(root, tmp);
        assert_eq!(cfg.project.name, "File");
    }

    #[test]
    fn discover_returns_none_when_no_config() {
        let tmp = tempdir();
        // No .sysml directory
        assert!(discover_project(&tmp).is_none());
    }

    #[test]
    fn discover_root_only() {
        let tmp = tempdir();
        setup_project(&tmp, "[project]\nname = \"RootOnly\"\n");

        let root = discover_project_root(&tmp).unwrap();
        assert_eq!(root, tmp);
    }

    #[test]
    fn discover_root_returns_none_when_absent() {
        let tmp = tempdir();
        assert!(discover_project_root(&tmp).is_none());
    }

    #[test]
    fn discover_prefers_nearest_ancestor() {
        let tmp = tempdir();
        // Outer project
        setup_project(&tmp, "[project]\nname = \"Outer\"\n");

        // Inner project
        let inner = tmp.join("packages").join("inner");
        setup_project(&inner, "[project]\nname = \"Inner\"\n");

        let deep = inner.join("src").join("deep");
        fs::create_dir_all(&deep).unwrap();

        let (root, cfg) = discover_project(&deep).unwrap();
        assert_eq!(root, inner);
        assert_eq!(cfg.project.name, "Inner");
    }

    /// Create a unique temporary directory that is automatically cleaned up
    /// when the `PathBuf` is dropped (well, not really — we rely on OS tmpdir
    /// cleanup, but the path is unique per test).
    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sysml_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
