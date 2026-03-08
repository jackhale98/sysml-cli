/// SSP (System Structure and Parameterization) XML generation.
///
/// Generates SystemStructureDescription 1.0 XML from SysML v2
/// part usages and connection definitions.

use serde::Serialize;

use crate::model::Model;

/// A component in the SSP structure.
#[derive(Debug, Clone, Serialize)]
pub struct SspComponent {
    pub name: String,
    pub type_name: String,
}

/// A connection between components.
#[derive(Debug, Clone, Serialize)]
pub struct SspConnection {
    pub name: Option<String>,
    pub start_element: String,
    pub start_connector: String,
    pub end_element: String,
    pub end_connector: String,
}

/// The complete SSP system structure.
#[derive(Debug, Clone, Serialize)]
pub struct SspStructure {
    pub components: Vec<SspComponent>,
    pub connections: Vec<SspConnection>,
}

/// Extract SSP structure from a SysML model.
///
/// Finds part usages (components) and connections, splitting dotted
/// references into element.connector pairs.
pub fn extract_ssp_structure(model: &Model) -> SspStructure {
    let mut components = Vec::new();
    let mut connections = Vec::new();

    // Part usages are components
    for usage in &model.usages {
        if usage.kind == "part" {
            if let Some(ref type_ref) = usage.type_ref {
                components.push(SspComponent {
                    name: usage.name.clone(),
                    type_name: type_ref.clone(),
                });
            }
        }
    }

    // Connections map to SSP connections
    for conn in &model.connections {
        let src_parts: Vec<&str> = conn.source.split('.').collect();
        let tgt_parts: Vec<&str> = conn.target.split('.').collect();

        connections.push(SspConnection {
            name: conn.name.clone(),
            start_element: src_parts[0].to_string(),
            start_connector: src_parts.get(1).unwrap_or(&src_parts[0]).to_string(),
            end_element: tgt_parts[0].to_string(),
            end_connector: tgt_parts.get(1).unwrap_or(&tgt_parts[0]).to_string(),
        });
    }

    SspStructure {
        components,
        connections,
    }
}

/// Generate SSP SystemStructureDescription XML from a structure.
pub fn generate_ssd_xml(structure: &SspStructure) -> String {
    let mut lines = Vec::new();

    lines.push(r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string());
    lines.push(
        "<ssd:SystemStructureDescription version=\"1.0\" name=\"system\"\n\
             \x20   xmlns:ssd=\"http://ssp-standard.org/SSP1/SystemStructureDescription\">"
            .to_string(),
    );
    lines.push("  <ssd:System name=\"root\">".to_string());

    // Elements
    lines.push("    <ssd:Elements>".to_string());
    for comp in &structure.components {
        lines.push(format!(
            "      <ssd:Component name=\"{}\" source=\"resources/{}.fmu\">",
            comp.name,
            comp.type_name.to_lowercase()
        ));
        lines.push("        <ssd:Connectors>".to_string());

        // Find connectors for this component
        let mut seen = std::collections::HashSet::new();
        for conn in &structure.connections {
            if conn.start_element == comp.name {
                if seen.insert(conn.start_connector.clone()) {
                    lines.push(format!(
                        "          <ssd:Connector name=\"{}\" kind=\"output\"/>",
                        conn.start_connector
                    ));
                }
            }
            if conn.end_element == comp.name {
                if seen.insert(conn.end_connector.clone()) {
                    lines.push(format!(
                        "          <ssd:Connector name=\"{}\" kind=\"input\"/>",
                        conn.end_connector
                    ));
                }
            }
        }
        lines.push("        </ssd:Connectors>".to_string());
        lines.push("      </ssd:Component>".to_string());
    }
    lines.push("    </ssd:Elements>".to_string());

    // Connections
    lines.push("    <ssd:Connections>".to_string());
    for conn in &structure.connections {
        lines.push(format!(
            "      <ssd:Connection startElement=\"{}\" startConnector=\"{}\" \
             endElement=\"{}\" endConnector=\"{}\"/>",
            conn.start_element, conn.start_connector, conn.end_element, conn.end_connector
        ));
    }
    lines.push("    </ssd:Connections>".to_string());
    lines.push("  </ssd:System>".to_string());
    lines.push("</ssd:SystemStructureDescription>".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    const FIXTURE: &str = include_str!("../../test/fixtures/fmi-vehicle.sysml");

    #[test]
    fn extract_vehicle_ssp() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let ssp = extract_ssp_structure(&model);

        // VehicleSystem has: part engine : Engine, part transmission : Transmission
        let names: Vec<&str> = ssp.components.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"engine"), "Should find engine component");
        assert!(names.contains(&"transmission"), "Should find transmission component");

        // connection engineToDrive connect engine.driveOut to transmission.driveIn
        assert_eq!(ssp.connections.len(), 1);
        let conn = &ssp.connections[0];
        assert_eq!(conn.start_element, "engine");
        assert_eq!(conn.start_connector, "driveOut");
        assert_eq!(conn.end_element, "transmission");
        assert_eq!(conn.end_connector, "driveIn");
    }

    #[test]
    fn generate_ssd_xml_valid() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let ssp = extract_ssp_structure(&model);
        let xml = generate_ssd_xml(&ssp);

        assert!(xml.contains("<?xml version="));
        assert!(xml.contains("ssd:SystemStructureDescription"));
        assert!(xml.contains("ssd:Component name=\"engine\""));
        assert!(xml.contains("ssd:Connection"));
        assert!(xml.contains("startElement=\"engine\""));
        assert!(xml.contains("endElement=\"transmission\""));
    }
}
