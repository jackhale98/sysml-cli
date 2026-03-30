/// Unit conversion system for engineering quantities.
///
/// Provides conversion between common engineering units used in SysML models.
/// Supports mass, length, time, force, power, energy, temperature, and currency.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::sim::expr::EvalError;

/// Convert a value from one unit to another.
pub fn convert(value: f64, from: &str, to: &str) -> Result<f64, EvalError> {
    if from == to {
        return Ok(value);
    }

    let table = conversion_table();
    if let Some(&factor) = table.get(&(from.to_string(), to.to_string())) {
        return Ok(value * factor);
    }
    // Try inverse
    if let Some(&factor) = table.get(&(to.to_string(), from.to_string())) {
        if factor != 0.0 {
            return Ok(value / factor);
        }
    }

    Err(EvalError::new(format!(
        "no conversion from `{}` to `{}`",
        from, to
    )))
}

/// Get the canonical base unit for a given unit string.
pub fn base_unit(unit: &str) -> &str {
    match unit {
        "kg" | "g" | "mg" | "lb" | "oz" => "kg",
        "m" | "mm" | "cm" | "km" | "in" | "ft" | "yd" | "mi" => "m",
        "s" | "ms" | "min" | "hr" | "h" => "s",
        "N" | "kN" | "lbf" => "N",
        "W" | "kW" | "MW" | "hp" => "W",
        "J" | "kJ" | "MJ" | "cal" | "kcal" | "Wh" | "kWh" => "J",
        "Pa" | "kPa" | "MPa" | "bar" | "psi" | "atm" => "Pa",
        "K" | "C" | "F" => "K",
        "m/s" | "km/h" | "mph" | "kn" => "m/s",
        "rad" | "deg" => "rad",
        "USD" | "EUR" | "GBP" => "USD",
        _ => unit,
    }
}

/// Check if two units are compatible (same dimension).
pub fn compatible(unit_a: &str, unit_b: &str) -> bool {
    base_unit(unit_a) == base_unit(unit_b)
}

fn conversion_table() -> &'static HashMap<(String, String), f64> {
    static TABLE: OnceLock<HashMap<(String, String), f64>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = HashMap::new();
        let mut add = |from: &str, to: &str, factor: f64| {
            t.insert((from.to_string(), to.to_string()), factor);
        };

        // Mass
        add("kg", "g", 1000.0);
        add("kg", "mg", 1_000_000.0);
        add("kg", "lb", 2.20462);
        add("kg", "oz", 35.274);
        add("g", "mg", 1000.0);
        add("lb", "oz", 16.0);

        // Length
        add("m", "mm", 1000.0);
        add("m", "cm", 100.0);
        add("m", "km", 0.001);
        add("m", "in", 39.3701);
        add("m", "ft", 3.28084);
        add("m", "yd", 1.09361);
        add("km", "mi", 0.621371);
        add("in", "ft", 1.0 / 12.0);
        add("ft", "yd", 1.0 / 3.0);

        // Time
        add("s", "ms", 1000.0);
        add("s", "min", 1.0 / 60.0);
        add("min", "hr", 1.0 / 60.0);
        add("hr", "h", 1.0);
        add("s", "hr", 1.0 / 3600.0);

        // Force
        add("N", "kN", 0.001);
        add("N", "lbf", 0.224809);

        // Power
        add("W", "kW", 0.001);
        add("W", "MW", 0.000001);
        add("W", "hp", 1.0 / 745.7);
        add("kW", "hp", 1.34102);

        // Energy
        add("J", "kJ", 0.001);
        add("J", "MJ", 0.000001);
        add("J", "cal", 0.239006);
        add("J", "Wh", 1.0 / 3600.0);
        add("kJ", "kcal", 0.239006);
        add("kWh", "MJ", 3.6);

        // Pressure
        add("Pa", "kPa", 0.001);
        add("Pa", "MPa", 0.000001);
        add("Pa", "bar", 0.00001);
        add("Pa", "psi", 0.000145038);
        add("Pa", "atm", 1.0 / 101325.0);
        add("bar", "psi", 14.5038);

        // Velocity
        add("m/s", "km/h", 3.6);
        add("m/s", "mph", 2.23694);
        add("km/h", "mph", 0.621371);

        // Angle
        add("rad", "deg", 180.0 / std::f64::consts::PI);

        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_conversion() {
        assert_eq!(convert(100.0, "kg", "kg").unwrap(), 100.0);
    }

    #[test]
    fn kg_to_g() {
        assert!((convert(1.0, "kg", "g").unwrap() - 1000.0).abs() < 0.01);
    }

    #[test]
    fn g_to_kg() {
        assert!((convert(1000.0, "g", "kg").unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn m_to_mm() {
        assert!((convert(1.0, "m", "mm").unwrap() - 1000.0).abs() < 0.01);
    }

    #[test]
    fn m_to_ft() {
        assert!((convert(1.0, "m", "ft").unwrap() - 3.28084).abs() < 0.01);
    }

    #[test]
    fn w_to_kw() {
        assert!((convert(1500.0, "W", "kW").unwrap() - 1.5).abs() < 0.001);
    }

    #[test]
    fn kw_to_hp() {
        assert!((convert(1.0, "kW", "hp").unwrap() - 1.341).abs() < 0.01);
    }

    #[test]
    fn pa_to_psi() {
        assert!((convert(101325.0, "Pa", "psi").unwrap() - 14.696).abs() < 0.01);
    }

    #[test]
    fn unknown_units_error() {
        assert!(convert(1.0, "foo", "bar").is_err());
    }

    #[test]
    fn compatible_same_dimension() {
        assert!(compatible("kg", "lb"));
        assert!(compatible("m", "ft"));
        assert!(compatible("W", "hp"));
    }

    #[test]
    fn incompatible_different_dimension() {
        assert!(!compatible("kg", "m"));
        assert!(!compatible("W", "Pa"));
    }

    #[test]
    fn rad_to_deg() {
        assert!((convert(std::f64::consts::PI, "rad", "deg").unwrap() - 180.0).abs() < 0.01);
    }

    #[test]
    fn s_to_hr() {
        assert!((convert(3600.0, "s", "hr").unwrap() - 1.0).abs() < 0.001);
    }
}
