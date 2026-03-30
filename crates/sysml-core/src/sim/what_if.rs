/// What-if analysis and parametric sweep for rollup calculations.
///
/// Enables exploring how changes to individual attributes affect
/// rollup totals. Supports named scenarios with overrides and
/// parametric sweeps across a range of values.

use crate::model::Model;
use crate::sim::expr::Env;
use crate::sim::resolve::{resolve_attribute_tree, AttributeNode};
use crate::sim::rollup::{evaluate_rollup, AggregationMethod, RollupResult};

/// A named what-if scenario with attribute overrides.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    /// Overrides: (dotted_path, value). E.g., ("engine.mass", 200.0)
    pub overrides: Vec<(String, f64)>,
}

/// Result of a what-if comparison.
#[derive(Debug)]
pub struct WhatIfResult {
    pub attribute: String,
    pub method: AggregationMethod,
    pub root: String,
    pub baseline: f64,
    pub scenarios: Vec<ScenarioResult>,
}

#[derive(Debug)]
pub struct ScenarioResult {
    pub name: String,
    pub total: f64,
    pub delta: f64,
    pub delta_pct: f64,
}

/// Evaluate a rollup under multiple scenarios.
pub fn evaluate_what_if(
    model: &Model,
    root: &str,
    attr: &str,
    method: AggregationMethod,
    scenarios: &[Scenario],
) -> WhatIfResult {
    let baseline = evaluate_rollup(model, root, attr, method);

    let scenario_results: Vec<ScenarioResult> = scenarios
        .iter()
        .map(|scenario| {
            let total = evaluate_with_overrides(model, root, attr, method, &scenario.overrides);
            let delta = total - baseline.total;
            let delta_pct = if baseline.total != 0.0 {
                (delta / baseline.total) * 100.0
            } else {
                0.0
            };
            ScenarioResult {
                name: scenario.name.clone(),
                total,
                delta,
                delta_pct,
            }
        })
        .collect();

    WhatIfResult {
        attribute: attr.to_string(),
        method,
        root: root.to_string(),
        baseline: baseline.total,
        scenarios: scenario_results,
    }
}

/// Configuration for a parametric sweep.
#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// Dotted path to the parameter (e.g., "engine.mass").
    pub parameter: String,
    /// Start value.
    pub start: f64,
    /// End value.
    pub end: f64,
    /// Number of steps.
    pub steps: usize,
}

/// Result of a parametric sweep.
#[derive(Debug)]
pub struct SweepResult {
    pub attribute: String,
    pub parameter: String,
    pub root: String,
    pub points: Vec<(f64, f64)>,
    /// Approximate sensitivity: d(total)/d(parameter).
    pub sensitivity: f64,
}

