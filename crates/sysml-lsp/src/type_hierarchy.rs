use sysml_core::model::{simple_name, DefKind, Model};
use tower_lsp_server::ls_types::{TypeHierarchyItem, Uri};

use crate::convert::span_to_range;
use crate::document_symbols::def_kind_to_symbol_kind;

/// Build a TypeHierarchyItem for a definition by name.
pub fn prepare_type_hierarchy(
    model: &Model,
    source: &str,
    uri: &Uri,
    name: &str,
) -> Option<TypeHierarchyItem> {
    let def = model.find_def(name)?;
    Some(make_item(
        &def.name,
        def.kind,
        uri,
        &def.span,
        def.super_type.as_deref(),
        source,
    ))
}

/// Find supertypes of a definition (direct parent only in single-file mode,
/// or chain if models are available).
pub fn supertypes(models: &[(&str, &str, &Model)], name: &str) -> Vec<TypeHierarchyItem> {
    let target = simple_name(name);
    // Find the definition to get its supertype
    for (_uri_str, _source, model) in models {
        if let Some(def) = model.find_def(target) {
            if let Some(ref st) = def.super_type {
                let st_name = simple_name(st);
                // Find the supertype definition
                for (st_uri, st_source, st_model) in models {
                    if let Some(st_def) = st_model.find_def(st_name) {
                        if let Ok(uri) = st_uri.parse::<Uri>() {
                            return vec![make_item(
                                &st_def.name,
                                st_def.kind,
                                &uri,
                                &st_def.span,
                                st_def.super_type.as_deref(),
                                st_source,
                            )];
                        }
                    }
                }
            }
            break;
        }
    }
    Vec::new()
}

/// Find subtypes of a definition (direct children).
pub fn subtypes(models: &[(&str, &str, &Model)], name: &str) -> Vec<TypeHierarchyItem> {
    let target = simple_name(name);
    let mut result = Vec::new();

    for (uri_str, source, model) in models {
        for def in &model.definitions {
            if let Some(ref st) = def.super_type {
                if simple_name(st) == target {
                    if let Ok(uri) = uri_str.parse::<Uri>() {
                        result.push(make_item(
                            &def.name,
                            def.kind,
                            &uri,
                            &def.span,
                            def.super_type.as_deref(),
                            source,
                        ));
                    }
                }
            }
        }
    }

    result
}

fn make_item(
    name: &str,
    kind: DefKind,
    uri: &Uri,
    span: &sysml_core::model::Span,
    detail: Option<&str>,
    source: &str,
) -> TypeHierarchyItem {
    let range = span_to_range(span, source);
    TypeHierarchyItem {
        name: name.to_string(),
        kind: def_kind_to_symbol_kind(kind),
        tags: None,
        detail: detail.map(|s| format!(":> {}", s)),
        uri: uri.clone(),
        range,
        selection_range: range,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn prepare_finds_definition() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let uri = "file:///test.sysml".parse::<Uri>().unwrap();
        let item = prepare_type_hierarchy(&model, source, &uri, "Vehicle");
        assert!(item.is_some());
        assert_eq!(item.unwrap().name, "Vehicle");
    }

    #[test]
    fn prepare_returns_none_for_unknown() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let uri = "file:///test.sysml".parse::<Uri>().unwrap();
        assert!(prepare_type_hierarchy(&model, source, &uri, "Unknown").is_none());
    }

    #[test]
    fn supertypes_finds_parent() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let result = supertypes(&models, "Sub");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Base");
    }

    #[test]
    fn supertypes_empty_for_root() {
        let source = "part def Root;\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let result = supertypes(&models, "Root");
        assert!(result.is_empty());
    }

    #[test]
    fn subtypes_finds_children() {
        let source = "part def Base;\npart def Sub1 :> Base;\npart def Sub2 :> Base;\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let result = subtypes(&models, "Base");
        assert_eq!(result.len(), 2);
        let names: Vec<_> = result.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Sub1"));
        assert!(names.contains(&"Sub2"));
    }

    #[test]
    fn subtypes_empty_for_leaf() {
        let source = "part def Leaf;\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let result = subtypes(&models, "Leaf");
        assert!(result.is_empty());
    }

    #[test]
    fn cross_file_supertype() {
        let source_a = "part def Base;\n";
        let source_b = "part def Derived :> Base;\n";
        let model_a = parse_file("a.sysml", source_a);
        let model_b = parse_file("b.sysml", source_b);
        let models = vec![
            ("file:///a.sysml", source_a, &model_a),
            ("file:///b.sysml", source_b, &model_b),
        ];
        let result = supertypes(&models, "Derived");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Base");
        assert_eq!(result[0].uri.as_str(), "file:///a.sysml");
    }
}
