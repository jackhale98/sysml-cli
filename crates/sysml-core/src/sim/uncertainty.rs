//! Generic uncertainty propagation: the analyzer contract of the
//! `Uncertainty` domain library.
//!
//! Pure math over resolved inputs — no model access. Three methods:
//! worst-case interval arithmetic, RSS linear variance propagation, and
//! seeded Monte Carlo sampling. Extraction of inputs from a model (feature
//! chains, senses, targets) lives in the analyzer front-end, not here.

/// Statistical distribution assumed for an uncertain input
/// (`Uncertainty::Distribution` in the domain library).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Distribution {
    Normal,
    Uniform,
    Triangular,
}

/// One resolved contribution: a value with asymmetric bounds, a signed
/// sense (+1.0 / -1.0), and a distribution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UncertainInput {
    pub name: String,
    pub nominal: f64,
    /// Magnitude of the allowed positive deviation (>= 0).
    pub plus: f64,
    /// Magnitude of the allowed negative deviation (>= 0).
    pub minus: f64,
    /// +1.0 when the input increases the result, -1.0 when it decreases it.
    pub sense: f64,
    pub distribution: Distribution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Specification limits for the result (`Uncertainty::LimitRange`).
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Target {
    pub nominal: f64,
    pub lower: f64,
    pub upper: f64,
}

/// Analysis settings (`Uncertainty::UncertaintyAnalysis` attributes).
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Divisor mapping a tolerance band to one standard deviation
    /// (default 6.0: the band is a +/-3-sigma process).
    pub sigma_level: f64,
    /// Bender k-factor: shift the predicted mean k*sigma toward the
    /// nearest specification limit (0.0 disables; 1.5 automotive).
    pub mean_shift_k: f64,
    pub iterations: u64,
    pub seed: Option<u64>,
    /// A positive margin below this fraction of the tolerance band
    /// reports MARGINAL rather than PASS. Acceptance policy, not
    /// arithmetic: the model owns it
    /// (`UncertaintyAnalysis::marginalFraction`), and this value is only
    /// the fallback for a model that declares nothing.
    pub marginal_fraction: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sigma_level: 6.0,
            mean_shift_k: 0.0,
            iterations: 10_000,
            seed: None,
            marginal_fraction: 0.10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PassFail {
    Pass,
    Marginal,
    Fail,
}

