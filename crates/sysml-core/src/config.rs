/// Project configuration parsed from `.sysml/config.toml`.
///
/// Provides typed access to project settings including name, model root,
/// library paths, and default output options. Includes a hand-written TOML
/// parser for the limited config format we support (no nested tables, no
/// arrays of tables).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when loading a project configuration file.
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error reading the config file.
    Io(std::io::Error),
    /// Parse error in the TOML content.
    Parse(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
            ConfigError::Parse(msg) => write!(f, "config parse error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::Parse(_) => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

/// Top-level project configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectConfig {
    #[serde(default)]
    pub project: ProjectSection,
    #[serde(default)]
    pub defaults: DefaultsSection,
    #[serde(default)]
    pub pipelines: Vec<PipelineConfig>,
}

/// A named pipeline: a sequence of sysml commands to run in order.
///
/// Defined in config as `[[pipeline]]` array-of-tables entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineConfig {
    /// Pipeline name (e.g. "ci", "nightly", "pre-commit").
    pub name: String,
    /// Ordered list of sysml commands to execute (without the `sysml` prefix).
    /// Example: `["lint *.sysml", "trace --check *.sysml"]`
    pub steps: Vec<String>,
}

/// The `[project]` section of the config file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSection {
    /// Human-readable project name.
    #[serde(default)]
    pub name: String,
    /// Root directory for model files, relative to the project root.
    #[serde(default = "default_model_root")]
    pub model_root: PathBuf,
    /// Additional directories to search for library models.
    #[serde(default)]
    pub library_paths: Vec<PathBuf>,
    /// Path to the SysML v2 standard library directory.
    #[serde(default)]
    pub stdlib_path: Option<PathBuf>,
}

/// The `[defaults]` section of the config file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultsSection {
    /// Default author name for generated elements.
    #[serde(default)]
    pub author: String,
    /// Default output directory for reports and exports.
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,
    /// Default output format (e.g. "text", "json").
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_model_root() -> PathBuf {
    PathBuf::from(".")
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("records/")
}

fn default_format() -> String {
    "text".to_string()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project: ProjectSection::default(),
            defaults: DefaultsSection::default(),
            pipelines: Vec::new(),
        }
    }
}

impl Default for ProjectSection {
    fn default() -> Self {
        Self {
            name: String::new(),
            model_root: default_model_root(),
            library_paths: Vec::new(),
            stdlib_path: None,
        }
    }
}

impl Default for DefaultsSection {
    fn default() -> Self {
        Self {
            author: String::new(),
            output_dir: default_output_dir(),
            format: default_format(),
        }
    }
}

// ---------------------------------------------------------------------------
// Loading & serialization
// ---------------------------------------------------------------------------

