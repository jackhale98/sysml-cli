use sysml_core::checks;
use sysml_core::diagnostic::Severity;
use sysml_core::model::Model;
use tower_lsp_server::ls_types::{self as lsp_types, DiagnosticSeverity, NumberOrString};

use crate::convert::span_to_range;

/// Run all lint checks on a model and return LSP diagnostics.
/// `workspace_names` are definitions known from other workspace files,
/// injected into `model.resolved_imports` so cross-file types aren't flagged as unresolved.
pub fn compute_diagnostics(
    model: &Model,
    source: &str,
    workspace_names: &[String],
    external_refs: &[String],
) -> Vec<lsp_types::Diagnostic> {
    // Clone model so we can populate resolved_imports for cross-file awareness
    let mut model = model.clone();
    for name in workspace_names {
        if !model.resolved_imports.contains(name) {
            model.resolved_imports.push(name.clone());
        }
    }
    // Names referenced by other workspace files count as used (W001).
    for name in external_refs {
        if !model.external_references.contains(name) {
            model.external_references.push(name.clone());
        }
    }

    let checks = checks::all_checks();
    let mut result = Vec::new();

    for check in &checks {
        for d in check.run(&model) {
            let severity = match d.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Note => DiagnosticSeverity::INFORMATION,
            };

            let mut message = d.message.clone();
            if let Some(ref suggestion) = d.suggestion {
                message.push_str(&format!("\nSuggestion: {}", suggestion));
            }

            result.push(lsp_types::Diagnostic {
                range: span_to_range(&d.span, source),
                severity: Some(severity),
                code: Some(NumberOrString::String(d.code.to_string())),
                source: Some("sysml".to_string()),
                message,
                ..Default::default()
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn unresolved_type_produces_warning() {
        let source = "part def Vehicle;\npart car : Vehicel;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        let w004: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("W004".to_string())))
            .collect();
        assert!(!w004.is_empty(), "expected W004 unresolved type diagnostic");
        assert_eq!(w004[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(w004[0].source.as_deref(), Some("sysml"));
        assert_eq!(w004[0].range.start.line, 1);
    }

    #[test]
    fn duplicate_def_produces_error() {
        let source = "part def A;\npart def A;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        let e002: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("E002".to_string())))
            .collect();
        assert!(!e002.is_empty(), "expected E002 duplicate def diagnostic");
        assert_eq!(e002[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn unused_def_produces_information() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        let w001: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("W001".to_string())))
            .collect();
        assert!(!w001.is_empty(), "expected W001 unused def diagnostic");
        assert_eq!(w001[0].severity, Some(DiagnosticSeverity::INFORMATION));
    }

    #[test]
    fn valid_model_no_errors_or_warnings() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        let errors_and_warnings: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.severity == Some(DiagnosticSeverity::ERROR)
                    || d.severity == Some(DiagnosticSeverity::WARNING)
            })
            .collect();
        assert!(
            errors_and_warnings.is_empty(),
            "expected no errors or warnings, got: {:?}",
            errors_and_warnings
        );
    }

    #[test]
    fn suggestion_appended_to_message() {
        let source = "part def Vehicle;\npart car : Vehicel;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        let w004: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("W004".to_string())))
            .collect();
        assert!(!w004.is_empty());
        assert!(
            w004[0].message.contains("Suggestion:") || w004[0].message.contains("did you mean"),
            "expected suggestion in message, got: {}",
            w004[0].message
        );
    }

    #[test]
    fn span_conversion_correct_0based() {
        let source = "part def Vehicle;\npart car : Vehicel;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, source, &[], &[]);
        for d in &diags {
            assert!(d.range.start.line <= 1);
            assert!(d.range.end.line <= 1);
        }
    }

    #[test]
    fn workspace_names_suppress_unresolved_type() {
        // File B references Engine which is defined in another file
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("b.sysml", source);
        let workspace = vec!["Engine".to_string()];
        let diags = compute_diagnostics(&model, source, &workspace, &[]);
        let w004: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("W004".to_string())))
            .collect();
        assert!(
            w004.is_empty(),
            "workspace-defined Engine should not be flagged as unresolved, got: {:?}",
            w004.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn true_typo_still_flagged_with_workspace_names() {
        let source = "part def Vehicle {\n    part engine : Engin;\n}\n";
        let model = parse_file("b.sysml", source);
        let workspace = vec!["Engine".to_string()];
        let diags = compute_diagnostics(&model, source, &workspace, &[]);
        let w004: Vec<_> = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("W004".to_string())))
            .collect();
        assert!(
            !w004.is_empty(),
            "true typo Engin should still be flagged even with workspace names"
        );
    }
}
