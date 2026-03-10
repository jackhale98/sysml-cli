//! Tolerance analysis domain for SysML v2 models.
//!
//! Provides worst-case, RSS, and Monte Carlo tolerance stack-up analysis,
//! sensitivity ranking, process capability metrics, and what-if exploration.
//! Integrates with `sysml-core` to extract tolerance data from parsed models.

use serde::Serialize;

// =========================================================================
// Types
// =========================================================================

/// Statistical distribution type for a tolerance contributor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionType {
    Normal,
    Uniform,
    Triangular,
    SkewedLeft,
    SkewedRight,
}

/// A single tolerance contributor in a dimension chain.
#[derive(Debug, Clone, Serialize)]
pub struct Tolerance {
    pub name: String,
    pub nominal: f64,
    pub upper_limit: f64,
    pub lower_limit: f64,
    pub distribution: DistributionType,
    pub sensitivity_coefficient: f64,
    pub is_critical: bool,
}

impl Tolerance {
    /// Bilateral tolerance range (upper - lower).
    pub fn range(&self) -> f64 {
        self.upper_limit - self.lower_limit
    }

    /// Half-range (tolerance / 2).
    pub fn half_range(&self) -> f64 {
        self.range() / 2.0
    }

    /// Maximum deviation from nominal on the upper side.
    pub fn upper_deviation(&self) -> f64 {
        self.upper_limit - self.nominal
    }

    /// Maximum deviation from nominal on the lower side.
    pub fn lower_deviation(&self) -> f64 {
        self.nominal - self.lower_limit
    }
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            name: String::new(),
            nominal: 0.0,
            upper_limit: 0.0,
            lower_limit: 0.0,
            distribution: DistributionType::Normal,
            sensitivity_coefficient: 1.0,
            is_critical: false,
        }
    }
}

/// A chain of tolerance contributors that form a stack-up.
#[derive(Debug, Clone, Serialize)]
pub struct DimensionChain {
    pub name: String,
    pub tolerances: Vec<Tolerance>,
    pub closing_dimension: String,
}

/// Analysis method selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMethod {
    WorstCase,
    Rss,
    MonteCarlo,
}

/// Result of a tolerance stack-up analysis.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub method: AnalysisMethod,
    pub nominal_result: f64,
    pub min_result: f64,
    pub max_result: f64,
    pub sigma: Option<f64>,
    pub cpk: Option<f64>,
    pub contributors: Vec<ContributorResult>,
}

/// A single contributor's impact within an analysis result.
#[derive(Debug, Clone, Serialize)]
pub struct ContributorResult {
    pub name: String,
    pub contribution_pct: f64,
    pub sensitivity: f64,
}

/// Process capability indices computed from measurement data.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessCapability {
    pub cp: f64,
    pub cpk: f64,
    pub pp: f64,
    pub ppk: f64,
    pub mean: f64,
    pub sigma: f64,
    pub sample_size: usize,
}

/// Raw measurement data for a tolerance feature.
#[derive(Debug, Clone, Serialize)]
pub struct MeasurementData {
    pub tolerance_name: String,
    pub values: Vec<f64>,
}

// =========================================================================
// Simple LCG PRNG
// =========================================================================

/// Linear congruential generator (Numerical Recipes parameters).
///
/// This is not cryptographically secure but is adequate for Monte Carlo
/// tolerance simulations with deterministic seeding.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        // Avoid degenerate zero state.
        Self {
            state: seed.wrapping_add(1),
        }
    }

    /// Advance state and return a value in [0.0, 1.0).
    fn next_f64(&mut self) -> f64 {
        // LCG with Numerical Recipes constants.
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // Use the upper 53 bits for a double in [0, 1).
        let upper = self.state >> 11;
        upper as f64 / ((1u64 << 53) as f64)
    }
}

// =========================================================================
// Distribution sampling helpers
// =========================================================================

/// Sample from a normal distribution using the Box-Muller transform.
fn normal_sample(rng: &mut Lcg, mean: f64, stddev: f64) -> f64 {
    loop {
        let u1 = rng.next_f64();
        let u2 = rng.next_f64();
        // Guard against log(0).
        if u1 <= f64::EPSILON {
            continue;
        }
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        return mean + z * stddev;
    }
}

/// Sample from a uniform distribution on [min, max].
fn uniform_sample(rng: &mut Lcg, min: f64, max: f64) -> f64 {
    min + rng.next_f64() * (max - min)
}

/// Sample from a triangular distribution with given min, mode, max.
fn triangular_sample(rng: &mut Lcg, min: f64, mode: f64, max: f64) -> f64 {
    let u = rng.next_f64();
    let fc = if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        (mode - min) / (max - min)
    };
    if u < fc {
        min + ((max - min) * (mode - min) * u).sqrt()
    } else {
        max - ((max - min) * (max - mode) * (1.0 - u)).sqrt()
    }
}

/// Sample a single value from a tolerance's distribution.
fn sample_tolerance(rng: &mut Lcg, tol: &Tolerance) -> f64 {
    match tol.distribution {
        DistributionType::Normal => {
            // Assume 3-sigma => range covers +-3sigma.
            let stddev = tol.half_range() / 3.0;
            normal_sample(rng, tol.nominal, stddev)
        }
        DistributionType::Uniform => {
            uniform_sample(rng, tol.lower_limit, tol.upper_limit)
        }
        DistributionType::Triangular => {
            triangular_sample(rng, tol.lower_limit, tol.nominal, tol.upper_limit)
        }
        DistributionType::SkewedLeft => {
            // Mode shifted toward the lower limit.
            let mode = tol.lower_limit + tol.range() * 0.25;
            triangular_sample(rng, tol.lower_limit, mode, tol.upper_limit)
        }
        DistributionType::SkewedRight => {
            // Mode shifted toward the upper limit.
            let mode = tol.lower_limit + tol.range() * 0.75;
            triangular_sample(rng, tol.lower_limit, mode, tol.upper_limit)
        }
    }
}

// =========================================================================
// Analysis functions
// =========================================================================

