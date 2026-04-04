/// Generic rollup engine for SysML v2 part hierarchies.
///
/// Aggregates attribute values across the composition tree using
/// configurable methods: sum, RSS, product, min, max.

use crate::model::Model;
use crate::sim::resolve::{resolve_attribute_tree, AttributeNode};

/// Aggregation method for rollup computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggregationMethod {
    /// Sum of (child_value × quantity)
    Sum,
    /// Root-sum-of-squares: sqrt(Σ (child_value × quantity)²)
    Rss,
    /// Product of child values
    Product,
    /// Minimum value across children
    Min,
    /// Maximum value across children
    Max,
}

impl AggregationMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sum" => Some(Self::Sum),
            "rss" => Some(Self::Rss),
            "product" | "prod" => Some(Self::Product),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Rss => "rss",
            Self::Product => "product",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// Per-child contribution in a rollup result.
#[derive(Debug, Clone)]
pub struct Contribution {
    /// Path from root (e.g., ["engine", "pistons"])
    pub path: Vec<String>,
    /// Definition type name
    pub definition: String,
    /// Quantity (from multiplicity)
    pub quantity: u32,
    /// Own value (before multiplication by quantity)
    pub own_value: f64,
    /// Subtotal: own_value × quantity + children's subtotals
    pub subtotal: f64,
    /// Percentage of total
    pub percentage: f64,
    /// Child contributions
    pub children: Vec<Contribution>,
}

/// Result of a rollup computation.
#[derive(Debug, Clone)]
pub struct RollupResult {
    /// Root definition name
    pub root: String,
    /// Attribute name
    pub attribute: String,
    /// Aggregation method used
    pub method: AggregationMethod,
    /// Total computed value
    pub total: f64,
    /// Root's own value (not from children)
    pub own_value: f64,
    /// Per-child contribution breakdown
    pub contributions: Vec<Contribution>,
}

/// Evaluate a rollup on a model.
pub fn evaluate_rollup(
    model: &Model,
    root_def: &str,
    attribute_name: &str,
    method: AggregationMethod,
) -> RollupResult {
    let tree = resolve_attribute_tree(model, root_def, attribute_name);
    let own = tree.own_value.unwrap_or(0.0);
    let (child_total, contributions) =
        aggregate_children(&tree.children, method, &[]);
    let total = own + child_total;

    // Compute percentages
    let contributions = set_percentages(contributions, total);

    RollupResult {
        root: tree.root,
        attribute: tree.attribute,
        method,
        total,
        own_value: own,
        contributions,
    }
}

fn aggregate_children(
    children: &[AttributeNode],
    method: AggregationMethod,
    parent_path: &[String],
) -> (f64, Vec<Contribution>) {
    let mut contributions = Vec::new();
    let mut values: Vec<f64> = Vec::new();

    for child in children {
        let mut path = parent_path.to_vec();
        path.push(child.name.clone());

        let own = child.own_value.unwrap_or(0.0);
        let (child_sum, child_contribs) =
            aggregate_children(&child.children, method, &path);
        let subtotal = (own + child_sum) * child.quantity as f64;

        values.push(subtotal);

        contributions.push(Contribution {
            path,
            definition: child.definition.clone(),
            quantity: child.quantity,
            own_value: own,
            subtotal,
            percentage: 0.0, // filled in later
            children: child_contribs,
        });
    }

    let total = match method {
        AggregationMethod::Sum => values.iter().sum(),
        AggregationMethod::Rss => {
            let sum_sq: f64 = values.iter().map(|v| v * v).sum();
            sum_sq.sqrt()
        }
        AggregationMethod::Product => {
            if values.is_empty() {
                0.0
            } else {
                values.iter().product()
            }
        }
        AggregationMethod::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        AggregationMethod::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    };

    // Handle empty case for min/max
    let total = if values.is_empty() { 0.0 } else { total };

    (total, contributions)
}

fn set_percentages(mut contributions: Vec<Contribution>, total: f64) -> Vec<Contribution> {
    for c in &mut contributions {
        c.percentage = if total > 0.0 {
            (c.subtotal / total) * 100.0
        } else {
            0.0
        };
        c.children = set_percentages(std::mem::take(&mut c.children), total);
    }
    contributions
}

/// Format a rollup result as a human-readable table.
pub fn format_rollup_text(result: &RollupResult) -> String {
    let mut out = format!(
        "Rollup: {} ({}) for {}\n",
        result.attribute,
        result.method.label(),
        result.root
    );
    out.push_str(&format!(
        "  {} {:>40} {:.4}\n",
        result.root, "total:", result.total
    ));
    if result.own_value != 0.0 {
        out.push_str(&format!(
            "    (own) {:>38} {:.4}\n",
            "", result.own_value
        ));
    }
    for c in &result.contributions {
        format_contribution(&mut out, c, 2);
    }
    out
}