/// Evaluate a rollup across a range of parameter values.
pub fn evaluate_sweep(
    model: &Model,
    root: &str,
    attr: &str,
    method: AggregationMethod,
    config: &SweepConfig,
) -> SweepResult {
    let steps = config.steps.max(1);
    let step_size = if steps > 1 {
        (config.end - config.start) / (steps - 1) as f64
    } else {
        0.0
    };

    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let param_value = config.start + step_size * i as f64;
        let overrides = vec![(config.parameter.clone(), param_value)];
        let total = evaluate_with_overrides(model, root, attr, method, &overrides);
        points.push((param_value, total));
    }

    // Compute sensitivity as finite difference at midpoint
    let sensitivity = if points.len() >= 2 {
        let first = &points[0];
        let last = &points[points.len() - 1];
        if (last.0 - first.0).abs() > f64::EPSILON {
            (last.1 - first.1) / (last.0 - first.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    SweepResult {
        attribute: attr.to_string(),
        parameter: config.parameter.clone(),
        root: root.to_string(),
        points,
        sensitivity,
    }
}

/// Evaluate a rollup with specific attribute overrides applied.
fn evaluate_with_overrides(
    model: &Model,
    root: &str,
    attr: &str,
    method: AggregationMethod,
    overrides: &[(String, f64)],
) -> f64 {
    let mut tree = resolve_attribute_tree(model, root, attr);

    // Apply overrides to the tree
    for (path, value) in overrides {
        apply_override_to_tree(&mut tree.children, path, *value);
        // Check if override is for root's own value
        if !path.contains('.') {
            if path == attr || path == root {
                tree.own_value = Some(*value);
            }
        }
    }

    // Re-aggregate manually
    let own = tree.own_value.unwrap_or(0.0);
    let child_total = aggregate_tree(&tree.children, method);
    own + child_total
}

fn apply_override_to_tree(nodes: &mut [AttributeNode], path: &str, value: f64) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    for node in nodes.iter_mut() {
        if node.name == parts[0] || node.definition == parts[0] {
            if parts.len() == 1 {
                // This is a leaf override — set the node's own_value
                node.own_value = Some(value);
            } else {
                // Recurse into children with remaining path
                let remaining = parts[1..].join(".");
                apply_override_to_tree(&mut node.children, &remaining, value);
            }
        }
    }
}

fn aggregate_tree(nodes: &[AttributeNode], method: AggregationMethod) -> f64 {
    let values: Vec<f64> = nodes
        .iter()
        .map(|n| {
            let own = n.own_value.unwrap_or(0.0);
            let children = aggregate_tree(&n.children, method);
            (own + children) * n.quantity as f64
        })
        .collect();

    match method {
        AggregationMethod::Sum => values.iter().sum(),
        AggregationMethod::Rss => {
            let sum_sq: f64 = values.iter().map(|v| v * v).sum();
            sum_sq.sqrt()
        }
        AggregationMethod::Product => {
            if values.is_empty() { 0.0 } else { values.iter().product() }
        }
        AggregationMethod::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        AggregationMethod::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    fn vehicle_model() -> Model {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Chassis { attribute mass : Real = 250; }
            part def Wheel { attribute mass : Real = 12.5; }
            part def Vehicle {
                attribute mass : Real = 20;
                part engine : Engine;
                part chassis : Chassis;
                part wheels : Wheel [4];
            }
        "#;
        parse_file("test.sysml", source)
    }

    #[test]
    fn what_if_baseline() {
        let model = vehicle_model();
        let result = evaluate_what_if(
            &model, "Vehicle", "mass", AggregationMethod::Sum, &[],
        );
        // 20 + 180 + 250 + 4*12.5 = 500
        assert_eq!(result.baseline, 500.0);
        assert!(result.scenarios.is_empty());
    }

    #[test]
    fn what_if_single_override() {
        let model = vehicle_model();
        let scenarios = vec![Scenario {
            name: "lighter engine".to_string(),
            overrides: vec![("engine".to_string(), 150.0)],
        }];
        let result = evaluate_what_if(
            &model, "Vehicle", "mass", AggregationMethod::Sum, &scenarios,
        );
        assert_eq!(result.baseline, 500.0);
        assert_eq!(result.scenarios.len(), 1);
        // 20 + 150 + 250 + 50 = 470
        assert_eq!(result.scenarios[0].total, 470.0);
        assert_eq!(result.scenarios[0].delta, -30.0);
    }

    #[test]
    fn what_if_multiple_scenarios() {
        let model = vehicle_model();
        let scenarios = vec![
            Scenario {
                name: "light".to_string(),
                overrides: vec![("engine".to_string(), 100.0)],
            },
            Scenario {
                name: "heavy".to_string(),
                overrides: vec![("engine".to_string(), 300.0)],
            },
        ];
        let result = evaluate_what_if(
            &model, "Vehicle", "mass", AggregationMethod::Sum, &scenarios,
        );
        assert!(result.scenarios[0].total < result.baseline);
        assert!(result.scenarios[1].total > result.baseline);
    }

    #[test]
    fn sweep_linear() {
        let model = vehicle_model();
        let config = SweepConfig {
            parameter: "engine".to_string(),
            start: 100.0,
            end: 300.0,
            steps: 5,
        };
        let result = evaluate_sweep(
            &model, "Vehicle", "mass", AggregationMethod::Sum, &config,
        );
        assert_eq!(result.points.len(), 5);
        // First point: engine=100, total = 20 + 100 + 250 + 50 = 420
        assert_eq!(result.points[0].0, 100.0);
        assert_eq!(result.points[0].1, 420.0);
        // Last point: engine=300, total = 20 + 300 + 250 + 50 = 620
        assert_eq!(result.points[4].0, 300.0);
        assert_eq!(result.points[4].1, 620.0);
        // Sensitivity: 200 change in param → 200 change in total = 1.0
        assert!((result.sensitivity - 1.0).abs() < 0.01);
    }

    #[test]
    fn sweep_single_step() {
        let model = vehicle_model();
        let config = SweepConfig {
            parameter: "engine".to_string(),
            start: 200.0,
            end: 200.0,
            steps: 1,
        };
        let result = evaluate_sweep(
            &model, "Vehicle", "mass", AggregationMethod::Sum, &config,
        );
        assert_eq!(result.points.len(), 1);
    }
}