/// Worst-case analysis: sum of nominals +/- sum of extreme deviations.
///
/// This assumes all tolerances stack at their worst simultaneously.
pub fn worst_case_analysis(chain: &DimensionChain) -> AnalysisResult {
    let nominal_sum: f64 = chain
        .tolerances
        .iter()
        .map(|t| t.sensitivity_coefficient * t.nominal)
        .sum();

    let upper_dev: f64 = chain
        .tolerances
        .iter()
        .map(|t| {
            let coeff = t.sensitivity_coefficient;
            if coeff >= 0.0 {
                coeff * t.upper_deviation()
            } else {
                coeff * t.lower_deviation()
            }
        })
        .sum();

    let lower_dev: f64 = chain
        .tolerances
        .iter()
        .map(|t| {
            let coeff = t.sensitivity_coefficient;
            if coeff >= 0.0 {
                coeff * t.lower_deviation()
            } else {
                coeff * t.upper_deviation()
            }
        })
        .sum();

    let max_result = nominal_sum + upper_dev;
    let min_result = nominal_sum - lower_dev;

    let contributors = compute_worst_case_contributors(chain);

    AnalysisResult {
        method: AnalysisMethod::WorstCase,
        nominal_result: nominal_sum,
        min_result,
        max_result,
        sigma: None,
        cpk: None,
        contributors,
    }
}

/// Compute contributor percentages for worst-case analysis.
fn compute_worst_case_contributors(chain: &DimensionChain) -> Vec<ContributorResult> {
    let total_range: f64 = chain
        .tolerances
        .iter()
        .map(|t| t.sensitivity_coefficient.abs() * t.range())
        .sum();

    if total_range.abs() < f64::EPSILON {
        return chain
            .tolerances
            .iter()
            .map(|t| ContributorResult {
                name: t.name.clone(),
                contribution_pct: 0.0,
                sensitivity: t.sensitivity_coefficient,
            })
            .collect();
    }

    chain
        .tolerances
        .iter()
        .map(|t| {
            let contrib = t.sensitivity_coefficient.abs() * t.range();
            ContributorResult {
                name: t.name.clone(),
                contribution_pct: (contrib / total_range) * 100.0,
                sensitivity: t.sensitivity_coefficient,
            }
        })
        .collect()
}

/// Root Sum of Squares analysis with sensitivity coefficients.
///
/// Assumes tolerances are statistically independent and normally distributed.
/// The result spans +/-3 sigma from the nominal.
pub fn rss_analysis(chain: &DimensionChain) -> AnalysisResult {
    let nominal_sum: f64 = chain
        .tolerances
        .iter()
        .map(|t| t.sensitivity_coefficient * t.nominal)
        .sum();

    // Each tolerance contributes (sensitivity * half_range)^2 to the variance.
    let sum_of_squares: f64 = chain
        .tolerances
        .iter()
        .map(|t| {
            let contribution = t.sensitivity_coefficient * t.half_range();
            contribution * contribution
        })
        .sum();

    let rss_tolerance = sum_of_squares.sqrt();
    // Sigma is RSS / 3 (since half_range = 3*sigma for each contributor).
    let sigma = rss_tolerance / 3.0;

    let contributors = compute_rss_contributors(chain, sum_of_squares);

    AnalysisResult {
        method: AnalysisMethod::Rss,
        nominal_result: nominal_sum,
        min_result: nominal_sum - rss_tolerance,
        max_result: nominal_sum + rss_tolerance,
        sigma: Some(sigma),
        cpk: None,
        contributors,
    }
}

/// Compute contributor percentages for RSS analysis.
fn compute_rss_contributors(
    chain: &DimensionChain,
    total_variance: f64,
) -> Vec<ContributorResult> {
    if total_variance.abs() < f64::EPSILON {
        return chain
            .tolerances
            .iter()
            .map(|t| ContributorResult {
                name: t.name.clone(),
                contribution_pct: 0.0,
                sensitivity: t.sensitivity_coefficient,
            })
            .collect();
    }

    chain
        .tolerances
        .iter()
        .map(|t| {
            let c = t.sensitivity_coefficient * t.half_range();
            let variance = c * c;
            ContributorResult {
                name: t.name.clone(),
                contribution_pct: (variance / total_variance) * 100.0,
                sensitivity: t.sensitivity_coefficient,
            }
        })
        .collect()
}

/// Monte Carlo simulation analysis with a deterministic seed.
///
/// Samples each tolerance according to its distribution type and sums them
/// using sensitivity coefficients for `iterations` trials.
pub fn monte_carlo_analysis(
    chain: &DimensionChain,
    iterations: usize,
    seed: u64,
) -> AnalysisResult {
    if chain.tolerances.is_empty() || iterations == 0 {
        return AnalysisResult {
            method: AnalysisMethod::MonteCarlo,
            nominal_result: 0.0,
            min_result: 0.0,
            max_result: 0.0,
            sigma: Some(0.0),
            cpk: None,
            contributors: Vec::new(),
        };
    }

    let mut rng = Lcg::new(seed);
    let n = chain.tolerances.len();

    // Accumulate per-tolerance sums for sensitivity analysis.
    let mut results = Vec::with_capacity(iterations);
    let mut per_tolerance_sums = vec![0.0f64; n];
    let mut per_tolerance_sq_sums = vec![0.0f64; n];

    for _ in 0..iterations {
        let mut total = 0.0f64;
        for (i, tol) in chain.tolerances.iter().enumerate() {
            let sample = sample_tolerance(&mut rng, tol);
            let contribution = tol.sensitivity_coefficient * sample;
            per_tolerance_sums[i] += contribution;
            per_tolerance_sq_sums[i] += contribution * contribution;
            total += contribution;
        }
        results.push(total);
    }

    let n_f = iterations as f64;
    let mean: f64 = results.iter().sum::<f64>() / n_f;
    let variance: f64 =
        results.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / n_f;
    let sigma = variance.sqrt();

    let min_result = results.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_result = results.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let nominal_sum: f64 = chain
        .tolerances
        .iter()
        .map(|t| t.sensitivity_coefficient * t.nominal)
        .sum();

    // Compute contributor variance fractions.
    let total_variance: f64 = per_tolerance_sq_sums
        .iter()
        .zip(per_tolerance_sums.iter())
        .map(|(&sq_sum, &sum)| {
            let m = sum / n_f;
            sq_sum / n_f - m * m
        })
        .sum();

    let contributors: Vec<ContributorResult> = chain
        .tolerances
        .iter()
        .enumerate()
        .map(|(i, tol)| {
            let m = per_tolerance_sums[i] / n_f;
            let var_i = per_tolerance_sq_sums[i] / n_f - m * m;
            let pct = if total_variance.abs() > f64::EPSILON {
                (var_i / total_variance) * 100.0
            } else {
                0.0
            };
            ContributorResult {
                name: tol.name.clone(),
                contribution_pct: pct,
                sensitivity: tol.sensitivity_coefficient,
            }
        })
        .collect();

    AnalysisResult {
        method: AnalysisMethod::MonteCarlo,
        nominal_result: nominal_sum,
        min_result,
        max_result,
        sigma: Some(sigma),
        cpk: None,
        contributors,
    }
}