fn format_contribution(out: &mut String, c: &Contribution, indent: usize) {
    let prefix = "  ".repeat(indent);
    let qty_str = if c.quantity > 1 {
        format!("[{}]", c.quantity)
    } else {
        String::new()
    };
    let own_str = if c.own_value != 0.0 {
        format!("{:.4}", c.own_value)
    } else {
        "-".to_string()
    };
    out.push_str(&format!(
        "{}{} : {} {} {:>8} => {:.4} ({:.1}%)\n",
        prefix, c.name(), c.definition, qty_str, own_str, c.subtotal, c.percentage
    ));
    for child in &c.children {
        format_contribution(out, child, indent + 1);
    }
}

impl Contribution {
    fn name(&self) -> &str {
        self.path.last().map(|s| s.as_str()).unwrap_or("?")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn sum_rollup_flat() {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Chassis { attribute mass : Real = 250; }
            part def Vehicle {
                attribute mass : Real = 50;
                part engine : Engine;
                part chassis : Chassis;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Vehicle", "mass", AggregationMethod::Sum);
        // 50 (own) + 180 (engine) + 250 (chassis) = 480
        assert_eq!(result.total, 480.0);
        assert_eq!(result.own_value, 50.0);
        assert_eq!(result.contributions.len(), 2);
    }

    #[test]
    fn sum_rollup_with_multiplicity() {
        let source = r#"
            part def Wheel { attribute mass : Real = 12.5; }
            part def Vehicle {
                part wheels : Wheel [4];
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Vehicle", "mass", AggregationMethod::Sum);
        // 4 * 12.5 = 50
        assert_eq!(result.total, 50.0);
        assert_eq!(result.contributions[0].quantity, 4);
        assert_eq!(result.contributions[0].own_value, 12.5);
        assert_eq!(result.contributions[0].subtotal, 50.0);
    }

    #[test]
    fn sum_rollup_nested() {
        let source = r#"
            part def Piston { attribute mass : Real = 0.5; }
            part def Engine {
                attribute mass : Real = 100;
                part pistons : Piston [4];
            }
            part def Vehicle {
                attribute mass : Real = 200;
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Vehicle", "mass", AggregationMethod::Sum);
        // 200 (own) + (100 + 4*0.5) * 1 = 200 + 102 = 302
        assert_eq!(result.total, 302.0);
    }

    #[test]
    fn rss_rollup() {
        let source = r#"
            part def A { attribute tolerance : Real = 3; }
            part def B { attribute tolerance : Real = 4; }
            part def Assembly {
                part a : A;
                part b : B;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Assembly", "tolerance", AggregationMethod::Rss);
        // sqrt(3^2 + 4^2) = 5
        assert_eq!(result.total, 5.0);
    }

    #[test]
    fn min_rollup() {
        let source = r#"
            part def A { attribute reliability : Real = 0.95; }
            part def B { attribute reliability : Real = 0.99; }
            part def System {
                part a : A;
                part b : B;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "System", "reliability", AggregationMethod::Min);
        assert!((result.total - 0.95).abs() < 0.001);
    }

    #[test]
    fn max_rollup() {
        let source = r#"
            part def A { attribute power : Real = 100; }
            part def B { attribute power : Real = 250; }
            part def System {
                part a : A;
                part b : B;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "System", "power", AggregationMethod::Max);
        assert_eq!(result.total, 250.0);
    }

    #[test]
    fn contribution_percentages() {
        let source = r#"
            part def A { attribute mass : Real = 75; }
            part def B { attribute mass : Real = 25; }
            part def System {
                part a : A;
                part b : B;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "System", "mass", AggregationMethod::Sum);
        assert_eq!(result.total, 100.0);
        assert!((result.contributions[0].percentage - 75.0).abs() < 0.01);
        assert!((result.contributions[1].percentage - 25.0).abs() < 0.01);
    }

    #[test]
    fn empty_tree_returns_zero() {
        let source = "part def Empty;\n";
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Empty", "mass", AggregationMethod::Sum);
        assert_eq!(result.total, 0.0);
        assert!(result.contributions.is_empty());
    }

    #[test]
    fn format_text_output() {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Vehicle {
                attribute mass : Real = 50;
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Vehicle", "mass", AggregationMethod::Sum);
        let text = format_rollup_text(&result);
        assert!(text.contains("Rollup: mass (sum) for Vehicle"));
        assert!(text.contains("230")); // total
        assert!(text.contains("engine"));
    }

    #[test]
    fn full_vehicle_example() {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Chassis { attribute mass : Real = 250; }
            part def Wheel { attribute mass : Real = 12.5; }
            part def Body { attribute mass : Real = 400; }
            part def Vehicle {
                attribute mass : Real = 20;
                part engine : Engine;
                part chassis : Chassis;
                part wheels : Wheel [4];
                part body : Body;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let result = evaluate_rollup(&model, "Vehicle", "mass", AggregationMethod::Sum);
        // 20 + 180 + 250 + 4*12.5 + 400 = 900
        assert_eq!(result.total, 900.0);
        assert_eq!(result.contributions.len(), 4);
    }
}
