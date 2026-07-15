use crate::parser::PerfData;
use crate::errors::SlugError;
use crate::units;
use crate::config::{Config, Policy};
use statrs::distribution::{ContinuousCDF, StudentsT};
use statrs::statistics::Statistics;

// Outcome of one statistical check
pub struct CheckReport {
    pub check: &'static str,
    pub flagged: bool,
    // These are so far only used by tests
    #[allow(dead_code)]
    pub value: f64,     // statistic the check computed
    #[allow(dead_code)]
    pub threshold: f64, // limit value was judged against
    pub text: String,   // formatted report line, empty = silent
}

// What check produces
pub enum CheckVerdict {
    Skipped,                                                            // metric too sparse in the window
    TooFewSamples { check: &'static str, have: usize, need: usize },    // window below min_samples
    MissingMetric { check: &'static str, metric: &'static str },        // newest measurement lacks required metric
    Judged(CheckReport),
}

impl CheckVerdict {
    fn passed(check: &'static str, value: f64, threshold: f64, text: String) -> CheckVerdict {
        CheckVerdict::Judged(CheckReport { check, flagged: false, value, threshold, text })
    }

    pub fn flagged(check: &'static str, value: f64, threshold: f64, text: String) -> CheckVerdict {
        CheckVerdict::Judged(CheckReport { check, flagged: true, value, threshold, text })
    }
}

// Type signature for a function that judges newest measurement against history
type StatEvaluator = fn(&[PerfData], &Config) -> Result<CheckVerdict, SlugError>;

struct StatCheck {
    pub name: &'static str,
    pub evaluator: StatEvaluator,
    pub metric: &'static str,
    pub window: usize,
}

macro_rules! add_stat_checks {
    ($checks:ident, $config:ident, { $($name:expr => ($field:ident, $evaluator:ident, $metric:expr)),+ $(,)? }) => {
        $(
            $checks.push(
                StatCheck {
                    name: $name,
                    evaluator: $evaluator,
                    metric: $metric,
                    window: $config.$field.window,
                }
            );
        )+
    };
}

// Registry of implemented statistical tests
fn registry(config: &Config) -> Vec<StatCheck> {
    let mut checks: Vec<StatCheck> = Vec::new();

    add_stat_checks!(checks, config, {
        "zscore" => (zscore, evaluate_zscore, "mean"),
        "ewma" => (ewma, evaluate_ewma, "mean"),
        "prediction-bound" => (prediction_bound, evaluate_prediction_bound, "mean"),
    });

    checks
}

// Names of statistical tests config may contain
pub fn check_names() -> Vec<&'static str> {
    registry(&Config::default()).iter().map(|check| check.name).collect()
}

// Last n points of history
fn tail(history: &[PerfData], n: usize) -> &[PerfData] {
    &history[history.len().saturating_sub(n)..]
}

// Run checks selected in config, each over its own window
pub fn run_checks(history: &[PerfData], config: &Config) -> Result<Vec<CheckVerdict>, SlugError> {
    if history.is_empty() {
        return Ok(Vec::new());
    }

    let latest = history.last().unwrap();

    let mut verdicts: Vec<CheckVerdict> = Vec::new();
    for check in &registry(config) {
        if !config.check_is_on(check.name) {
            continue;
        }

        // Enabled check that cannot run still declines visibly
        if !latest.map.contains_key(check.metric) {
            verdicts.push(CheckVerdict::MissingMetric { check: check.name, metric: check.metric });
            continue;
        }

        let slice = tail(history, check.window);

        // Warm up floor
        if slice.len() < config.min_samples {
            verdicts.push(CheckVerdict::TooFewSamples { check: check.name, have: slice.len(), need: config.min_samples });
        } else {
            verdicts.push((check.evaluator)(slice, config)?);
        }
    }

    Ok(verdicts)
}

// Combine verdicts of statistical tests
pub fn combine(verdicts: &[CheckVerdict], policy: Policy) -> bool {
    let ran = verdicts.len();
    let mut flagged = 0;
    for verdict in verdicts {
        if let CheckVerdict::Judged(report) = verdict {
            if report.flagged {
                flagged += 1;
            }
        }
    }

    match policy {
        Policy::Any => flagged > 0,
        Policy::All => ran > 0 && flagged == ran,
    }
}

// Did the prediction-bound check flag last run commits in row, nothing stored
pub fn prediction_bound_run(history: &[PerfData], config: &Config) -> Result<Option<String>, SlugError> {
    let k = config.prediction_bound.run;

    if k < 2 || !config.check_is_on("prediction-bound") {
        return Ok(None);
    }
    if history.len() < k {
        return Ok(None);
    }

    // Walk down from newest commit
    let mut end = history.len();
    for _ in 0..k {
        if !prediction_bound_flagged(&history[..end], config)? {
            return Ok(None);
        }
        end -= 1;
    }

    Ok(Some(format!("prediction-bound check flagged the last {} commits in a row", k)))
}

fn prediction_bound_flagged(history: &[PerfData], config: &Config) -> Result<bool, SlugError> {
    let slice = tail(history, config.prediction_bound.window);

    if slice.len() < config.min_samples {
        return Ok(false);
    }
    if !slice.last().unwrap().map.contains_key("mean") {
        return Ok(false);
    }

    match evaluate_prediction_bound(slice, config)? {
        CheckVerdict::Judged(report) => Ok(report.flagged),
        _ => Ok(false),
    }
}