impl ProjectConfig {
    /// Load a `ProjectConfig` from a TOML file at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Parse a `ProjectConfig` from a TOML string.
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        parse_toml_config(input)
    }

    /// Serialize the config back to a TOML string.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();

        out.push_str("[project]\n");
        out.push_str(&format!("name = {}\n", quote_toml_string(&self.project.name)));
        out.push_str(&format!(
            "model_root = {}\n",
            quote_toml_string(&self.project.model_root.to_string_lossy())
        ));
        out.push_str(&format!(
            "library_paths = [{}]\n",
            self.project
                .library_paths
                .iter()
                .map(|p| quote_toml_string(&p.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(ref stdlib) = self.project.stdlib_path {
            out.push_str(&format!(
                "stdlib_path = {}\n",
                quote_toml_string(&stdlib.to_string_lossy())
            ));
        }

        out.push('\n');
        out.push_str("[defaults]\n");
        out.push_str(&format!(
            "author = {}\n",
            quote_toml_string(&self.defaults.author)
        ));
        out.push_str(&format!(
            "output_dir = {}\n",
            quote_toml_string(&self.defaults.output_dir.to_string_lossy())
        ));
        out.push_str(&format!(
            "format = {}\n",
            quote_toml_string(&self.defaults.format)
        ));

        for pipeline in &self.pipelines {
            out.push('\n');
            out.push_str("[[pipeline]]\n");
            out.push_str(&format!("name = {}\n", quote_toml_string(&pipeline.name)));
            out.push_str(&format!(
                "steps = [{}]\n",
                pipeline
                    .steps
                    .iter()
                    .map(|s| quote_toml_string(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        out
    }
}

/// Produce a TOML-quoted string value.
fn quote_toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

// ---------------------------------------------------------------------------
// Hand-written TOML parser
// ---------------------------------------------------------------------------

/// Parse our limited TOML config format.
///
/// Supported features:
/// - `[section]` headers
/// - `key = "string"` pairs
/// - `key = ["a", "b"]` inline string arrays
/// - `#` line comments
/// - blank lines
fn parse_toml_config(input: &str) -> Result<ProjectConfig, ConfigError> {
    let mut config = ProjectConfig::default();
    let mut current_section: Option<&str> = None;
    // Track current pipeline being built (for [[pipeline]] array-of-tables)
    let mut current_pipeline: Option<PipelineConfig> = None;

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = strip_comment(raw_line).trim();

        if line.is_empty() {
            continue;
        }

        // Array-of-tables header: [[pipeline]]
        if line.starts_with("[[") && line.ends_with("]]") {
            let table_name = line[2..line.len() - 2].trim();
            match table_name {
                "pipeline" => {
                    // Flush any previous pipeline
                    if let Some(p) = current_pipeline.take() {
                        config.pipelines.push(p);
                    }
                    current_pipeline = Some(PipelineConfig {
                        name: String::new(),
                        steps: Vec::new(),
                    });
                    current_section = Some("pipeline");
                    continue;
                }
                other => {
                    return Err(ConfigError::Parse(format!(
                        "line {}: unknown array-of-tables [[{other}]]",
                        line_no + 1
                    )));
                }
            }
        }

        // Section header: [project], [defaults]
        if line.starts_with('[') {
            // Flush any in-progress pipeline
            if let Some(p) = current_pipeline.take() {
                config.pipelines.push(p);
            }
            if !line.ends_with(']') {
                return Err(ConfigError::Parse(format!(
                    "line {}: unclosed section header",
                    line_no + 1
                )));
            }
            let section_name = line[1..line.len() - 1].trim();
            current_section = match section_name {
                "project" => Some("project"),
                "defaults" => Some("defaults"),
                other => {
                    return Err(ConfigError::Parse(format!(
                        "line {}: unknown section [{other}]",
                        line_no + 1
                    )));
                }
            };
            continue;
        }

        // Key = value
        let Some((key, value)) = parse_key_value(line) else {
            return Err(ConfigError::Parse(format!(
                "line {}: expected `key = value`",
                line_no + 1
            )));
        };

        let section = current_section.ok_or_else(|| {
            ConfigError::Parse(format!(
                "line {}: key `{key}` appears before any section header",
                line_no + 1
            ))
        })?;

        match (section, key) {
            ("project", "name") => {
                config.project.name = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
            }
            ("project", "model_root") => {
                let s = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
                config.project.model_root = PathBuf::from(s);
            }
            ("project", "library_paths") => {
                config.project.library_paths = parse_string_array(value)
                    .map_err(|e| {
                        ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                    })?
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
            }
            ("project", "stdlib_path") => {
                let s = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
                config.project.stdlib_path = Some(PathBuf::from(s));
            }
            ("defaults", "author") => {
                config.defaults.author = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
            }
            ("defaults", "output_dir") => {
                let s = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
                config.defaults.output_dir = PathBuf::from(s);
            }
            ("defaults", "format") => {
                config.defaults.format = parse_string_value(value).map_err(|e| {
                    ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                })?;
            }
            ("pipeline", "name") => {
                if let Some(ref mut p) = current_pipeline {
                    p.name = parse_string_value(value).map_err(|e| {
                        ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                    })?;
                }
            }
            ("pipeline", "steps") => {
                if let Some(ref mut p) = current_pipeline {
                    p.steps = parse_string_array(value).map_err(|e| {
                        ConfigError::Parse(format!("line {}: {e}", line_no + 1))
                    })?;
                }
            }
            (sec, k) => {
                return Err(ConfigError::Parse(format!(
                    "line {}: unknown key `{k}` in [{sec}]",
                    line_no + 1
                )));
            }
        }
    }

    // Flush trailing pipeline
    if let Some(p) = current_pipeline.take() {
        config.pipelines.push(p);
    }

    Ok(config)
}

/// Strip a trailing `# comment` from a line, respecting quoted strings.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split `key = value` and return `(key, value)` with trimmed whitespace.
fn parse_key_value(line: &str) -> Option<(&str, &str)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim();
    let value = line[eq_pos + 1..].trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Parse a TOML string value: `"..."` -> inner string with escape handling.
fn parse_string_value(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Err(format!("expected quoted string, got: {value}"));
    }

    // Find closing quote (respecting escapes)
    let inner = &value[1..];
    let mut result = String::new();
    let mut chars = inner.chars();
    loop {
        match chars.next() {
            None => return Err("unterminated string".to_string()),
            Some('"') => break,
            Some('\\') => match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => return Err("unterminated escape in string".to_string()),
            },
            Some(c) => result.push(c),
        }
    }
    Ok(result)
}

/// Parse an inline TOML array of strings: `["a", "b", "c"]`.
fn parse_string_array(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(format!("expected array [...], got: {value}"));
    }

    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    // Split on commas that are outside of quoted strings
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in inner.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => {
                current.push(ch);
                escaped = true;
            }
            '"' => {
                in_string = !in_string;
                current.push(ch);
            }
            ',' if !in_string => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    items.push(parse_string_value(&trimmed)?);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        items.push(parse_string_value(&trimmed)?);
    }

    Ok(items)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let cfg = ProjectConfig::default();
        assert_eq!(cfg.project.name, "");
        assert_eq!(cfg.project.model_root, PathBuf::from("."));
        assert!(cfg.project.library_paths.is_empty());
        assert_eq!(cfg.defaults.author, "");
        assert_eq!(cfg.defaults.output_dir, PathBuf::from("records/"));
        assert_eq!(cfg.defaults.format, "text");
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[project]
name = "BrakeSystem"
model_root = "model/"
library_paths = ["libraries/"]

[defaults]
author = "Alice"
output_dir = "records/"
format = "text"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.project.name, "BrakeSystem");
        assert_eq!(cfg.project.model_root, PathBuf::from("model/"));
        assert_eq!(cfg.project.library_paths, vec![PathBuf::from("libraries/")]);
        assert_eq!(cfg.defaults.author, "Alice");
        assert_eq!(cfg.defaults.output_dir, PathBuf::from("records/"));
        assert_eq!(cfg.defaults.format, "text");
    }

    #[test]
    fn parse_minimal_config() {
        let toml = "[project]\nname = \"Foo\"\n";
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.project.name, "Foo");
        // Defaults should fill in
        assert_eq!(cfg.project.model_root, PathBuf::from("."));
        assert_eq!(cfg.defaults.format, "text");
    }

    #[test]
    fn parse_empty_config() {
        // Completely empty string is valid — all defaults apply
        let cfg = ProjectConfig::from_toml_str("").unwrap();
        assert_eq!(cfg, ProjectConfig::default());
    }

    #[test]
    fn parse_comments_and_blank_lines() {
        let toml = r#"
# Project configuration
[project]
name = "Test"  # inline comment

# Another comment
[defaults]
format = "json"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.project.name, "Test");
        assert_eq!(cfg.defaults.format, "json");
    }

    #[test]
    fn parse_multiple_library_paths() {
        let toml = r#"
[project]
name = "Multi"
library_paths = ["libs/", "vendor/models/", "third_party/"]
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(
            cfg.project.library_paths,
            vec![
                PathBuf::from("libs/"),
                PathBuf::from("vendor/models/"),
                PathBuf::from("third_party/"),
            ]
        );
    }

    #[test]
    fn parse_empty_array() {
        let toml = "[project]\nlibrary_paths = []\n";
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert!(cfg.project.library_paths.is_empty());
    }

    #[test]
    fn parse_escaped_strings() {
        let toml = r#"
[project]
name = "Brake\"System"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.project.name, "Brake\"System");
    }

    #[test]
    fn error_unknown_section() {
        let toml = "[unknown]\nfoo = \"bar\"\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("unknown section"));
    }

    #[test]
    fn error_unknown_key() {
        let toml = "[project]\nbogus = \"value\"\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("unknown key"));
    }

    #[test]
    fn error_key_before_section() {
        let toml = "name = \"oops\"\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("before any section"));
    }

    #[test]
    fn error_unclosed_section() {
        let toml = "[project\nname = \"x\"\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("unclosed section"));
    }

    #[test]
    fn error_unquoted_value() {
        let toml = "[project]\nname = bare\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("expected quoted string"));
    }

    #[test]
    fn roundtrip_to_toml_string() {
        let cfg = ProjectConfig {
            project: ProjectSection {
                name: "RoundTrip".to_string(),
                model_root: PathBuf::from("src/model/"),
                library_paths: vec![PathBuf::from("libs/"), PathBuf::from("ext/")],
                stdlib_path: None,
            },
            defaults: DefaultsSection {
                author: "Bob".to_string(),
                output_dir: PathBuf::from("out/"),
                format: "json".to_string(),
            },
            pipelines: Vec::new(),
        };

        let toml_str = cfg.to_toml_string();
        let parsed = ProjectConfig::from_toml_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn to_toml_string_format() {
        let cfg = ProjectConfig::default();
        let s = cfg.to_toml_string();
        assert!(s.contains("[project]"));
        assert!(s.contains("[defaults]"));
        assert!(s.contains("model_root = \".\""));
        assert!(s.contains("format = \"text\""));
    }

    #[test]
    fn config_error_display() {
        let io_err = ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(io_err.to_string().contains("I/O error"));

        let parse_err = ConfigError::Parse("bad input".to_string());
        assert!(parse_err.to_string().contains("bad input"));
    }

    #[test]
    fn config_error_source() {
        use std::error::Error;

        let io_err = ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(io_err.source().is_some());

        let parse_err = ConfigError::Parse("oops".to_string());
        assert!(parse_err.source().is_none());
    }

    #[test]
    fn load_nonexistent_file() {
        let result = ProjectConfig::load(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::Io(_) => {} // expected
            other => panic!("expected Io error, got: {other}"),
        }
    }

    #[test]
    fn strip_comment_preserves_hashes_in_strings() {
        assert_eq!(strip_comment(r#"name = "a#b" # real comment"#), r#"name = "a#b" "#);
    }

    #[test]
    fn parse_string_array_with_spaces() {
        let items = parse_string_array(r#"[ "a" , "b" , "c" ]"#).unwrap();
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    // -- Pipeline config tests -----------------------------------------------

    #[test]
    fn parse_single_pipeline() {
        let toml = r#"
[project]
name = "Test"

[[pipeline]]
name = "ci"
steps = ["lint *.sysml", "trace --check *.sysml"]
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.pipelines.len(), 1);
        assert_eq!(cfg.pipelines[0].name, "ci");
        assert_eq!(cfg.pipelines[0].steps, vec![
            "lint *.sysml",
            "trace --check *.sysml",
        ]);
    }

    #[test]
    fn parse_multiple_pipelines() {
        let toml = r#"
[project]
name = "Multi"

[[pipeline]]
name = "ci"
steps = ["lint *.sysml", "fmt --check *.sysml"]

[[pipeline]]
name = "nightly"
steps = ["coverage --check --min-score 80 *.sysml", "report dashboard *.sysml"]
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.pipelines.len(), 2);
        assert_eq!(cfg.pipelines[0].name, "ci");
        assert_eq!(cfg.pipelines[0].steps.len(), 2);
        assert_eq!(cfg.pipelines[1].name, "nightly");
        assert_eq!(cfg.pipelines[1].steps.len(), 2);
        assert_eq!(cfg.pipelines[1].steps[0], "coverage --check --min-score 80 *.sysml");
    }

    #[test]
    fn parse_pipeline_after_defaults() {
        let toml = r#"
[project]
name = "Test"

[defaults]
format = "json"

[[pipeline]]
name = "check"
steps = ["lint model.sysml"]
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.defaults.format, "json");
        assert_eq!(cfg.pipelines.len(), 1);
        assert_eq!(cfg.pipelines[0].name, "check");
    }

    #[test]
    fn parse_no_pipelines_is_empty() {
        let toml = r#"
[project]
name = "NoPipes"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert!(cfg.pipelines.is_empty());
    }

    #[test]
    fn parse_pipeline_empty_steps() {
        let toml = r#"
[[pipeline]]
name = "empty"
steps = []
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.pipelines.len(), 1);
        assert!(cfg.pipelines[0].steps.is_empty());
    }

    #[test]
    fn roundtrip_pipeline_config() {
        let cfg = ProjectConfig {
            project: ProjectSection {
                name: "PipeTest".to_string(),
                ..ProjectSection::default()
            },
            defaults: DefaultsSection::default(),
            pipelines: vec![
                PipelineConfig {
                    name: "ci".to_string(),
                    steps: vec!["lint *.sysml".to_string(), "fmt --check *.sysml".to_string()],
                },
                PipelineConfig {
                    name: "deploy".to_string(),
                    steps: vec!["report dashboard *.sysml".to_string()],
                },
            ],
        };
        let toml_str = cfg.to_toml_string();
        let parsed = ProjectConfig::from_toml_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn parse_stdlib_path() {
        let toml = r#"
[project]
name = "StdLib"
stdlib_path = "/usr/share/sysml/stdlib"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(
            cfg.project.stdlib_path,
            Some(PathBuf::from("/usr/share/sysml/stdlib"))
        );
    }

    #[test]
    fn parse_no_stdlib_path() {
        let toml = "[project]\nname = \"NoStdLib\"\n";
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert!(cfg.project.stdlib_path.is_none());
    }

    #[test]
    fn roundtrip_stdlib_path() {
        let cfg = ProjectConfig {
            project: ProjectSection {
                name: "StdTest".to_string(),
                stdlib_path: Some(PathBuf::from("/opt/sysml/stdlib")),
                ..ProjectSection::default()
            },
            defaults: DefaultsSection::default(),
            pipelines: Vec::new(),
        };
        let toml_str = cfg.to_toml_string();
        assert!(toml_str.contains("stdlib_path"));
        let parsed = ProjectConfig::from_toml_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn error_unknown_array_of_tables() {
        let toml = "[[unknown]]\nfoo = \"bar\"\n";
        let err = ProjectConfig::from_toml_str(toml).unwrap_err();
        assert!(err.to_string().contains("unknown array-of-tables"));
    }

    #[test]
    fn pipeline_between_sections() {
        // Pipeline sandwiched between project and defaults
        let toml = r#"
[project]
name = "Sandwich"

[[pipeline]]
name = "mid"
steps = ["lint a.sysml"]

[defaults]
author = "Bob"
"#;
        let cfg = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.project.name, "Sandwich");
        assert_eq!(cfg.defaults.author, "Bob");
        assert_eq!(cfg.pipelines.len(), 1);
        assert_eq!(cfg.pipelines[0].name, "mid");
    }
}