/// Dispatch to the appropriate analysis method.
///
/// The `iterations` parameter is only used for Monte Carlo; ignored otherwise.
pub fn analyze(
    chain: &DimensionChain,
    method: &AnalysisMethod,
    iterations: usize,
) -> AnalysisResult {
    match method {
        AnalysisMethod::WorstCase => worst_case_analysis(chain),
        AnalysisMethod::Rss => rss_analysis(chain),
        AnalysisMethod::MonteCarlo => monte_carlo_analysis(chain, iterations, 42),
    }
}

/// Rank tolerances by their contribution to overall variation.
///
/// Uses the RSS variance-based approach: each tolerance's contribution is
/// proportional to `(sensitivity * half_range)^2`.
pub fn sensitivity_analysis(chain: &DimensionChain) -> Vec<ContributorResult> {
    let variances: Vec<f64> = chain
        .tolerances
        .iter()
        .map(|t| {
            let c = t.sensitivity_coefficient * t.half_range();
            c * c
        })
        .collect();

    let total: f64 = variances.iter().sum();

    let mut contributors: Vec<ContributorResult> = chain
        .tolerances
        .iter()
        .zip(variances.iter())
        .map(|(t, &var)| ContributorResult {
            name: t.name.clone(),
            contribution_pct: if total.abs() > f64::EPSILON {
                (var / total) * 100.0
            } else {
                0.0
            },
            sensitivity: t.sensitivity_coefficient,
        })
        .collect();

    // Sort descending by contribution.
    contributors.sort_by(|a, b| {
        b.contribution_pct
            .partial_cmp(&a.contribution_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    contributors
}

/// Compute process capability indices from measurement data.
///
/// Returns `Cp`, `Cpk`, `Pp`, `Ppk` along with mean and sigma.  For this
/// implementation Pp/Ppk are computed identically to Cp/Cpk (using the
/// overall standard deviation since we have no subgroup information).
pub fn compute_capability(
    data: &MeasurementData,
    tolerance: &Tolerance,
) -> ProcessCapability {
    let n = data.values.len();
    if n < 2 {
        return ProcessCapability {
            cp: 0.0,
            cpk: 0.0,
            pp: 0.0,
            ppk: 0.0,
            mean: data.values.first().copied().unwrap_or(0.0),
            sigma: 0.0,
            sample_size: n,
        };
    }

    let n_f = n as f64;
    let mean = data.values.iter().sum::<f64>() / n_f;
    let variance =
        data.values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n_f - 1.0);
    let sigma = variance.sqrt();

    let usl = tolerance.upper_limit;
    let lsl = tolerance.lower_limit;

    let (cp, cpk) = if sigma.abs() < f64::EPSILON {
        (f64::INFINITY, f64::INFINITY)
    } else {
        let cp = (usl - lsl) / (6.0 * sigma);
        let cpu = (usl - mean) / (3.0 * sigma);
        let cpl = (mean - lsl) / (3.0 * sigma);
        let cpk = cpu.min(cpl);
        (cp, cpk)
    };

    // Pp/Ppk use overall sigma (same as Cp/Cpk here since we have no
    // subgroup information).
    ProcessCapability {
        cp,
        cpk,
        pp: cp,
        ppk: cpk,
        mean,
        sigma,
        sample_size: n,
    }
}

/// Clone the dimension chain, apply bilateral tolerance modifications, and
/// run the specified analysis.
///
/// Each entry in `modifications` is `(tolerance_name, new_bilateral_value)`.
/// The bilateral value is applied symmetrically around the nominal:
/// `upper = nominal + bilateral`, `lower = nominal - bilateral`.
pub fn whatif_analysis(
    chain: &DimensionChain,
    modifications: &[(String, f64)],
    method: &AnalysisMethod,
) -> AnalysisResult {
    let mut modified = chain.clone();
    for (name, bilateral) in modifications {
        for tol in &mut modified.tolerances {
            if tol.name == *name {
                tol.upper_limit = tol.nominal + bilateral;
                tol.lower_limit = tol.nominal - bilateral;
            }
        }
    }
    analyze(&modified, method, 10_000)
}

// =========================================================================
// Model extraction
// =========================================================================

/// Scan a parsed SysML model for attribute definitions that represent
/// tolerances.
///
/// Looks for attribute definitions named "tolerance" (case-insensitive) or
/// specializing "ToleranceDef". Extracts nominal, upper_limit, and
/// lower_limit from nested attribute usages with matching names and
/// `value_expr` fields.
pub fn extract_tolerances(model: &sysml_core::model::Model) -> Vec<Tolerance> {
    let mut result = Vec::new();

    for def in &model.definitions {
        if def.kind != sysml_core::model::DefKind::Attribute {
            continue;
        }

        let is_tolerance_def = def.name.to_lowercase().contains("tolerance")
            || def
                .super_type
                .as_deref()
                .map(|s| s == "ToleranceDef")
                .unwrap_or(false);

        if !is_tolerance_def {
            continue;
        }

        let usages = model.usages_in_def(&def.name);
        let mut tol = Tolerance {
            name: def.name.clone(),
            ..Default::default()
        };

        for u in &usages {
            let attr_name = u.name.to_lowercase();
            if let Some(ref expr) = u.value_expr {
                if let Ok(val) = expr.trim().parse::<f64>() {
                    match attr_name.as_str() {
                        "nominal" => tol.nominal = val,
                        "upper_limit" | "upperlimit" | "upper" => {
                            tol.upper_limit = val
                        }
                        "lower_limit" | "lowerlimit" | "lower" => {
                            tol.lower_limit = val
                        }
                        "sensitivity" | "sensitivity_coefficient" => {
                            tol.sensitivity_coefficient = val
                        }
                        _ => {}
                    }
                }
                // Check for boolean "critical" attribute.
                if attr_name == "is_critical" || attr_name == "critical" {
                    tol.is_critical = expr.trim() == "true";
                }
            }
        }

        result.push(tol);
    }

    result
}

/// Scan a parsed SysML model for part definitions that represent dimension
/// chains.
///
/// Looks for part definitions specializing "DimensionChainDef". Nested part
/// usages whose type_ref matches a tolerance name are treated as members of
/// the chain. The closing dimension is taken from an attribute usage named
/// "closing_dimension" or defaults to the definition name.
pub fn extract_dimension_chains(
    model: &sysml_core::model::Model,
) -> Vec<DimensionChain> {
    let tolerance_names: std::collections::HashSet<String> = extract_tolerances(model)
        .into_iter()
        .map(|t| t.name)
        .collect();

    let tolerances_by_name: std::collections::HashMap<String, Tolerance> =
        extract_tolerances(model)
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect();

    let mut chains = Vec::new();

    for def in &model.definitions {
        let is_chain_def = def
            .super_type
            .as_deref()
            .map(|s| s == "DimensionChainDef")
            .unwrap_or(false);

        if !is_chain_def {
            continue;
        }

        let usages = model.usages_in_def(&def.name);
        let mut chain_tolerances = Vec::new();
        let mut closing = def.name.clone();

        for u in &usages {
            // Check if this usage references a tolerance type.
            if let Some(ref type_ref) = u.type_ref {
                let simple = sysml_core::model::simple_name(type_ref);
                if let Some(tol) = tolerances_by_name.get(simple) {
                    chain_tolerances.push(tol.clone());
                } else if tolerance_names.contains(simple) {
                    chain_tolerances.push(Tolerance {
                        name: simple.to_string(),
                        ..Default::default()
                    });
                }
            }

            // Look for closing_dimension attribute.
            if u.name.to_lowercase() == "closing_dimension"
                || u.name.to_lowercase() == "closingdimension"
            {
                if let Some(ref expr) = u.value_expr {
                    closing = expr.trim().trim_matches('"').to_string();
                }
            }
        }

        chains.push(DimensionChain {
            name: def.name.clone(),
            tolerances: chain_tolerances,
            closing_dimension: closing,
        });
    }

    chains
}

// =========================================================================
// SysML generation
// =========================================================================

/// Generate SysML text for a tolerance chain constraint.
pub fn tolerance_chain_to_sysml(name: &str, contributors: &[(String, f64, f64)]) -> String {
    let mut out = format!("constraint def {} {{\n", name);
    out.push_str("    doc /* Tolerance stack-up chain */\n");
    for (cname, nominal, tolerance) in contributors {
        out.push_str(&format!(
            "    attribute {} : Real = {} +/- {};\n",
            cname, nominal, tolerance
        ));
    }
    out.push_str("}\n");
    out
}

/// Build wizard steps for interactive tolerance chain creation.
pub fn build_tol_add_wizard(model: Option<&sysml_core::model::Model>) -> Vec<sysml_core::interactive::WizardStep> {
    use sysml_core::interactive::*;

    let dist_choices = if let Some(m) = model {
        let choices = sysml_core::query::get_enum_choices(m, "DistributionType");
        if !choices.is_empty() {
            choices.into_iter().map(|(v, _)| ChoiceOption::new(&v, &v)).collect()
        } else {
            default_distribution_choices()
        }
    } else {
        default_distribution_choices()
    };

    vec![
        WizardStep::string("name", "Tolerance chain name")
            .with_explanation("Name for the tolerance stack-up analysis."),
        WizardStep {
            id: "distribution".into(),
            prompt: "Default distribution type".into(),
            explanation: Some("Statistical distribution assumed for contributors.".into()),
            kind: PromptKind::Choice(dist_choices),
            required: true,
            default: None,
        },
        WizardStep::number("contributor_count", "Number of contributors")
            .with_explanation("How many dimensions contribute to this stack?")
            .with_bounds(Some(1.0), Some(50.0)),
    ]
}

/// Interpret wizard results into (element_name, sysml_text).
pub fn interpret_tol_add_wizard(result: &sysml_core::interactive::WizardResult) -> Option<(String, String)> {
    let name = result.get_string("name")?;
    // Generate a placeholder chain (actual contributors would come from follow-up prompts)
    let sysml = tolerance_chain_to_sysml(name, &[]);
    Some((name.to_string(), sysml))
}

fn default_distribution_choices() -> Vec<sysml_core::interactive::ChoiceOption> {
    use sysml_core::interactive::ChoiceOption;
    vec![
        ChoiceOption::new("normal", "normal"),
        ChoiceOption::new("uniform", "uniform"),
        ChoiceOption::new("triangular", "triangular"),
    ]
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers -----------------------------------------------------------

    /// Build a simple two-tolerance chain for testing.
    fn simple_chain() -> DimensionChain {
        DimensionChain {
            name: "TestChain".into(),
            tolerances: vec![
                Tolerance {
                    name: "A".into(),
                    nominal: 10.0,
                    upper_limit: 10.5,
                    lower_limit: 9.5,
                    distribution: DistributionType::Normal,
                    sensitivity_coefficient: 1.0,
                    is_critical: false,
                },
                Tolerance {
                    name: "B".into(),
                    nominal: 20.0,
                    upper_limit: 20.3,
                    lower_limit: 19.7,
                    distribution: DistributionType::Normal,
                    sensitivity_coefficient: 1.0,
                    is_critical: false,
                },
            ],
            closing_dimension: "Gap".into(),
        }
    }

    /// Build a chain with unequal tolerances and sensitivity coefficients.
    fn weighted_chain() -> DimensionChain {
        DimensionChain {
            name: "WeightedChain".into(),
            tolerances: vec![
                Tolerance {
                    name: "X".into(),
                    nominal: 5.0,
                    upper_limit: 5.2,
                    lower_limit: 4.8,
                    distribution: DistributionType::Normal,
                    sensitivity_coefficient: 2.0,
                    is_critical: true,
                },
                Tolerance {
                    name: "Y".into(),
                    nominal: 3.0,
                    upper_limit: 3.1,
                    lower_limit: 2.9,
                    distribution: DistributionType::Uniform,
                    sensitivity_coefficient: -1.0,
                    is_critical: false,
                },
            ],
            closing_dimension: "Result".into(),
        }
    }

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    // -- LCG tests --------------------------------------------------------

    #[test]
    fn lcg_produces_values_in_range() {
        let mut rng = Lcg::new(12345);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!(v >= 0.0 && v < 1.0, "value out of range: {v}");
        }
    }

    #[test]
    fn lcg_deterministic() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_f64().to_bits(), b.next_f64().to_bits());
        }
    }

    #[test]
    fn lcg_different_seeds_differ() {
        let mut a = Lcg::new(1);
        let mut b = Lcg::new(2);
        let va: Vec<u64> = (0..10).map(|_| a.next_f64().to_bits()).collect();
        let vb: Vec<u64> = (0..10).map(|_| b.next_f64().to_bits()).collect();
        assert_ne!(va, vb);
    }

    // -- Tolerance struct tests -------------------------------------------

    #[test]
    fn tolerance_range_and_half_range() {
        let t = Tolerance {
            name: "T".into(),
            nominal: 10.0,
            upper_limit: 10.5,
            lower_limit: 9.5,
            ..Default::default()
        };
        assert!(approx_eq(t.range(), 1.0, 1e-10));
        assert!(approx_eq(t.half_range(), 0.5, 1e-10));
    }

    #[test]
    fn tolerance_deviations() {
        let t = Tolerance {
            name: "T".into(),
            nominal: 10.0,
            upper_limit: 10.3,
            lower_limit: 9.8,
            ..Default::default()
        };
        assert!(approx_eq(t.upper_deviation(), 0.3, 1e-10));
        assert!(approx_eq(t.lower_deviation(), 0.2, 1e-10));
    }

    #[test]
    fn tolerance_default() {
        let t = Tolerance::default();
        assert_eq!(t.sensitivity_coefficient, 1.0);
        assert!(!t.is_critical);
        assert!(t.name.is_empty());
        assert_eq!(t.distribution, DistributionType::Normal);
    }

    // -- Worst case analysis tests ----------------------------------------

    #[test]
    fn worst_case_simple_chain() {
        let chain = simple_chain();
        let result = worst_case_analysis(&chain);

        assert_eq!(result.method, AnalysisMethod::WorstCase);
        assert!(approx_eq(result.nominal_result, 30.0, 1e-10));
        // max = 10.5 + 20.3 = 30.8
        assert!(approx_eq(result.max_result, 30.8, 1e-10));
        // min = 9.5 + 19.7 = 29.2
        assert!(approx_eq(result.min_result, 29.2, 1e-10));
        assert!(result.sigma.is_none());
    }

    #[test]
    fn worst_case_with_sensitivity_coefficients() {
        let chain = weighted_chain();
        let result = worst_case_analysis(&chain);

        // nominal = 2*5 + (-1)*3 = 7.0
        assert!(approx_eq(result.nominal_result, 7.0, 1e-10));
        // For coeff=2 (positive): upper_dev = 2*0.2 = 0.4
        // For coeff=-1 (negative): upper uses lower_dev = -1*0.1 = -0.1
        // total upper_dev = 0.4 + (-0.1) = 0.3
        // max = 7.0 + 0.3 = 7.3
        assert!(approx_eq(result.max_result, 7.3, 1e-10));
        // total lower_dev = 2*0.2 + (-1)*0.1 = 0.4 + (-0.1) = 0.3
        // min = 7.0 - 0.3 = 6.7
        assert!(approx_eq(result.min_result, 6.7, 1e-10));
    }

    #[test]
    fn worst_case_contributors_sum_to_100() {
        let chain = simple_chain();
        let result = worst_case_analysis(&chain);
        let total: f64 = result
            .contributors
            .iter()
            .map(|c| c.contribution_pct)
            .sum();
        assert!(approx_eq(total, 100.0, 1e-6));
    }

    #[test]
    fn worst_case_empty_chain() {
        let chain = DimensionChain {
            name: "Empty".into(),
            tolerances: vec![],
            closing_dimension: "None".into(),
        };
        let result = worst_case_analysis(&chain);
        assert!(approx_eq(result.nominal_result, 0.0, 1e-10));
        assert!(approx_eq(result.min_result, 0.0, 1e-10));
        assert!(approx_eq(result.max_result, 0.0, 1e-10));
    }

    // -- RSS analysis tests -----------------------------------------------

    #[test]
    fn rss_simple_chain() {
        let chain = simple_chain();
        let result = rss_analysis(&chain);

        assert_eq!(result.method, AnalysisMethod::Rss);
        assert!(approx_eq(result.nominal_result, 30.0, 1e-10));

        // RSS tolerance = sqrt(0.5^2 + 0.3^2) = sqrt(0.25 + 0.09) = sqrt(0.34)
        let expected_rss = (0.25f64 + 0.09).sqrt();
        assert!(approx_eq(result.max_result, 30.0 + expected_rss, 1e-10));
        assert!(approx_eq(result.min_result, 30.0 - expected_rss, 1e-10));
    }

    #[test]
    fn rss_sigma_is_rss_over_3() {
        let chain = simple_chain();
        let result = rss_analysis(&chain);
        let expected_rss = (0.25f64 + 0.09).sqrt();
        let expected_sigma = expected_rss / 3.0;
        assert!(approx_eq(result.sigma.unwrap(), expected_sigma, 1e-10));
    }

    #[test]
    fn rss_contributors_sum_to_100() {
        let chain = simple_chain();
        let result = rss_analysis(&chain);
        let total: f64 = result
            .contributors
            .iter()
            .map(|c| c.contribution_pct)
            .sum();
        assert!(approx_eq(total, 100.0, 1e-6));
    }

    #[test]
    fn rss_tighter_tolerance_contributes_less() {
        let chain = simple_chain();
        let result = rss_analysis(&chain);
        // A has range 1.0 (half 0.5), B has range 0.6 (half 0.3)
        // A variance = 0.25, B variance = 0.09, total = 0.34
        // A = 73.5%, B = 26.5%
        let a_contrib = result
            .contributors
            .iter()
            .find(|c| c.name == "A")
            .unwrap();
        let b_contrib = result
            .contributors
            .iter()
            .find(|c| c.name == "B")
            .unwrap();
        assert!(a_contrib.contribution_pct > b_contrib.contribution_pct);
        assert!(approx_eq(a_contrib.contribution_pct, 73.529, 0.01));
        assert!(approx_eq(b_contrib.contribution_pct, 26.470, 0.01));
    }

    #[test]
    fn rss_with_sensitivity_coefficients() {
        let chain = weighted_chain();
        let result = rss_analysis(&chain);

        // X: sens=2, half_range=0.2 => contribution = 2*0.2 = 0.4, var = 0.16
        // Y: sens=-1, half_range=0.1 => contribution = -1*0.1 = -0.1, var = 0.01
        // total_variance = 0.17
        // rss = sqrt(0.17)
        let expected_rss = 0.17f64.sqrt();
        assert!(approx_eq(
            result.max_result - result.nominal_result,
            expected_rss,
            1e-10
        ));
    }

    // -- Monte Carlo analysis tests ---------------------------------------

    #[test]
    fn monte_carlo_deterministic() {
        let chain = simple_chain();
        let a = monte_carlo_analysis(&chain, 5000, 42);
        let b = monte_carlo_analysis(&chain, 5000, 42);
        assert_eq!(a.min_result.to_bits(), b.min_result.to_bits());
        assert_eq!(a.max_result.to_bits(), b.max_result.to_bits());
        assert_eq!(a.sigma.unwrap().to_bits(), b.sigma.unwrap().to_bits());
    }

    #[test]
    fn monte_carlo_results_within_worst_case() {
        let chain = simple_chain();
        let wc = worst_case_analysis(&chain);
        let mc = monte_carlo_analysis(&chain, 50_000, 99);

        // Monte Carlo min/max should be within worst-case bounds (with some
        // allowance for sampling noise at the extreme tails).
        assert!(mc.min_result >= wc.min_result - 0.5);
        assert!(mc.max_result <= wc.max_result + 0.5);
    }

    #[test]
    fn monte_carlo_nominal_close_to_expected() {
        let chain = simple_chain();
        let mc = monte_carlo_analysis(&chain, 100_000, 7);
        assert!(approx_eq(mc.nominal_result, 30.0, 1e-10));
    }

    #[test]
    fn monte_carlo_sigma_reasonable() {
        let chain = simple_chain();
        let mc = monte_carlo_analysis(&chain, 100_000, 7);
        // For normal distributions, sigma should be close to RSS sigma.
        let rss = rss_analysis(&chain);
        let mc_sigma = mc.sigma.unwrap();
        let rss_sigma = rss.sigma.unwrap();
        // Allow 15% tolerance for Monte Carlo sampling variation.
        assert!(
            approx_eq(mc_sigma, rss_sigma, rss_sigma * 0.15),
            "MC sigma {mc_sigma} too far from RSS sigma {rss_sigma}"
        );
    }

    #[test]
    fn monte_carlo_empty_chain() {
        let chain = DimensionChain {
            name: "Empty".into(),
            tolerances: vec![],
            closing_dimension: "None".into(),
        };
        let result = monte_carlo_analysis(&chain, 1000, 42);
        assert!(approx_eq(result.nominal_result, 0.0, 1e-10));
    }

    #[test]
    fn monte_carlo_zero_iterations() {
        let chain = simple_chain();
        let result = monte_carlo_analysis(&chain, 0, 42);
        assert!(approx_eq(result.nominal_result, 0.0, 1e-10));
    }

    // -- analyze dispatch tests -------------------------------------------

    #[test]
    fn analyze_dispatches_worst_case() {
        let chain = simple_chain();
        let result = analyze(&chain, &AnalysisMethod::WorstCase, 0);
        assert_eq!(result.method, AnalysisMethod::WorstCase);
    }

    #[test]
    fn analyze_dispatches_rss() {
        let chain = simple_chain();
        let result = analyze(&chain, &AnalysisMethod::Rss, 0);
        assert_eq!(result.method, AnalysisMethod::Rss);
    }

    #[test]
    fn analyze_dispatches_monte_carlo() {
        let chain = simple_chain();
        let result = analyze(&chain, &AnalysisMethod::MonteCarlo, 1000);
        assert_eq!(result.method, AnalysisMethod::MonteCarlo);
    }

    // -- Sensitivity analysis tests ---------------------------------------

    #[test]
    fn sensitivity_sorted_descending() {
        let chain = simple_chain();
        let ranked = sensitivity_analysis(&chain);
        assert_eq!(ranked.len(), 2);
        // A has higher variance so it should come first.
        assert_eq!(ranked[0].name, "A");
        assert_eq!(ranked[1].name, "B");
        assert!(ranked[0].contribution_pct > ranked[1].contribution_pct);
    }

    #[test]
    fn sensitivity_sums_to_100() {
        let chain = weighted_chain();
        let ranked = sensitivity_analysis(&chain);
        let total: f64 = ranked.iter().map(|c| c.contribution_pct).sum();
        assert!(approx_eq(total, 100.0, 1e-6));
    }

    // -- Process capability tests -----------------------------------------

    #[test]
    fn capability_perfect_process() {
        // Data centered exactly at nominal with small spread.
        let tol = Tolerance {
            name: "T".into(),
            nominal: 10.0,
            upper_limit: 10.6,
            lower_limit: 9.4,
            ..Default::default()
        };
        // Range = 1.2, center = 10.0
        // Generate data tightly centered at 10.0.
        let values: Vec<f64> = (0..100)
            .map(|i| 10.0 + (i as f64 - 49.5) * 0.001)
            .collect();
        let data = MeasurementData {
            tolerance_name: "T".into(),
            values,
        };
        let cap = compute_capability(&data, &tol);
        assert_eq!(cap.sample_size, 100);
        assert!(
            cap.cp > 1.0,
            "Cp should be > 1 for tight data: {}",
            cap.cp
        );
        assert!(
            cap.cpk > 1.0,
            "Cpk should be > 1 for centered data: {}",
            cap.cpk
        );
    }

    #[test]
    fn capability_off_center_reduces_cpk() {
        let tol = Tolerance {
            name: "T".into(),
            nominal: 10.0,
            upper_limit: 10.5,
            lower_limit: 9.5,
            ..Default::default()
        };
        // Data shifted toward upper limit.
        let values: Vec<f64> = (0..100)
            .map(|i| 10.3 + (i as f64 - 49.5) * 0.001)
            .collect();
        let data = MeasurementData {
            tolerance_name: "T".into(),
            values,
        };
        let cap = compute_capability(&data, &tol);
        assert!(
            cap.cp > cap.cpk,
            "Cp ({}) should be > Cpk ({}) when off-center",
            cap.cp,
            cap.cpk
        );
    }

    #[test]
    fn capability_insufficient_data() {
        let tol = Tolerance::default();
        let data = MeasurementData {
            tolerance_name: "T".into(),
            values: vec![1.0],
        };
        let cap = compute_capability(&data, &tol);
        assert_eq!(cap.sample_size, 1);
        assert!(approx_eq(cap.sigma, 0.0, 1e-10));
    }

    #[test]
    fn capability_pp_equals_cp() {
        // In our implementation Pp/Ppk = Cp/Cpk.
        let tol = Tolerance {
            name: "T".into(),
            nominal: 10.0,
            upper_limit: 11.0,
            lower_limit: 9.0,
            ..Default::default()
        };
        let values: Vec<f64> = (0..50)
            .map(|i| 10.0 + (i as f64 - 24.5) * 0.01)
            .collect();
        let data = MeasurementData {
            tolerance_name: "T".into(),
            values,
        };
        let cap = compute_capability(&data, &tol);
        assert!(approx_eq(cap.cp, cap.pp, 1e-10));
        assert!(approx_eq(cap.cpk, cap.ppk, 1e-10));
    }

    // -- What-if analysis tests -------------------------------------------

    #[test]
    fn whatif_tightening_reduces_range() {
        let chain = simple_chain();
        let baseline = worst_case_analysis(&chain);
        let baseline_range = baseline.max_result - baseline.min_result;

        // Tighten tolerance A from +/-0.5 to +/-0.1.
        let modified = whatif_analysis(
            &chain,
            &[("A".into(), 0.1)],
            &AnalysisMethod::WorstCase,
        );
        let modified_range = modified.max_result - modified.min_result;
        assert!(
            modified_range < baseline_range,
            "Tightened range {modified_range} should be < baseline {baseline_range}"
        );
    }

    #[test]
    fn whatif_loosening_increases_range() {
        let chain = simple_chain();
        let baseline = worst_case_analysis(&chain);
        let baseline_range = baseline.max_result - baseline.min_result;

        // Loosen tolerance B from +/-0.3 to +/-1.0.
        let modified = whatif_analysis(
            &chain,
            &[("B".into(), 1.0)],
            &AnalysisMethod::WorstCase,
        );
        let modified_range = modified.max_result - modified.min_result;
        assert!(
            modified_range > baseline_range,
            "Loosened range {modified_range} should be > baseline {baseline_range}"
        );
    }

    #[test]
    fn whatif_preserves_nominal() {
        let chain = simple_chain();
        let modified = whatif_analysis(
            &chain,
            &[("A".into(), 0.1)],
            &AnalysisMethod::WorstCase,
        );
        assert!(approx_eq(modified.nominal_result, 30.0, 1e-10));
    }

    #[test]
    fn whatif_nonexistent_tolerance_is_noop() {
        let chain = simple_chain();
        let baseline = worst_case_analysis(&chain);
        let modified = whatif_analysis(
            &chain,
            &[("ZZZ".into(), 0.001)],
            &AnalysisMethod::WorstCase,
        );
        assert!(approx_eq(baseline.max_result, modified.max_result, 1e-10));
    }

    // -- Distribution sampling tests --------------------------------------

    #[test]
    fn normal_sample_mean_close_to_target() {
        let mut rng = Lcg::new(100);
        let samples: Vec<f64> = (0..10_000)
            .map(|_| normal_sample(&mut rng, 5.0, 1.0))
            .collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!(
            approx_eq(mean, 5.0, 0.1),
            "Normal mean {mean} not close to 5.0"
        );
    }

    #[test]
    fn uniform_sample_within_bounds() {
        let mut rng = Lcg::new(200);
        for _ in 0..1000 {
            let v = uniform_sample(&mut rng, 2.0, 5.0);
            assert!(v >= 2.0 && v <= 5.0, "uniform sample {v} out of [2, 5]");
        }
    }

    #[test]
    fn triangular_sample_within_bounds() {
        let mut rng = Lcg::new(300);
        for _ in 0..1000 {
            let v = triangular_sample(&mut rng, 1.0, 3.0, 5.0);
            assert!(
                v >= 1.0 && v <= 5.0,
                "triangular sample {v} out of [1, 5]"
            );
        }
    }

    // -- Model extraction tests -------------------------------------------

    #[test]
    fn extract_tolerances_from_empty_model() {
        let model = sysml_core::model::Model::new("test.sysml".into());
        let tolerances = extract_tolerances(&model);
        assert!(tolerances.is_empty());
    }

    #[test]
    fn extract_dimension_chains_from_empty_model() {
        let model = sysml_core::model::Model::new("test.sysml".into());
        let chains = extract_dimension_chains(&model);
        assert!(chains.is_empty());
    }

    #[test]
    fn extract_tolerances_finds_tolerance_def() {
        let mut model = sysml_core::model::Model::new("test.sysml".into());
        model.definitions.push(sysml_core::model::Definition {
            kind: sysml_core::model::DefKind::Attribute,
            name: "LengthTolerance".into(),
            super_type: Some("ToleranceDef".into()),
            span: sysml_core::model::Span::default(),
            has_body: true,
            param_count: 0,
            has_constraint_expr: false,
            has_return: false,
            visibility: None,
            short_name: None,
            doc: None,
            is_abstract: false,
            enum_members: Vec::new(),
            parent_def: None,
            body_start_byte: None,
            body_end_byte: None,
            qualified_name: None,
        });
        model.usages.push(sysml_core::model::Usage {
            kind: "attribute".into(),
            name: "nominal".into(),
            type_ref: None,
            span: sysml_core::model::Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("LengthTolerance".into()),
            multiplicity: None,
            value_expr: Some("10.0".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });
        model.usages.push(sysml_core::model::Usage {
            kind: "attribute".into(),
            name: "upper_limit".into(),
            type_ref: None,
            span: sysml_core::model::Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("LengthTolerance".into()),
            multiplicity: None,
            value_expr: Some("10.5".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });
        model.usages.push(sysml_core::model::Usage {
            kind: "attribute".into(),
            name: "lower_limit".into(),
            type_ref: None,
            span: sysml_core::model::Span::default(),
            direction: None,
            is_conjugated: false,
            parent_def: Some("LengthTolerance".into()),
            multiplicity: None,
            value_expr: Some("9.5".into()),
            short_name: None,
            redefinition: None,
            subsets: None,
            qualified_name: None,
        });

        let tolerances = extract_tolerances(&model);
        assert_eq!(tolerances.len(), 1);
        assert_eq!(tolerances[0].name, "LengthTolerance");
        assert!(approx_eq(tolerances[0].nominal, 10.0, 1e-10));
        assert!(approx_eq(tolerances[0].upper_limit, 10.5, 1e-10));
        assert!(approx_eq(tolerances[0].lower_limit, 9.5, 1e-10));
    }

    // -- Single-tolerance chain tests -------------------------------------

    #[test]
    fn single_tolerance_worst_case() {
        let chain = DimensionChain {
            name: "Single".into(),
            tolerances: vec![Tolerance {
                name: "Only".into(),
                nominal: 50.0,
                upper_limit: 50.1,
                lower_limit: 49.9,
                distribution: DistributionType::Normal,
                sensitivity_coefficient: 1.0,
                is_critical: false,
            }],
            closing_dimension: "Gap".into(),
        };
        let result = worst_case_analysis(&chain);
        assert!(approx_eq(result.nominal_result, 50.0, 1e-10));
        assert!(approx_eq(result.max_result, 50.1, 1e-10));
        assert!(approx_eq(result.min_result, 49.9, 1e-10));
        assert_eq!(result.contributors.len(), 1);
        assert!(approx_eq(
            result.contributors[0].contribution_pct,
            100.0,
            1e-6
        ));
    }

    #[test]
    fn single_tolerance_rss_equals_worst_case() {
        let chain = DimensionChain {
            name: "Single".into(),
            tolerances: vec![Tolerance {
                name: "Only".into(),
                nominal: 50.0,
                upper_limit: 50.1,
                lower_limit: 49.9,
                distribution: DistributionType::Normal,
                sensitivity_coefficient: 1.0,
                is_critical: false,
            }],
            closing_dimension: "Gap".into(),
        };
        let wc = worst_case_analysis(&chain);
        let rss = rss_analysis(&chain);
        // For a single tolerance RSS = WC.
        assert!(approx_eq(wc.max_result, rss.max_result, 1e-10));
        assert!(approx_eq(wc.min_result, rss.min_result, 1e-10));
    }

    // -- RSS vs worst-case comparison ------------------------------------

    #[test]
    fn rss_range_less_than_worst_case() {
        let chain = simple_chain();
        let wc = worst_case_analysis(&chain);
        let rss = rss_analysis(&chain);
        let wc_range = wc.max_result - wc.min_result;
        let rss_range = rss.max_result - rss.min_result;
        assert!(
            rss_range < wc_range,
            "RSS range {rss_range} should be < WC range {wc_range}"
        );
    }

    // -- Serialization sanity check --------------------------------------

    #[test]
    fn analysis_result_serializable() {
        let chain = simple_chain();
        let result = worst_case_analysis(&chain);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"worst_case\""));
        assert!(json.contains("\"nominal_result\""));
    }

    #[test]
    fn distribution_type_serializable() {
        let json = serde_json::to_string(&DistributionType::SkewedLeft).unwrap();
        assert_eq!(json, "\"skewed_left\"");
    }

    // -- SysML generator tests ----------------------------------------------

    #[test]
    fn tolerance_chain_to_sysml_basic() {
        let sysml = tolerance_chain_to_sysml(
            "BoltStack",
            &[
                ("length1".to_string(), 10.0, 0.1),
                ("length2".to_string(), 5.0, 0.05),
            ],
        );
        assert!(sysml.contains("constraint def BoltStack {"));
        assert!(sysml.contains("attribute length1 : Real = 10 +/- 0.1;"));
        assert!(sysml.contains("attribute length2 : Real = 5 +/- 0.05;"));
    }

    #[test]
    fn build_tol_wizard_defaults() {
        let steps = build_tol_add_wizard(None);
        assert!(steps.len() >= 2);
        assert_eq!(steps[0].id, "name");
    }

    #[test]
    fn interpret_tol_wizard() {
        use sysml_core::interactive::*;
        let mut result = WizardResult::new();
        result.set("name", WizardAnswer::String("GapStack".into()));
        result.set("distribution", WizardAnswer::String("normal".into()));
        result.set("contributor_count", WizardAnswer::Number(3.0));
        let (name, sysml) = interpret_tol_add_wizard(&result).unwrap();
        assert_eq!(name, "GapStack");
        assert!(sysml.contains("constraint def GapStack"));
    }
}
