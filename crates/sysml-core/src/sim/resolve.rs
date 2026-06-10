//! Attribute resolution across the SysML v2 part hierarchy.
//!
//! Walks the composition tree from a root definition, resolving named
//! attribute values on each part instance. Respects multiplicity (quantity),
//! inheritance (supertype), and value expressions.

use std::collections::HashSet;

use crate::model::{simple_name, Model};

/// A node in the resolved attribute tree.
#[derive(Debug, Clone)]
pub struct AttributeNode {
    /// Usage name (e.g., "engine", "wheels")
    pub name: String,
    /// Definition type name (e.g., "Engine", "Wheel")
    pub definition: String,
    /// Quantity from multiplicity (default 1)
    pub quantity: u32,
    /// Resolved value of the target attribute on this node (if any)
    pub own_value: Option<f64>,
    /// Child nodes (parts inside this definition)
    pub children: Vec<AttributeNode>,
}

/// The full resolved attribute tree from a root.
#[derive(Debug, Clone)]
pub struct AttributeTree {
    /// Root definition name
    pub root: String,
    /// Target attribute name
    pub attribute: String,
    /// Root's own attribute value (if any)
    pub own_value: Option<f64>,
    /// Child part nodes
    pub children: Vec<AttributeNode>,
}

/// Resolve an attribute tree starting from a root definition.
///
/// Walks the part usages inside `root_def`, recursively following type
/// references to find nested parts. For each part, looks for an attribute
/// usage or value expression matching `attribute_name`.
pub fn resolve_attribute_tree(
    model: &Model,
    root_def: &str,
    attribute_name: &str,
) -> AttributeTree {
    let own_value = find_attribute_value(model, root_def, attribute_name);
    let mut visited = HashSet::new();
    visited.insert(root_def.to_string());
    let children = resolve_children(model, root_def, attribute_name, &mut visited);

    AttributeTree {
        root: root_def.to_string(),
        attribute: attribute_name.to_string(),
        own_value,
        children,
    }
}

fn resolve_children(
    model: &Model,
    def_name: &str,
    attribute_name: &str,
    visited: &mut HashSet<String>,
) -> Vec<AttributeNode> {
    let mut nodes = Vec::new();

    for usage in model.usages_in_def(def_name) {
        // Skip non-part usages (attributes, actions, states, etc.)
        // We want structural composition: parts, items
        let is_part = matches!(
            usage.kind.as_str(),
            "part" | "item" | "connection" | "flow" | "allocation"
        );
        if !is_part {
            continue;
        }

        let type_name = usage
            .type_ref
            .as_deref()
            .map(simple_name)
            .unwrap_or(&usage.name);

        // Extract quantity from multiplicity
        let quantity = quantity_from_multiplicity(usage);

        // Resolve the attribute value on this usage's type
        let own_value = find_attribute_value(model, type_name, attribute_name);

        // Recurse into children (cycle detection)
        let children = if visited.contains(type_name) {
            Vec::new()
        } else {
            visited.insert(type_name.to_string());
            let c = resolve_children(model, type_name, attribute_name, visited);
            visited.remove(type_name);
            c
        };

        nodes.push(AttributeNode {
            name: usage.name.clone(),
            definition: type_name.to_string(),
            quantity,
            own_value,
            children,
        });
    }

    nodes
}

/// Find the value of a named attribute within a definition.
/// Checks attribute usages and feature usages with matching names.
pub fn find_attribute_value(model: &Model, def_name: &str, attr_name: &str) -> Option<f64> {
    // Check direct usages in this definition
    for usage in model.usages_in_def(def_name) {
        if usage.name == attr_name && matches!(usage.kind.as_str(), "attribute" | "feature") {
            if let Some(ref expr) = usage.value_expr {
                if let Ok(v) = expr.trim().parse::<f64>() {
                    return Some(v);
                }
            }
        }
    }

    // Check if the definition itself has a value_expr on a matching attribute
    // (for definitions that directly have default values in their body)
    for usage in &model.usages {
        if usage.parent_def.as_deref() == Some(def_name)
            && usage.name == attr_name
            && matches!(usage.kind.as_str(), "attribute" | "feature")
        {
            if let Some(ref expr) = usage.value_expr {
                if let Ok(v) = expr.trim().parse::<f64>() {
                    return Some(v);
                }
            }
        }
    }

    // Check supertype chain
    if let Some(def) = model.find_def(def_name) {
        if let Some(ref super_type) = def.super_type {
            let st = simple_name(super_type);
            if st != def_name {
                return find_attribute_value(model, st, attr_name);
            }
        }
    }

    None
}

