use tower_lsp::lsp_types::{Location, SymbolInformation, Url};

use crate::convert::span_to_range;
use crate::document_symbols::def_kind_to_symbol_kind;
use crate::state::DefLocation;

/// Filter workspace definitions by a query string (case-insensitive substring match).
#[allow(deprecated)] // SymbolInformation::deprecated field
pub fn workspace_symbols(query: &str, defs: &[DefLocation]) -> Vec<SymbolInformation> {
    let query_lower = query.to_lowercase();
    defs.iter()
        .filter(|loc| query.is_empty() || loc.name.to_lowercase().contains(&query_lower))
        .filter_map(|loc| {
            let uri = Url::parse(&loc.uri).ok()?;
            Some(SymbolInformation {
                name: loc.name.clone(),
                kind: def_kind_to_symbol_kind(loc.kind),
                tags: None,
                deprecated: None,
                location: Location {
                    uri,
                    range: span_to_range(&loc.span),
                },
                container_name: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::model::{DefKind, Span};
    use tower_lsp::lsp_types::SymbolKind;

    fn make_def(name: &str, kind: DefKind) -> DefLocation {
        DefLocation {
            uri: "file:///test.sysml".to_string(),
            name: name.to_string(),
            kind,
            span: Span {
                start_row: 1,
                start_col: 1,
                end_row: 1,
                end_col: 10,
                start_byte: 0,
                end_byte: 9,
            },
            doc: None,
            super_type: None,
            qualified_name: None,
        }
    }

    #[test]
    fn empty_query_returns_all() {
        let defs = vec![
            make_def("Vehicle", DefKind::Part),
            make_def("Engine", DefKind::Part),
        ];
        let results = workspace_symbols("", &defs);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filters_by_substring() {
        let defs = vec![
            make_def("Vehicle", DefKind::Part),
            make_def("Engine", DefKind::Part),
            make_def("VehicleController", DefKind::Action),
        ];
        let results = workspace_symbols("Vehicle", &defs);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.name.contains("Vehicle")));
    }

    #[test]
    fn case_insensitive() {
        let defs = vec![
            make_def("Vehicle", DefKind::Part),
            make_def("Engine", DefKind::Part),
        ];
        let results = workspace_symbols("vehicle", &defs);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Vehicle");
    }

    #[test]
    fn no_match_returns_empty() {
        let defs = vec![make_def("Vehicle", DefKind::Part)];
        let results = workspace_symbols("Nonexistent", &defs);
        assert!(results.is_empty());
    }

    #[test]
    fn symbol_kind_preserved() {
        let defs = vec![
            make_def("Vehicle", DefKind::Part),
            make_def("Drive", DefKind::Action),
            make_def("FuelPort", DefKind::Port),
        ];
        let results = workspace_symbols("", &defs);
        let vehicle = results.iter().find(|s| s.name == "Vehicle").unwrap();
        assert_eq!(vehicle.kind, SymbolKind::CLASS);
        let drive = results.iter().find(|s| s.name == "Drive").unwrap();
        assert_eq!(drive.kind, SymbolKind::FUNCTION);
        let port = results.iter().find(|s| s.name == "FuelPort").unwrap();
        assert_eq!(port.kind, SymbolKind::INTERFACE);
    }
}
