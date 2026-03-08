/// Modelica partial model stub generation from FMI interface contracts.

use crate::export::fmi::FmiInterface;

/// Map FMI type to Modelica input connector type.
fn modelica_input_type(fmi_type: &str) -> &'static str {
    match fmi_type {
        "Float64" => "Modelica.Blocks.Interfaces.RealInput",
        "Int32" => "Modelica.Blocks.Interfaces.IntegerInput",
        "Boolean" => "Modelica.Blocks.Interfaces.BooleanInput",
        _ => "Modelica.Blocks.Interfaces.RealInput",
    }
}

/// Map FMI type to Modelica output connector type.
fn modelica_output_type(fmi_type: &str) -> &'static str {
    match fmi_type {
        "Float64" => "Modelica.Blocks.Interfaces.RealOutput",
        "Int32" => "Modelica.Blocks.Interfaces.IntegerOutput",
        "Boolean" => "Modelica.Blocks.Interfaces.BooleanOutput",
        _ => "Modelica.Blocks.Interfaces.RealOutput",
    }
}

/// Map SysML type to Modelica parameter type.
fn modelica_param_type(sysml_type: &str) -> &'static str {
    match sysml_type {
        "Real" | "ScalarValues::Real" => "Real",
        "Integer" | "ScalarValues::Integer" | "Natural" => "Integer",
        "Boolean" | "ScalarValues::Boolean" => "Boolean",
        "String" | "ScalarValues::String" => "String",
        _ => "Real",
    }
}

/// Generate a Modelica partial model stub from an FMI interface contract.
pub fn generate_modelica(interface: &FmiInterface) -> String {
    let mut lines = Vec::new();

    lines.push(format!("partial model {}", interface.part_name));
    lines.push(format!(
        "  \"Generated from SysML v2 part def {}\"",
        interface.part_name
    ));

    // Interface connectors
    for item in &interface.items {
        let mo_type = if item.direction == "out" || item.direction == "output" {
            modelica_output_type(&item.fmi_type)
        } else {
            modelica_input_type(&item.fmi_type)
        };
        lines.push(format!(
            "  {} {} \"From port {}\";",
            mo_type, item.name, item.source_port
        ));
    }

    // Attributes as parameters
    for attr in &interface.attributes {
        lines.push(format!(
            "  parameter {} {} \"From SysML attribute\";",
            modelica_param_type(&attr.sysml_type),
            attr.name
        ));
    }

    lines.push("equation".to_string());
    lines.push("  // Equations to be filled by model developer".to_string());
    lines.push(format!("end {};", interface.part_name));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::fmi;
    use crate::parser::parse_file;

    const FIXTURE: &str = include_str!("../../test/fixtures/fmi-vehicle.sysml");

    #[test]
    fn generate_engine_modelica() {
        let model = parse_file("fmi-vehicle.sysml", FIXTURE);
        let interface = fmi::extract_interface(&model, "Engine").unwrap();
        let mo = generate_modelica(&interface);

        assert!(mo.starts_with("partial model Engine"));
        assert!(mo.ends_with("end Engine;"));
        assert!(mo.contains("equation"));
        assert!(mo.contains("parameter Real displacement"));
        assert!(mo.contains("parameter Integer cylinders"));
        // All interface items from conjugated driveOut should be inputs
        assert!(mo.contains("RealInput"));
    }
}