// Newest measurement of a metric plus the historical distribution it is judged against
struct Sample {
    current: f64,
    n: f64,
    mean: f64,
    std_dev: f64,
}

// Preprocess history
fn sample(history: &[PerfData], metric: &str) -> Option<Sample> {
    let (latest, past) = history.split_last().unwrap();
    let values: Vec<f64> = past.iter()
        .filter_map(|entry| entry.map.get(metric).copied())
        .collect();

    // Standard deviation needs >2 elements (N - 1)
    if values.len() < 2 {
        return None;
    }

    Some(Sample {
        current: latest.map[metric],
        n: values.len() as f64,
        mean: (&values).mean(),
        std_dev: (&values).std_dev(), // sample standard deviation (Bessel's correction)
    })
}

pub fn evaluate_zscore(history: &[PerfData], config: &Config) -> Result<CheckVerdict, SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC) else {
        return Ok(CheckVerdict::Skipped);
    };

    let threshold = config.zscore.threshold;

    // std_dev == 0.0
    if s.std_dev < f64::EPSILON {
        return Ok(if s.current > s.mean {
            CheckVerdict::flagged(
                "zscore",
                f64::INFINITY,
                threshold,
                format!("{} increased from flat baseline", METRIC),
            )
        } else {
            CheckVerdict::passed("zscore", 0.0, threshold, String::new())
        });
    }

    let z_score = (s.current - s.mean) / s.std_dev;

    let text = format!("{:.2} (threshold {:.1})", z_score, threshold);

    // Z-score > threshold execution time is 3 standard deviations worse than average
    if z_score > threshold {
        Ok(CheckVerdict::flagged("zscore", z_score, threshold, text))
    } else {
        Ok(CheckVerdict::passed("zscore", z_score, threshold, text))
    }
}

pub fn evaluate_prediction_bound(history: &[PerfData], config: &Config) -> Result<CheckVerdict, SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC) else {
        return Ok(CheckVerdict::Skipped);
    };

    // Highest value a healthy new measurement should reach:
    // mean + t(level, n-1) * std_dev * sqrt(1 + 1/n)
    // Student's t because mean and std-dev are just estimates from n points
    let t = StudentsT::new(0.0, 1.0, s.n - 1.0)
        .map_err(|e| SlugError::config(format!("prediction-bound check: {}", e)))?
        .inverse_cdf(config.prediction_bound.level);
    let upper_bound = s.mean + t * s.std_dev * (1.0 + 1.0 / s.n).sqrt();

    let level_pct = config.prediction_bound.level * 100.0;
    let bound = units::format_ns(upper_bound);
    let current = units::format_ns(s.current);

    if s.current > upper_bound {
        // How far the new measurement overshoots the recent average
        let above_pct = (s.current / s.mean - 1.0) * 100.0;
        let text = format!("{}, {:+.1}% above the recent average, {:.0}% bound {}", current, above_pct, level_pct, bound);
        Ok(CheckVerdict::flagged("prediction-bound", s.current, upper_bound, text))
    } else {
        let text = format!("{} within {:.0}% bound {}", current, level_pct, bound);
        Ok(CheckVerdict::passed("prediction-bound", s.current, upper_bound, text))
    }
}

// Exponential weighted moving average calculator
pub fn ewma_calc(values: &[PerfData], alpha: f64) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    const METRIC: &str = "mean";

    let mut averages = Vec::new();

    let mut previous_average = values[0].map[METRIC];

    averages.push(previous_average);

    for i in 1..values.len() {
        let current_average = alpha * values[i].map[METRIC] + (1.0 - alpha) * previous_average;
        averages.push(current_average);
        previous_average = current_average;
    }

    averages
}

// Exponential weighted moving average control chart
pub fn evaluate_ewma(history: &[PerfData], config: &Config) -> Result<CheckVerdict, SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC) else {
        return Ok(CheckVerdict::Skipped);
    };

    let alpha = config.ewma.alpha;

    // Smoothed level of the series, newest measurement folded in
    let level = *ewma_calc(history, alpha).last().unwrap();

    // At steady state the EWMA statistic has standard deviation
    // sd * sqrt(alpha / (2 - alpha)), so an L-sigma upper control limit is
    // mean + L * sd * sqrt(alpha / (2 - alpha)). One-sided: only slowdowns flag,
    // and a sustained shift accumulates in the smoothed level until it crosses.
    let ewma_sigma = s.std_dev * (alpha / (2.0 - alpha)).sqrt();
    let upper_limit = s.mean + config.ewma.limit * ewma_sigma;

    let level_fmt = units::format_ns(level);
    let limit_fmt = units::format_ns(upper_limit);

    if level > upper_limit {
        let above_pct = (level / s.mean - 1.0) * 100.0;
        let text = format!("smoothed {}, {:+.1}% above baseline, limit {} (L={:.1})", level_fmt, above_pct, limit_fmt, config.ewma.limit);
        Ok(CheckVerdict::flagged("ewma", level, upper_limit, text))
    } else {
        let text = format!("smoothed {} within limit {} (L={:.1})", level_fmt, limit_fmt, config.ewma.limit);
        Ok(CheckVerdict::passed("ewma", level, upper_limit, text))
    }
}
