/// FMI 3.0 interface extraction from SysML v2 models.
///
/// Extracts port items from part definitions and maps them to FMI
/// variable contracts with type mapping and conjugation support.

use serde::Serialize;

use crate::model::{DefKind, Direction, Model};

/// Default SysML → FMI 3.0 type mapping.
const TYPE_MAP: &[(&str, &str)] = &[
    ("Real", "Float64"),
    ("Integer", "Int32"),
    ("Boolean", "Boolean"),
    ("String", "String"),
    ("ScalarValues::Real", "Float64"),
    ("ScalarValues::Integer", "Int32"),
    ("ScalarValues::Boolean", "Boolean"),
    ("ScalarValues::String", "String"),
    ("Natural", "Int32"),
];

fn map_type(sysml_type: &str) -> &'static str {
    TYPE_MAP
        .iter()
        .find(|(k, _)| *k == sysml_type)
        .map(|(_, v)| *v)
        .unwrap_or("Float64")
}

fn direction_to_causality(dir: Direction) -> &'static str {
    match dir {
        Direction::In => "input",
        Direction::Out => "output",
        Direction::InOut => "input",
    }
}

/// A single FMI interface item extracted from a SysML port.
#[derive(Debug, Clone, Serialize)]
pub struct FmiInterfaceItem {
    pub name: String,
    pub direction: String,
    pub sysml_type: String,
    pub fmi_type: String,
    pub causality: String,
    pub variability: String,
    pub source_port: String,
}

/// An attribute on a part definition.
#[derive(Debug, Clone, Serialize)]
pub struct FmiAttribute {
    pub name: String,
    pub sysml_type: String,
}

/// Complete FMI interface contract for a part definition.
#[derive(Debug, Clone, Serialize)]
pub struct FmiInterface {
    pub part_name: String,
    pub items: Vec<FmiInterfaceItem>,
    pub attributes: Vec<FmiAttribute>,
}

/// An exportable part and its interface summary.
#[derive(Debug, Clone, Serialize)]
pub struct ExportablePart {
    pub name: String,
    pub ports: usize,
    pub attributes: usize,
    pub connections: usize,
}

/// Extract the FMI interface contract for a named part definition.
pub fn extract_interface(model: &Model, part_name: &str) -> Result<FmiInterface, String> {
    // Find the part definition
    let _part_def = model
        .find_def(part_name)
        .filter(|d| d.kind == DefKind::Part)
        .ok_or_else(|| format!("part def `{}` not found", part_name))?;

    let mut items = Vec::new();
    let mut attributes = Vec::new();

    // Get all usages scoped to this part
    let part_usages = model.usages_in_def(part_name);

    for usage in &part_usages {
        match usage.kind.as_str() {
            "port" => {
                // Get port type name
                let port_type = match &usage.type_ref {
                    Some(t) => t.clone(),
                    None => continue,
                };
                let port_conjugated = usage.is_conjugated;

                // Find items inside the port definition
                let port_items = model.usages_in_def(&port_type);
                for item in port_items {
                    if item.kind != "item" {
                        continue;
                    }
                    let item_type = item
                        .type_ref
                        .as_deref()
                        .unwrap_or("Real");

                    // Determine effective direction with conjugation
                    let raw_dir = item.direction.unwrap_or(Direction::In);
                    let effective_dir = if port_conjugated {
                        raw_dir.conjugated()
                    } else {
                        raw_dir
                    };

                    let fmi_type = map_type(item_type);

                    items.push(FmiInterfaceItem {
                        name: item.name.clone(),
                        direction: effective_dir.label().to_string(),
                        sysml_type: item_type.to_string(),
                        fmi_type: fmi_type.to_string(),
                        causality: direction_to_causality(effective_dir).to_string(),
                        variability: "continuous".to_string(),
                        source_port: usage.name.clone(),
                    });
                }
            }
            "attribute" => {
                let attr_type = usage
                    .type_ref
                    .as_deref()
                    .unwrap_or("Real");
                attributes.push(FmiAttribute {
                    name: usage.name.clone(),
                    sysml_type: attr_type.to_string(),
                });
            }
            _ => {}
        }
    }

    Ok(FmiInterface {
        part_name: part_name.to_string(),
        items,
        attributes,
    })
}

