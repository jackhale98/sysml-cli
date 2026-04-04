/// Analysis case extraction and evaluation for SysML v2.
///
/// Extracts analysis case definitions (subject, objective, parameters,
/// return expression) from parsed models and provides evaluation support
/// for parametric studies and trade-off analysis.

use crate::model::{DefKind, Model, Span};
use crate::parser;

/// A parsed analysis case with its structural components.
#[derive(Debug, Clone)]
pub struct AnalysisCaseModel {
    /// Name of the analysis case definition or usage.
    pub name: String,
    /// The subject declaration (part being analyzed).
    pub subject: Option<SubjectDecl>,
    /// The objective declaration.
    pub objective: Option<ObjectiveDecl>,
    /// Input parameters (in attributes).
    pub parameters: Vec<Parameter>,
    /// Return declaration (the computed result).
    pub return_decl: Option<ReturnDecl>,
    /// Local attribute bindings (intermediate calculations).
    pub local_bindings: Vec<LocalBinding>,
    /// Alternatives (for trade studies — parts inside the analysis that specialize the subject).
    pub alternatives: Vec<Alternative>,
    /// Span in source.
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SubjectDecl {
    pub name: String,
    pub type_ref: Option<String>,
    pub value_binding: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ObjectiveDecl {
    pub name: String,
    pub kind: ObjectiveKind,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectiveKind {
    /// General objective (no maximize/minimize).
    General,
    /// Maximize the evaluation function.
    Maximize,
    /// Minimize the evaluation function.
    Minimize,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_ref: Option<String>,
    pub direction: ParameterDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterDirection {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone)]
pub struct ReturnDecl {
    pub name: String,
    pub type_ref: Option<String>,
    pub value_expr: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LocalBinding {
    pub name: String,
    pub type_ref: Option<String>,
    pub value_expr: String,
}

#[derive(Debug, Clone)]
pub struct Alternative {
    pub name: String,
    pub type_ref: Option<String>,
    /// Attribute overrides within this alternative.
    pub overrides: Vec<(String, String)>,
}

/// Extract all analysis case models from a source file.
pub fn extract_analysis_cases(file: &str, source: &str) -> Vec<AnalysisCaseModel> {
    let model = parser::parse_file(file, source);
    extract_analysis_cases_from_model(&model)
}

/// Extract analysis case models from an already-parsed Model.
pub fn extract_analysis_cases_from_model(model: &Model) -> Vec<AnalysisCaseModel> {
    let mut cases = Vec::new();

    for def in &model.definitions {
        if def.kind == DefKind::Analysis {
            cases.push(build_analysis_case(model, &def.name, &def.span));
        }
    }

    // Also check analysis usages (instances of analysis defs)
    for usage in &model.usages {
        if usage.kind == "analysis" {
            cases.push(build_analysis_case(model, &usage.name, &usage.span));
        }
    }

    cases
}

fn build_analysis_case(model: &Model, name: &str, span: &Span) -> AnalysisCaseModel {
    let mut subject = None;
    let mut objective = None;
    let mut parameters = Vec::new();
    let mut return_decl = None;
    let mut local_bindings = Vec::new();
    let mut alternatives = Vec::new();

    // Scan usages that are children of this analysis case
    for usage in &model.usages {
        if usage.parent_def.as_deref() != Some(name) {
            continue;
        }

        match usage.kind.as_str() {
            "subject" => {
                subject = Some(SubjectDecl {
                    name: usage.name.clone(),
                    type_ref: usage.type_ref.clone(),
                    value_binding: usage.value_expr.clone(),
                });
            }
            "objective" => {
                let kind = detect_objective_kind(usage);
                objective = Some(ObjectiveDecl {
                    name: usage.name.clone(),
                    kind,
                    doc: None, // Could extract from nested doc comment
                });
            }
            "return" => {
                return_decl = Some(ReturnDecl {
                    name: usage.name.clone(),
                    type_ref: usage.type_ref.clone(),
                    value_expr: usage.value_expr.clone(),
                });
            }
            "attribute" | "feature" => {
                if let Some(ref dir) = usage.direction {
                    // in/out parameter
                    let pd = match dir {
                        crate::model::Direction::In => ParameterDirection::In,
                        crate::model::Direction::Out => ParameterDirection::Out,
                        crate::model::Direction::InOut => ParameterDirection::InOut,
                    };
                    parameters.push(Parameter {
                        name: usage.name.clone(),
                        type_ref: usage.type_ref.clone(),
                        direction: pd,
                    });
                } else if let Some(ref expr) = usage.value_expr {
                    // Local binding (computed intermediate value)
                    local_bindings.push(LocalBinding {
                        name: usage.name.clone(),
                        type_ref: usage.type_ref.clone(),
                        value_expr: expr.clone(),
                    });
                }
            }
            "part" => {
                // Parts inside analysis case = alternatives (for trade studies)
                alternatives.push(Alternative {
                    name: usage.name.clone(),
                    type_ref: usage.type_ref.clone(),
                    overrides: collect_overrides(model, &usage.name),
                });
            }
            _ => {}
        }
    }

    AnalysisCaseModel {
        name: name.to_string(),
        subject,
        objective,
        parameters,
        return_decl,
        local_bindings,
        alternatives,
        span: span.clone(),
    }
}

fn detect_objective_kind(usage: &crate::model::Usage) -> ObjectiveKind {
    // Check type_ref for MaximizeObjective or MinimizeObjective
    if let Some(ref tr) = usage.type_ref {
        let simple = crate::model::simple_name(tr);
        if simple.contains("Maximize") {
            return ObjectiveKind::Maximize;
        }
        if simple.contains("Minimize") {
            return ObjectiveKind::Minimize;
        }
    }
    ObjectiveKind::General
}

fn collect_overrides(model: &Model, alt_name: &str) -> Vec<(String, String)> {
    let mut overrides = Vec::new();
    for usage in &model.usages {
        if usage.parent_def.as_deref() == Some(alt_name) {
            if let Some(ref val) = usage.value_expr {
                overrides.push((usage.name.clone(), val.clone()));
            }
        }
    }
    overrides
}

/// Format a summary of analysis cases found in a model.
pub fn format_analysis_list(cases: &[AnalysisCaseModel]) -> String {
    if cases.is_empty() {
        return "No analysis cases found.".to_string();
    }
    let mut out = String::new();
    for case in cases {
        out.push_str(&format!("analysis {}", case.name));
        if let Some(ref subj) = case.subject {
            out.push_str(&format!(
                " (subject: {}{})",
                subj.name,
                subj.type_ref
                    .as_ref()
                    .map(|t| format!(" : {}", t))
                    .unwrap_or_default()
            ));
        }
        out.push('\n');
        if let Some(ref obj) = case.objective {
            let kind_str = match obj.kind {
                ObjectiveKind::General => "",
                ObjectiveKind::Maximize => " [maximize]",
                ObjectiveKind::Minimize => " [minimize]",
            };
            out.push_str(&format!("  objective: {}{}\n", obj.name, kind_str));
        }
        for param in &case.parameters {
            let dir = match param.direction {
                ParameterDirection::In => "in",
                ParameterDirection::Out => "out",
                ParameterDirection::InOut => "inout",
            };
            out.push_str(&format!(
                "  {} {} {}\n",
                dir,
                param.name,
                param
                    .type_ref
                    .as_ref()
                    .map(|t| format!(": {}", t))
                    .unwrap_or_default()
            ));
        }
        if let Some(ref ret) = case.return_decl {
            out.push_str(&format!(
                "  return {}{}{}\n",
                ret.name,
                ret.type_ref
                    .as_ref()
                    .map(|t| format!(" : {}", t))
                    .unwrap_or_default(),
                ret.value_expr
                    .as_ref()
                    .map(|e| format!(" = {}", e))
                    .unwrap_or_default(),
            ));
        }
        if !case.alternatives.is_empty() {
            out.push_str(&format!(
                "  alternatives: {}\n",
                case.alternatives
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for binding in &case.local_bindings {
            out.push_str(&format!("  {} = {}\n", binding.name, binding.value_expr));
        }
    }
    out
}

// =========================================================================
// Evaluation
// =========================================================================

/// Result of evaluating an analysis case.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub name: String,
    pub subject_name: Option<String>,
    pub bindings: Vec<(String, f64)>,
    pub return_value: Option<f64>,
}

/// Result of a trade study evaluation.
#[derive(Debug, Clone)]
pub struct TradeResult {
    pub name: String,
    pub objective: ObjectiveKind,
    pub alternatives: Vec<AlternativeScore>,
    pub winner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlternativeScore {
    pub name: String,
    pub score: Option<f64>,
    pub overrides: Vec<(String, String)>,
}

/// Evaluate an analysis case using attribute values from the model.
/// Binds the subject's attributes, evaluates local bindings and return expr.
pub fn evaluate_analysis(
    model: &Model,
    case: &AnalysisCaseModel,
    extra_bindings: &crate::sim::expr::Env,
) -> AnalysisResult {
    use crate::sim::expr::Value;
    use crate::sim::resolve::find_attribute_value;

    let mut env = extra_bindings.clone();
    let mut computed_bindings = Vec::new();

    // Bind subject attributes if subject has a type
    if let Some(ref subj) = case.subject {
        if let Some(ref type_ref) = subj.type_ref {
            let type_name = crate::model::simple_name(type_ref);
            // Find all attribute values on the subject's type
            for usage in &model.usages {
                if usage.parent_def.as_deref() == Some(type_name)
                    && matches!(usage.kind.as_str(), "attribute" | "feature")
                {
                    if let Some(ref val_expr) = usage.value_expr {
                        if let Ok(v) = val_expr.trim().parse::<f64>() {
                            env.bind(usage.name.clone(), Value::Number(v));
                            // Also bind as subject.attr
                            env.bind(format!("{}.{}", subj.name, usage.name), Value::Number(v));
                        }
                    }
                }
            }
            // Also resolve via rollup for nested parts
            if let Some(val) = find_attribute_value(model, type_name, "mass") {
                env.bind("mass".to_string(), Value::Number(val));
            }
        }
    }

    // Evaluate local bindings in order
    for binding in &case.local_bindings {
        if let Ok(v) = binding.value_expr.trim().parse::<f64>() {
            env.bind(binding.name.clone(), Value::Number(v));
            computed_bindings.push((binding.name.clone(), v));
        } else if let Some(val) = env.get(&binding.value_expr).and_then(|v| v.as_number()) {
            env.bind(binding.name.clone(), Value::Number(val));
            computed_bindings.push((binding.name.clone(), val));
        }
    }

    // Evaluate return expression
    let return_value = case.return_decl.as_ref().and_then(|ret| {
        if let Some(ref expr) = ret.value_expr {
            if let Ok(v) = expr.trim().parse::<f64>() {
                return Some(v);
            }
            // Try looking up in env
            env.get(expr.trim()).and_then(|v| v.as_number())
        } else {
            None
        }
    });

    AnalysisResult {
        name: case.name.clone(),
        subject_name: case.subject.as_ref().map(|s| s.name.clone()),
        bindings: computed_bindings,
        return_value,
    }
}

/// Evaluate a trade study: score each alternative and pick the best.
pub fn evaluate_trade_study(
    _model: &Model,
    case: &AnalysisCaseModel,
) -> TradeResult {
    let objective = case.objective.as_ref()
        .map(|o| o.kind.clone())
        .unwrap_or(ObjectiveKind::General);

    let alt_scores: Vec<AlternativeScore> = case.alternatives.iter().map(|alt| {
        // Try to compute a score from overrides
        // Look for numeric overrides that could serve as evaluation criteria
        let score = alt.overrides.iter()
            .find(|(k, _)| k.contains("cost") || k.contains("mass") || k.contains("eval"))
            .and_then(|(_, v)| v.trim().parse::<f64>().ok());

        AlternativeScore {
            name: alt.name.clone(),
            score,
            overrides: alt.overrides.clone(),
        }
    }).collect();

    let winner = match objective {
        ObjectiveKind::Maximize => {
            alt_scores.iter()
                .filter_map(|a| a.score.map(|s| (a.name.clone(), s)))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(name, _)| name)
        }
        ObjectiveKind::Minimize => {
            alt_scores.iter()
                .filter_map(|a| a.score.map(|s| (a.name.clone(), s)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(name, _)| name)
        }
        ObjectiveKind::General => None,
    };

    TradeResult {
        name: case.name.clone(),
        objective,
        alternatives: alt_scores,
        winner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_analysis_case() {
        let source = r#"
            part def V { attribute mass : Real = 100; }
            analysis def MassAnalysis {
                subject v : V;
                objective obj;
                return totalMass : Real;
            }
        "#;
        let cases = extract_analysis_cases("test.sysml", source);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "MassAnalysis");
        assert!(cases[0].subject.is_some());
        assert_eq!(cases[0].subject.as_ref().unwrap().name, "v");
        assert_eq!(
            cases[0].subject.as_ref().unwrap().type_ref.as_deref(),
            Some("V")
        );
        assert!(cases[0].objective.is_some());
        assert!(cases[0].return_decl.is_some());
    }

    #[test]
    fn extract_analysis_with_parameters() {
        let source = r#"
            analysis def FuelAnalysis {
                subject vehicle : Vehicle;
                in attribute scenario : Scenario;
                attribute distance : Real = 100;
                return fuelEconomy : Real;
            }
        "#;
        let cases = extract_analysis_cases("test.sysml", source);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].parameters.len(), 1);
        assert_eq!(cases[0].parameters[0].name, "scenario");
        assert_eq!(cases[0].parameters[0].direction, ParameterDirection::In);
        assert_eq!(cases[0].local_bindings.len(), 1);
        assert_eq!(cases[0].local_bindings[0].name, "distance");
    }

    #[test]
    fn extract_trade_study_with_alternatives() {
        let source = r#"
            part def Engine { attribute mass : Real; }
            analysis def EngineTradeOff {
                subject engineAlternatives : Engine;
                objective : MaximizeObjective;
                part engine4cyl : Engine;
                part engine6cyl : Engine;
            }
        "#;
        let model = parser::parse_file("test.sysml", source);
        let cases = extract_analysis_cases_from_model(&model);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].alternatives.len(), 2);
        assert!(cases[0]
            .alternatives
            .iter()
            .any(|a| a.name == "engine4cyl"));
        assert!(cases[0]
            .alternatives
            .iter()
            .any(|a| a.name == "engine6cyl"));
        assert_eq!(
            cases[0].objective.as_ref().unwrap().kind,
            ObjectiveKind::Maximize
        );
    }

    #[test]
    fn extract_minimize_objective() {
        let source = r#"
            analysis def CostAnalysis {
                subject system : System;
                objective : MinimizeObjective;
                return cost : Real;
            }
        "#;
        let cases = extract_analysis_cases("test.sysml", source);
        assert_eq!(cases.len(), 1);
        assert_eq!(
            cases[0].objective.as_ref().unwrap().kind,
            ObjectiveKind::Minimize
        );
    }

    #[test]
    fn no_analysis_cases() {
        let source = "part def Vehicle { part engine : Engine; }\n";
        let cases = extract_analysis_cases("test.sysml", source);
        assert!(cases.is_empty());
    }

    #[test]
    fn format_list_output() {
        let source = r#"
            analysis def MyAnalysis {
                subject v : Vehicle;
                objective obj;
                in attribute speed : Real;
                return result : Real;
            }
        "#;
        let cases = extract_analysis_cases("test.sysml", source);
        let text = format_analysis_list(&cases);
        assert!(text.contains("MyAnalysis"));
        assert!(text.contains("subject: v : Vehicle"));
        assert!(text.contains("objective:"));
        assert!(text.contains("in speed"));
        assert!(text.contains("return result"));
    }

    #[test]
    fn analysis_usage_extracted() {
        let source = r#"
            analysis def FuelStudy {
                subject v : Vehicle;
                return fuel : Real;
            }
            part context {
                analysis myStudy : FuelStudy;
            }
        "#;
        let cases = extract_analysis_cases("test.sysml", source);
        // Should find both the def and the usage
        assert!(cases.len() >= 1);
        assert!(cases.iter().any(|c| c.name == "FuelStudy"));
    }

    // --- Evaluation tests ---

    #[test]
    fn evaluate_with_subject_bindings() {
        let source = r#"
            part def Vehicle {
                attribute mass : Real = 1500;
                attribute power : Real = 200;
            }
            analysis def PowerToWeight {
                subject v : Vehicle;
                attribute ratio : Real = 7.5;
                return result : Real;
            }
        "#;
        let model = parser::parse_file("test.sysml", source);
        let cases = extract_analysis_cases_from_model(&model);
        let case = &cases[0];
        let env = crate::sim::expr::Env::new();
        let result = evaluate_analysis(&model, case, &env);
        assert_eq!(result.name, "PowerToWeight");
        // Should have resolved local binding
        assert!(!result.bindings.is_empty() || result.return_value.is_some(),
            "should have computed something: bindings={:?}, return={:?}",
            result.bindings, result.return_value);
    }

    #[test]
    fn evaluate_with_extra_bindings() {
        let source = r#"
            part def System;
            analysis def CostAnalysis {
                subject s : System;
                return totalCost : Real;
            }
        "#;
        let model = parser::parse_file("test.sysml", source);
        let cases = extract_analysis_cases_from_model(&model);
        let case = &cases[0];
        let mut env = crate::sim::expr::Env::new();
        env.bind("totalCost", crate::sim::expr::Value::Number(42.0));
        let result = evaluate_analysis(&model, case, &env);
        assert_eq!(result.name, "CostAnalysis");
    }

    #[test]
    fn trade_study_with_maximize() {
        let source = r#"
            part def Engine { attribute mass : Real; attribute cost : Real; }
            analysis def EngineStudy {
                subject e : Engine;
                objective : MaximizeObjective;
                part engine4cyl : Engine;
                part engine6cyl : Engine;
            }
        "#;
        let model = parser::parse_file("test.sysml", source);
        let cases = extract_analysis_cases_from_model(&model);
        let case = &cases[0];
        let result = evaluate_trade_study(&model, case);
        assert_eq!(result.alternatives.len(), 2);
        assert_eq!(result.objective, ObjectiveKind::Maximize);
    }
}
