use crate::parser::PerfData;
use crate::errors::SlugError;
use crate::cli::CliOptions;
use crate::units;
use crate::config::{Config, Policy};
use statrs::distribution::{ContinuousCDF, StudentsT};
use statrs::statistics::Statistics;

// Outcome of one statistical check
pub struct CheckReport {
    pub flagged: bool,
    // These are so far only used by tests
    #[allow(dead_code)]
    pub value: f64,           // statistic the check computed
    #[allow(dead_code)]
    pub threshold: f64,       // limit the value was judged against
    pub line: Option<String>, // verdict sentence, None = silent
}

// What check produces
pub enum CheckVerdict {
    Skipped,                                                            // metric too sparse in the window
    TooFewSamples { check: &'static str, have: usize, need: usize },    // window below min_samples
    MissingMetric { check: &'static str, metric: &'static str },        // newest measurement lacks required metric
    Judged(CheckReport),
}

impl CheckVerdict {
    fn passed(value: f64, threshold: f64, line: Option<String>) -> CheckVerdict {
        CheckVerdict::Judged(CheckReport { flagged: false, value, threshold, line })
    }

    fn flagged(value: f64, threshold: f64, line: String) -> CheckVerdict {
        CheckVerdict::Judged(CheckReport { flagged: true, value, threshold, line: Some(line) })
    }

    fn flag_reason(&self) -> Option<String> {
        match self {
            CheckVerdict::Judged(report) if report.flagged => report.line.clone(),
            _ => None,
        }
    }
}

// Type signature for a function that judges newest measurement against history
type StatEvaluator = fn(&[PerfData], &CliOptions) -> Result<CheckVerdict, SlugError>;

struct StatCheck {
    pub name: &'static str,
    pub evaluator: StatEvaluator,
    pub metric: &'static str,
    pub window: usize,
}

macro_rules! add_stat_checks {
    ($checks:ident, $config:ident, { $($name:ident => ($evaluator:ident, $metric:expr)),+ $(,)? }) => {
        $(
            $checks.push(
                StatCheck {
                    name: stringify!($name),
                    evaluator: $evaluator,
                    metric: $metric,
                    window: $config.$name.window,
                }
            );
        )+
    };
}

// Registry of implemented statistical tests
fn registry(config: &Config) -> Vec<StatCheck> {
    let mut checks: Vec<StatCheck> = Vec::new();

    add_stat_checks!(checks, config, {
        ewma => (evaluate_ewma, "mean"),
        zscore => (evaluate_zscore, "mean"),
        confidence => (evaluate_confidence, "mean"),
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
pub fn run_checks(history: &[PerfData], options: &CliOptions, config: &Config) -> Result<Vec<CheckVerdict>, SlugError> {
    if history.is_empty() {
        return Ok(Vec::new());
    }

    let latest = history.last().unwrap();

    let mut verdicts: Vec<CheckVerdict> = Vec::new();
    for check in &registry(config) {
        if !config.enabled.iter().any(|name| name == check.name) {
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
            verdicts.push((check.evaluator)(slice, options)?);
        }
    }

    Ok(verdicts)
}

// Combined outcome of all enabled checks for one benchmark
pub struct TestReport {
    pub flagged: bool,
    pub reasons: Vec<String>,
}

// Combine verdicts of statistical tests
pub fn combine(verdicts: &[CheckVerdict], policy: Policy) -> TestReport {
    let ran = verdicts.len();
    let reasons: Vec<String> = verdicts.iter().filter_map(CheckVerdict::flag_reason).collect();

    let flagged = match policy {
        Policy::Any => !reasons.is_empty(),
        Policy::All => ran > 0 && reasons.len() == ran,
    };

    TestReport { flagged, reasons }
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

pub fn evaluate_zscore(history: &[PerfData], options: &CliOptions) -> Result<CheckVerdict, SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC) else {
        return Ok(CheckVerdict::Skipped);
    };

    // std_dev == 0.0
    if s.std_dev < f64::EPSILON {
        return Ok(if s.current > s.mean {
            CheckVerdict::flagged(
                f64::INFINITY,
                options.zscore_threshold,
                format!("Z-Score: {} increased from flat baseline", METRIC),
            )
        } else {
            CheckVerdict::passed(0.0, options.zscore_threshold, None)
        });
    }

    let z_score = (s.current - s.mean) / s.std_dev;

    // Z-score > threshold execution time is 3 standard deviations worse than average
    if z_score > options.zscore_threshold {
        Ok(CheckVerdict::flagged(
            z_score,
            options.zscore_threshold,
            format!("Z-Score anomaly detected ({:.2}, threshold {:.1})", z_score, options.zscore_threshold),
        ))
    } else {
        Ok(CheckVerdict::passed(
            z_score,
            options.zscore_threshold,
            Some(format!("Z-Score within norm ({:.2})", z_score)),
        ))
    }
}

pub fn evaluate_confidence(history: &[PerfData], options: &CliOptions) -> Result<CheckVerdict, SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC) else {
        return Ok(CheckVerdict::Skipped);
    };

    // Highest value a healthy new measurement should reach:
    // mean + t(level, n-1) * std_dev * sqrt(1 + 1/n)
    // Student's t because mean and std-dev are just estimates from n points
    let t = StudentsT::new(0.0, 1.0, s.n - 1.0)
        .map_err(|e| SlugError::config(format!("confidence check: {}", e)))?
        .inverse_cdf(options.confidence_level);
    let upper_bound = s.mean + t * s.std_dev * (1.0 + 1.0 / s.n).sqrt();

    if s.current > upper_bound {
        Ok(CheckVerdict::flagged(
            s.current,
            upper_bound,
            format!("{} {} is {:+.1}% above the recent average ({:.0}% upper bound {})",
                METRIC, units::format_ns(s.current), (s.current / s.mean - 1.0) * 100.0,
                options.confidence_level * 100.0, units::format_ns(upper_bound)),
        ))
    } else {
        Ok(CheckVerdict::passed(
            s.current,
            upper_bound,
            Some(format!("{} {} within the {:.0}% confidence interval (upper bound {})",
                METRIC, units::format_ns(s.current), options.confidence_level * 100.0, units::format_ns(upper_bound))),
        ))
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

// Exponential weighted moving average evaluation
pub fn evaluate_ewma(history: &[PerfData], options: &CliOptions) -> Result<CheckVerdict, SlugError> {
    let avg = ewma_calc(history, options.ewma_alpha);

    // Calculate percentual change in last two values
    let len = avg.len();
    let change = (avg[len - 1] - avg[len - 2]) / avg[len - 2];

    // Comparing this way ensures NaN flags
    if change < options.ewma_threshold {
        Ok(CheckVerdict::passed(
            change,
            options.ewma_threshold,
            Some(format!("EWMA change {:+.1}% (threshold {:.0}%)", change * 100.0, options.ewma_threshold * 100.0)),
        ))
    } else {
        Ok(CheckVerdict::flagged(
            change,
            options.ewma_threshold,
            format!("EWMA change {:+.1}% exceeds threshold {:.0}%", change * 100.0, options.ewma_threshold * 100.0),
        ))
    }
}