/// Fail on a negative margin; otherwise marginal while the margin is
/// under `marginal_fraction` of the tolerance band. The fraction comes
/// from the model (`UncertaintyAnalysis::marginalFraction`), so what
/// counts as too close to a limit is a project decision rather than a
/// constant compiled into the analyzer.
fn classify(margin: f64, target: &Target, marginal_fraction: f64) -> PassFail {
    let band = target.upper - target.lower;
    if margin < 0.0 {
        PassFail::Fail
    } else if margin < marginal_fraction * band {
        PassFail::Marginal
    } else {
        PassFail::Pass
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorstCaseResult {
    pub min: f64,
    pub max: f64,
    pub margin: f64,
    pub result: PassFail,
}

/// Worst-case interval arithmetic: every input simultaneously at the
/// limit that drives the result toward each extreme.
pub fn worst_case(
    inputs: &[UncertainInput],
    target: &Target,
    settings: &Settings,
) -> WorstCaseResult {
    let mut min = 0.0;
    let mut max = 0.0;
    for c in inputs {
        if c.sense >= 0.0 {
            min += c.nominal - c.minus;
            max += c.nominal + c.plus;
        } else {
            min -= c.nominal + c.plus;
            max -= c.nominal - c.minus;
        }
    }
    let margin = (target.upper - max).min(min - target.lower);
    WorstCaseResult {
        min,
        max,
        margin,
        result: classify(margin, target, settings.marginal_fraction),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RssResult {
    pub mean: f64,
    /// Mean after the Bender shift; equals `mean` when mean_shift_k is 0.
    pub shifted_mean: f64,
    pub sigma: f64,
    pub sigma3: f64,
    pub cp: f64,
    pub cpk: f64,
    pub yield_percent: f64,
    /// Percent of total variance contributed by each input (sums to 100).
    pub sensitivity: Vec<f64>,
    pub margin: f64,
    pub result: PassFail,
}

/// RSS (root-sum-square) linear variance propagation assuming independent
/// inputs. Per-input sigma = (plus + minus) / sigma_level.
pub fn rss(inputs: &[UncertainInput], target: &Target, settings: &Settings) -> RssResult {
    let mean: f64 = inputs.iter().map(|c| c.sense * c.nominal).sum();
    let variances: Vec<f64> = inputs
        .iter()
        .map(|c| {
            let s = (c.plus + c.minus) / settings.sigma_level;
            s * s
        })
        .collect();
    let var_total: f64 = variances.iter().sum();
    let sigma = var_total.sqrt();

    let shifted_mean = if settings.mean_shift_k > 0.0 && sigma > 0.0 {
        // Shift toward the nearest specification limit (conservative).
        if (target.upper - mean) < (mean - target.lower) {
            mean + settings.mean_shift_k * sigma
        } else {
            mean - settings.mean_shift_k * sigma
        }
    } else {
        mean
    };

    let (cp, cpk) = if sigma > 0.0 {
        (
            (target.upper - target.lower) / (6.0 * sigma),
            ((target.upper - shifted_mean).min(shifted_mean - target.lower)) / (3.0 * sigma),
        )
    } else {
        (f64::INFINITY, f64::INFINITY)
    };

    let yield_percent = if sigma > 0.0 {
        let hi = normal_cdf((target.upper - shifted_mean) / sigma);
        let lo = normal_cdf((target.lower - shifted_mean) / sigma);
        (hi - lo) * 100.0
    } else if shifted_mean >= target.lower && shifted_mean <= target.upper {
        100.0
    } else {
        0.0
    };

    let sensitivity = if var_total > 0.0 {
        variances.iter().map(|v| v / var_total * 100.0).collect()
    } else {
        vec![0.0; inputs.len()]
    };

    let sigma3 = 3.0 * sigma;
    let margin = (target.upper - (shifted_mean + sigma3)).min((shifted_mean - sigma3) - target.lower);

    RssResult {
        mean,
        shifted_mean,
        sigma,
        sigma3,
        cp,
        cpk,
        yield_percent,
        sensitivity,
        margin,
        result: classify(margin, target, settings.marginal_fraction),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistogramBin {
    pub lower: f64,
    pub upper: f64,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MonteCarloResult {
    pub iterations: u64,
    /// The seed actually used — always recorded so runs are reproducible.
    pub seed: u64,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub yield_percent: f64,
    pub percentile_2_5: f64,
    pub percentile_97_5: f64,
    pub pp: f64,
    pub ppk: f64,
    pub result: PassFail,
    /// Sample distribution over [min, max] in equal-width bins.
    pub histogram: Vec<HistogramBin>,
}

/// Bin sorted samples into `bins` equal-width intervals over their range.
fn build_histogram(sorted: &[f64], bins: usize) -> Vec<HistogramBin> {
    if sorted.is_empty() {
        return Vec::new();
    }
    let (lo, hi) = (sorted[0], sorted[sorted.len() - 1]);
    if hi <= lo {
        return vec![HistogramBin {
            lower: lo,
            upper: hi,
            count: sorted.len() as u64,
        }];
    }
    let width = (hi - lo) / bins as f64;
    let mut out: Vec<HistogramBin> = (0..bins)
        .map(|i| HistogramBin {
            lower: lo + width * i as f64,
            upper: lo + width * (i + 1) as f64,
            count: 0,
        })
        .collect();
    for &x in sorted {
        let idx = (((x - lo) / width) as usize).min(bins - 1);
        out[idx].count += 1;
    }
    out
}

/// Seeded Monte Carlo sampling. Identical seed + identical inputs gives
/// bit-for-bit identical results (audit trails).
pub fn monte_carlo(
    inputs: &[UncertainInput],
    target: &Target,
    settings: &Settings,
    default_seed: u64,
) -> MonteCarloResult {
    let seed = settings.seed.unwrap_or(default_seed);
    let mut rng = Pcg32::new(seed);
    let n = settings.iterations.max(1);

    let mut samples: Vec<f64> = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let mut total = 0.0;
        for c in inputs {
            total += c.sense * sample(c, &mut rng);
        }
        samples.push(total);
    }

    let count = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / count;
    let var = samples.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / count;
    let std_dev = var.sqrt();
    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let in_spec = samples
        .iter()
        .filter(|&&x| x >= target.lower && x <= target.upper)
        .count() as f64;
    let yield_percent = in_spec / count * 100.0;

    let mut sorted = samples;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let percentile = |p: f64| -> f64 {
        let idx = (p / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };

    let (pp, ppk) = if std_dev > 0.0 {
        (
            (target.upper - target.lower) / (6.0 * std_dev),
            ((target.upper - mean).min(mean - target.lower)) / (3.0 * std_dev),
        )
    } else {
        (f64::INFINITY, f64::INFINITY)
    };

    let margin = (target.upper - max).min(min - target.lower);

    MonteCarloResult {
        iterations: n,
        seed,
        mean,
        std_dev,
        min,
        max,
        yield_percent,
        percentile_2_5: percentile(2.5),
        percentile_97_5: percentile(97.5),
        pp,
        ppk,
        result: classify(margin, target, settings.marginal_fraction),
        histogram: build_histogram(&sorted, 21),
    }
}

/// Draw one sample from an input's distribution. Normal is centered on the
/// nominal with sigma = (plus + minus) / 6 (the tolerance band treated as
/// +/-3 sigma); uniform and triangular span [nominal - minus, nominal + plus].
fn sample(c: &UncertainInput, rng: &mut Pcg32) -> f64 {
    let lo = c.nominal - c.minus;
    let hi = c.nominal + c.plus;
    match c.distribution {
        Distribution::Normal => {
            let sigma = (c.plus + c.minus) / 6.0;
            c.nominal + sigma * rng.next_gaussian()
        }
        Distribution::Uniform => lo + (hi - lo) * rng.next_f64(),
        Distribution::Triangular => {
            // Inverse-CDF sampling with the peak at nominal.
            let u = rng.next_f64();
            let range = hi - lo;
            if range <= 0.0 {
                return c.nominal;
            }
            let fc = (c.nominal - lo) / range;
            if u < fc {
                lo + (u * range * (c.nominal - lo)).sqrt()
            } else {
                hi - ((1.0 - u) * range * (hi - c.nominal)).sqrt()
            }
        }
    }
}

/// Standard normal CDF via the Abramowitz & Stegun 7.1.26 erf
/// approximation (max abs error ~1.5e-7) — good enough for yield
/// percentages, avoids a special-functions dependency.
fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    const A1: f64 = 0.254829592;
    const A2: f64 = -0.284496736;
    const A3: f64 = 1.421413741;
    const A4: f64 = -1.453152027;
    const A5: f64 = 1.061405429;
    const P: f64 = 0.3275911;
    let t = 1.0 / (1.0 + P * x);
    let y = 1.0 - (((((A5 * t + A4) * t) + A3) * t + A2) * t + A1) * t * (-x * x).exp();
    sign * y
}

/// PCG-XSH-RR 32 with 64-bit state: tiny, fast, statistically solid for
/// tolerance-analysis sampling, and dependency-free. Fixed increment.
struct Pcg32 {
    state: u64,
}

impl Pcg32 {
    const MULT: u64 = 6364136223846793005;
    const INC: u64 = 1442695040888963407;

    fn new(seed: u64) -> Self {
        let mut rng = Pcg32 { state: 0 };
        rng.state = rng.state.wrapping_add(seed).wrapping_add(Self::INC);
        rng.next_u32();
        rng
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULT).wrapping_add(Self::INC);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1).
    fn next_f64(&mut self) -> f64 {
        // 53 random bits, the full mantissa of an f64.
        let hi = (self.next_u32() as u64) << 21;
        let lo = (self.next_u32() as u64) >> 11;
        (hi | lo) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal via Box-Muller.
    fn next_gaussian(&mut self) -> f64 {
        let mut u1 = self.next_f64();
        if u1 <= f64::MIN_POSITIVE {
            u1 = f64::MIN_POSITIVE;
        }
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enclosure-gap chain from the domain-library examples:
    /// +50 +/-0.1 normal, -45 +/-0.08 normal, -2 +0.15/-0.10 uniform;
    /// target 3.0 in [2.5, 3.5].
    fn enclosure_inputs() -> Vec<UncertainInput> {
        vec![
            UncertainInput {
                name: "housingDepth".into(),
                nominal: 50.0,
                plus: 0.1,
                minus: 0.1,
                sense: 1.0,
                distribution: Distribution::Normal,
                source: None,
            },
            UncertainInput {
                name: "coverHeight".into(),
                nominal: 45.0,
                plus: 0.08,
                minus: 0.08,
                sense: -1.0,
                distribution: Distribution::Normal,
                source: None,
            },
            UncertainInput {
                name: "gasketThickness".into(),
                nominal: 2.0,
                plus: 0.15,
                minus: 0.10,
                sense: -1.0,
                distribution: Distribution::Uniform,
                source: None,
            },
        ]
    }

    fn target() -> Target {
        Target {
            nominal: 3.0,
            lower: 2.5,
            upper: 3.5,
        }
    }

    #[test]
    fn worst_case_hand_checked() {
        let wc = worst_case(&enclosure_inputs(), &target(), &Settings::default());
        // min = 49.9 - 45.08 - 2.15 = 2.67; max = 50.1 - 44.92 - 1.9 = 3.28
        assert!((wc.min - 2.67).abs() < 1e-9, "min = {}", wc.min);
        assert!((wc.max - 3.28).abs() < 1e-9, "max = {}", wc.max);
        // margin = min(3.5 - 3.28, 2.67 - 2.5) = 0.17 > 10% of band (0.1)
        assert!((wc.margin - 0.17).abs() < 1e-9);
        assert_eq!(wc.result, PassFail::Pass);
    }

    #[test]
    fn worst_case_fail_and_marginal() {
        let inputs = enclosure_inputs();
        // Tight limits: band [2.7, 3.2] -> max 3.28 exceeds USL -> fail
        let fail = worst_case(
            &inputs,
            &Target {
                nominal: 3.0,
                lower: 2.7,
                upper: 3.2,
            },
            &Settings::default(),
        );
        assert_eq!(fail.result, PassFail::Fail);
        // Barely-clearing limits: [2.65, 3.30] -> margin 0.02 < 10% of 0.65
        let marginal = worst_case(
            &inputs,
            &Target {
                nominal: 3.0,
                lower: 2.65,
                upper: 3.30,
            },
            &Settings::default(),
        );
        assert_eq!(marginal.result, PassFail::Marginal);

        // The same geometry with the model's acceptance policy relaxed:
        // a project that only wants PASS/FAIL sets marginalFraction to 0.
        let strict_off = worst_case(
            &inputs,
            &Target {
                nominal: 3.0,
                lower: 2.65,
                upper: 3.30,
            },
            &Settings {
                marginal_fraction: 0.0,
                ..Settings::default()
            },
        );
        assert_eq!(strict_off.result, PassFail::Pass);

        // ...and tightened: 10% was not enough for this reviewer.
        let stricter = worst_case(
            &inputs,
            &Target {
                nominal: 3.0,
                lower: 2.6,
                upper: 3.4,
            },
            &Settings {
                marginal_fraction: 0.5,
                ..Settings::default()
            },
        );
        assert_eq!(stricter.result, PassFail::Marginal);
    }

    #[test]
    fn rss_hand_checked() {
        let r = rss(&enclosure_inputs(), &target(), &Settings::default());
        assert!((r.mean - 3.0).abs() < 1e-9);
        // sigma = sqrt((0.2/6)^2 + (0.16/6)^2 + (0.25/6)^2) = 0.0596515...
        assert!((r.sigma - 0.059651).abs() < 1e-5, "sigma = {}", r.sigma);
        // Cp = Cpk = 1.0 / (6 * sigma) = 2.7940...
        assert!((r.cp - 2.7940).abs() < 1e-3, "cp = {}", r.cp);
        assert!((r.cpk - r.cp).abs() < 1e-9, "centered process: cpk == cp");
        // Sensitivity: 31.23%, 19.99%, 48.79% (variance shares)
        assert!((r.sensitivity[0] - 31.23).abs() < 0.05);
        assert!((r.sensitivity[1] - 19.99).abs() < 0.05);
        assert!((r.sensitivity[2] - 48.79).abs() < 0.05);
        let total: f64 = r.sensitivity.iter().sum();
        assert!((total - 100.0).abs() < 1e-6);
        assert!(r.yield_percent > 99.99);
        assert_eq!(r.result, PassFail::Pass);
    }

    #[test]
    fn rss_yield_tight_limits() {
        // Limits at +/-0.1 around the mean: z = 0.1/0.0596515 = 1.67641
        // yield = 2*Phi(z) - 1 = 90.647%
        let r = rss(
            &enclosure_inputs(),
            &Target {
                nominal: 3.0,
                lower: 2.9,
                upper: 3.1,
            },
            &Settings::default(),
        );
        assert!(
            (r.yield_percent - 90.647).abs() < 0.1,
            "yield = {}",
            r.yield_percent
        );
    }

    #[test]
    fn rss_bender_shift_is_conservative() {
        let plain = rss(&enclosure_inputs(), &target(), &Settings::default());
        let shifted = rss(
            &enclosure_inputs(),
            &target(),
            &Settings {
                mean_shift_k: 1.5,
                ..Settings::default()
            },
        );
        assert!((shifted.shifted_mean - (3.0 - 1.5 * plain.sigma)).abs() < 1e-9);
        assert!(shifted.cpk < plain.cpk, "shift must reduce Cpk");
        assert!(shifted.yield_percent < plain.yield_percent);
    }

    #[test]
    fn monte_carlo_reproducible_and_sane() {
        let s = Settings {
            seed: Some(12345),
            iterations: 20_000,
            ..Settings::default()
        };
        let a = monte_carlo(&enclosure_inputs(), &target(), &s, 0);
        let b = monte_carlo(&enclosure_inputs(), &target(), &s, 0);
        assert_eq!(a.seed, 12345);
        // Bit-for-bit reproducibility with a fixed seed.
        assert_eq!(a.mean.to_bits(), b.mean.to_bits());
        assert_eq!(a.std_dev.to_bits(), b.std_dev.to_bits());
        assert_eq!(a.percentile_97_5.to_bits(), b.percentile_97_5.to_bits());
        // Statistically sane: mean near 3.0 (uniform input is symmetric
        // about 2.025, so true mean is 2.975), spread near RSS sigma.
        assert!((a.mean - 2.975).abs() < 0.005, "mean = {}", a.mean);
        assert!(a.yield_percent > 99.9);
        assert!(a.min < a.percentile_2_5 && a.percentile_2_5 < a.mean);
        assert!(a.mean < a.percentile_97_5 && a.percentile_97_5 < a.max);
    }

    #[test]
    fn monte_carlo_different_seeds_differ() {
        let base = Settings {
            iterations: 1_000,
            ..Settings::default()
        };
        let a = monte_carlo(
            &enclosure_inputs(),
            &target(),
            &Settings {
                seed: Some(1),
                ..base
            },
            0,
        );
        let b = monte_carlo(
            &enclosure_inputs(),
            &target(),
            &Settings {
                seed: Some(2),
                ..base
            },
            0,
        );
        assert_ne!(a.mean.to_bits(), b.mean.to_bits());
    }

    #[test]
    fn erf_reference_values() {
        // erf(0) = 0, erf(1) = 0.8427008, erf(2) = 0.9953223
        assert!(erf(0.0).abs() < 1e-9);
        assert!((erf(1.0) - 0.8427008).abs() < 1e-6);
        assert!((erf(2.0) - 0.9953223).abs() < 1e-6);
        assert!((erf(-1.0) + 0.8427008).abs() < 1e-6);
    }

    #[test]
    fn uniform_sampling_bounds() {
        let mut rng = Pcg32::new(7);
        let c = UncertainInput {
            name: "u".into(),
            nominal: 2.0,
            plus: 0.15,
            minus: 0.10,
            sense: 1.0,
            distribution: Distribution::Uniform,
            source: None,
        };
        for _ in 0..10_000 {
            let x = sample(&c, &mut rng);
            assert!((1.9..=2.15).contains(&x), "out of bounds: {x}");
        }
    }

    #[test]
    fn triangular_sampling_bounds_and_mode() {
        let mut rng = Pcg32::new(11);
        let c = UncertainInput {
            name: "t".into(),
            nominal: 5.0,
            plus: 0.2,
            minus: 0.1,
            sense: 1.0,
            distribution: Distribution::Triangular,
            source: None,
        };
        let mut sum = 0.0;
        const N: usize = 50_000;
        for _ in 0..N {
            let x = sample(&c, &mut rng);
            assert!((4.9..=5.2).contains(&x), "out of bounds: {x}");
            sum += x;
        }
        // Triangular mean = (a + c + b)/3 = (4.9 + 5.0 + 5.2)/3 = 5.0333
        let mean = sum / N as f64;
        assert!((mean - 5.0333).abs() < 0.005, "mean = {mean}");
    }
}

#[cfg(test)]
mod histogram_tests {
    use super::*;

    #[test]
    fn histogram_bins_cover_all_samples() {
        let sorted: Vec<f64> = (0..1000).map(|i| i as f64 / 100.0).collect();
        let bins = build_histogram(&sorted, 21);
        assert_eq!(bins.len(), 21);
        assert_eq!(bins.iter().map(|b| b.count).sum::<u64>(), 1000);
        assert_eq!(bins[0].lower, 0.0);
        assert!((bins[20].upper - 9.99).abs() < 1e-9);
    }

    #[test]
    fn histogram_degenerate_range_single_bin() {
        let sorted = vec![5.0; 100];
        let bins = build_histogram(&sorted, 21);
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].count, 100);
    }

    #[test]
    fn monte_carlo_result_carries_histogram() {
        let inputs = vec![UncertainInput {
            name: "x".into(),
            nominal: 10.0,
            plus: 0.5,
            minus: 0.5,
            distribution: Distribution::Normal,
            sense: 1.0,
            source: None,
        }];
        let target = Target { nominal: 10.0, lower: 8.0, upper: 12.0 };
        let settings = Settings { iterations: 5000, seed: Some(7), ..Default::default() };
        let r = monte_carlo(&inputs, &target, &settings, 0);
        assert_eq!(r.histogram.iter().map(|b| b.count).sum::<u64>(), 5000);
        // Deterministic: same seed, same bins.
        let r2 = monte_carlo(&inputs, &target, &settings, 0);
        assert_eq!(r.histogram.len(), r2.histogram.len());
        assert!(r.histogram.iter().zip(&r2.histogram).all(|(a, b)| a.count == b.count));
    }
}