/// List all part definitions that are exportable (have ports or attributes).
pub fn list_exportable(model: &Model) -> Vec<ExportablePart> {
    let mut parts = Vec::new();

    for def in &model.definitions {
        if def.kind != DefKind::Part {
            continue;
        }

        let usages = model.usages_in_def(&def.name);
        let ports = usages.iter().filter(|u| u.kind == "port").count();
        let attrs = usages.iter().filter(|u| u.kind == "attribute").count();

        // Count connections where this part is the container
        let connections = model
            .connections
            .iter()
            .filter(|c| {
                c.source.starts_with(&format!("{}.", def.name))
                    || c.target.starts_with(&format!("{}.", def.name))
                    || usages.iter().any(|u| u.kind == "part" && u.name == c.source.split('.').next().unwrap_or(""))
            })
            .count();

        if ports > 0 || attrs > 0 {
            parts.push(ExportablePart {
                name: def.name.clone(),
                ports,
                attributes: attrs,
                connections,
            });
        }
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    const FIXTURE: &str = include_str!("../../test/fixtures/fmi-vehicle.sysml");

    #[test]
    fn extract_engine_interface() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let interface = extract_interface(&model, "Engine").unwrap();

        // Engine has 3 ports: fuelIn (FuelPort), driveOut (~DrivePort), ignition (IgnitionPort)
        // FuelPort: in item fuelFlow : Real → 1 item
        // DrivePort: out item torque : Real, out item speed : Real → 2 items (conjugated → in)
        // IgnitionPort: in item ignitionOn : Boolean → 1 item
        assert_eq!(interface.items.len(), 4, "Engine should have 4 interface items");

        // Check conjugation: driveOut uses ~DrivePort, so out→in
        let drive_items: Vec<_> = interface
            .items
            .iter()
            .filter(|i| i.source_port == "driveOut")
            .collect();
        assert_eq!(drive_items.len(), 2);
        for item in &drive_items {
            assert_eq!(item.direction, "in", "Conjugated DrivePort items should be 'in'");
            assert_eq!(item.causality, "input");
        }

        // Check non-conjugated: fuelIn uses FuelPort (no ~)
        let fuel_items: Vec<_> = interface
            .items
            .iter()
            .filter(|i| i.source_port == "fuelIn")
            .collect();
        assert_eq!(fuel_items.len(), 1);
        assert_eq!(fuel_items[0].direction, "in");
        assert_eq!(fuel_items[0].name, "fuelFlow");
        assert_eq!(fuel_items[0].fmi_type, "Float64");

        // Check ignition
        let ign_items: Vec<_> = interface
            .items
            .iter()
            .filter(|i| i.source_port == "ignition")
            .collect();
        assert_eq!(ign_items.len(), 1);
        assert_eq!(ign_items[0].name, "ignitionOn");
        assert_eq!(ign_items[0].fmi_type, "Boolean");

        // Check attributes
        assert_eq!(interface.attributes.len(), 2);
        let displacement = interface.attributes.iter().find(|a| a.name == "displacement");
        assert!(displacement.is_some());
        assert_eq!(displacement.unwrap().sysml_type, "Real");

        let cylinders = interface.attributes.iter().find(|a| a.name == "cylinders");
        assert!(cylinders.is_some());
        assert_eq!(cylinders.unwrap().sysml_type, "Integer");
    }

    #[test]
    fn extract_transmission_interface() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let interface = extract_interface(&model, "Transmission").unwrap();

        // Transmission has 1 port: driveIn (DrivePort, no conjugation)
        assert_eq!(interface.items.len(), 2, "Transmission has 2 items from DrivePort");
        for item in &interface.items {
            assert_eq!(item.direction, "out", "DrivePort items are 'out' (no conjugation)");
        }

        assert_eq!(interface.attributes.len(), 1);
        assert_eq!(interface.attributes[0].name, "gearCount");
    }

    #[test]
    fn list_exportable_parts() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let parts = list_exportable(&model);

        let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Engine"), "Engine should be exportable");
        assert!(names.contains(&"Transmission"), "Transmission should be exportable");
    }

    #[test]
    fn nonexistent_part_errors() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let result = extract_interface(&model, "NonExistent");
        assert!(result.is_err());
    }
}