/// Extract quantity from a usage's multiplicity (default 1).
fn quantity_from_multiplicity(usage: &crate::model::Usage) -> u32 {
    if let Some(ref mult) = usage.multiplicity {
        // If lower == upper and is a number, use that as exact count
        if let Some(ref lower) = mult.lower {
            if let Ok(n) = lower.parse::<u32>() {
                if mult.upper.is_none() {
                    return n; // exact multiplicity like [4]
                }
            }
        }
        if let Some(ref upper) = mult.upper {
            if mult.lower.is_none() {
                // [N] shorthand — upper only
                if let Ok(n) = upper.parse::<u32>() {
                    return n;
                }
            }
        }
        // For ranges like [1..*], default to lower bound
        if let Some(ref lower) = mult.lower {
            if let Ok(n) = lower.parse::<u32>() {
                return n;
            }
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn resolve_flat_attribute() {
        let source = r#"
            part def Vehicle {
                attribute mass : Real = 200;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Vehicle", "mass");
        assert_eq!(tree.root, "Vehicle");
        assert_eq!(tree.own_value, Some(200.0));
    }

    #[test]
    fn resolve_child_attributes() {
        let source = r#"
            part def Engine {
                attribute mass : Real = 180;
            }
            part def Vehicle {
                attribute mass : Real = 50;
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Vehicle", "mass");
        assert_eq!(tree.own_value, Some(50.0));
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].name, "engine");
        assert_eq!(tree.children[0].own_value, Some(180.0));
    }

    #[test]
    fn resolve_with_multiplicity() {
        let source = r#"
            part def Wheel {
                attribute mass : Real = 12.5;
            }
            part def Vehicle {
                part wheels : Wheel [4];
            }
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Vehicle", "mass");
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].quantity, 4);
        assert_eq!(tree.children[0].own_value, Some(12.5));
    }

    #[test]
    fn resolve_nested_hierarchy() {
        let source = r#"
            part def Piston {
                attribute mass : Real = 0.5;
            }
            part def Engine {
                attribute mass : Real = 100;
                part pistons : Piston [4];
            }
            part def Vehicle {
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Vehicle", "mass");
        assert_eq!(tree.children.len(), 1);
        let engine = &tree.children[0];
        assert_eq!(engine.name, "engine");
        assert_eq!(engine.own_value, Some(100.0));
        assert_eq!(engine.children.len(), 1);
        assert_eq!(engine.children[0].name, "pistons");
        assert_eq!(engine.children[0].quantity, 4);
        assert_eq!(engine.children[0].own_value, Some(0.5));
    }

    #[test]
    fn resolve_missing_attribute() {
        let source = r#"
            part def Vehicle {
                part engine : Engine;
            }
            part def Engine;
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Vehicle", "mass");
        assert_eq!(tree.own_value, None);
        assert_eq!(tree.children[0].own_value, None);
    }

    #[test]
    fn resolve_cycle_detection() {
        // A contains B, B contains A — should not infinite loop
        let source = r#"
            part def A {
                attribute mass : Real = 10;
                part b : B;
            }
            part def B {
                attribute mass : Real = 20;
                part a : A;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "A", "mass");
        assert_eq!(tree.own_value, Some(10.0));
        // B is resolved but A inside B is not re-expanded
        assert_eq!(tree.children[0].own_value, Some(20.0));
    }

    #[test]
    fn resolve_no_children() {
        let source = "part def Leaf { attribute cost : Real = 5; }\n";
        let model = parse_file("test.sysml", source);
        let tree = resolve_attribute_tree(&model, "Leaf", "cost");
        assert_eq!(tree.own_value, Some(5.0));
        assert!(tree.children.is_empty());
    }
}
