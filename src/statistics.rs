use crate::parser::PerfData;
use crate::errors::SlugError;
use crate::cli::CliOptions;
use crate::config::{Config, Policy};
use statrs::distribution::{ContinuousCDF, StudentsT};
use statrs::statistics::Statistics;

// Type signature for a function that evaluates history and returns an error if degradation is found
type StatEvaluator = fn(&[PerfData], &CliOptions) -> Result<(), SlugError>;

struct StatCheck {
    pub name: &'static str,
    pub evaluator: StatEvaluator,
    pub required_keys: Vec<&'static str>,
}

macro_rules! add_stat_checks {
    ($checks:ident, { $($name:expr => ($evaluator:ident, [$($req:expr),*])),+ $(,)? }) => {
        $(
            $checks.push(
                StatCheck {
                    name: $name,
                    evaluator: $evaluator,
                    required_keys: vec![$($req),*],
                }
            );
        )+
    };
}

pub fn calculate_stats(history: &[PerfData], options: &CliOptions, config: &Config) -> Result<(), SlugError> {
    if history.is_empty() {
        return Ok(());
    }

    let mut checks: Vec<StatCheck> = Vec::new();

    add_stat_checks!(checks, {
        "ewma" => (evaluate_ewma, ["mean"]),
        "zscore" => (evaluate_zscore, ["mean"]),
        "confidence" => (evaluate_confidence, ["mean"]),
    });

    let latest = history.last().unwrap();

    // Run the checks selected in config, each over its own window of recent points
    let mut verdicts: Vec<Result<(), SlugError>> = Vec::new();
    for check in &checks {
        if !config.enabled.iter().any(|name| name == check.name) {
            continue;
        }
        if check.required_keys.iter().all(|&k| latest.map.contains_key(k)) {
            let window = match check.name {
                "zscore" => config.zscore.window,
                "ewma" => config.ewma.window,
                "confidence" => config.confidence.window,
                _ => history.len(),
            };
            let start = history.len().saturating_sub(window);
            verdicts.push((check.evaluator)(&history[start..], options));
        }
    }

    combine(verdicts, config.policy)
}

// Combine the enabled checks verdicts into pass/fail based on selected policy
fn combine(verdicts: Vec<Result<(), SlugError>>, policy: Policy) -> Result<(), SlugError> {
    let mut regressions: Vec<String> = Vec::new();
    let mut ran = 0;

    for verdict in verdicts {
        ran += 1;
        match verdict {
            Ok(()) => {}
            Err(SlugError::PerformanceRegression(msg)) => regressions.push(msg),
            Err(other) => return Err(other),
        }
    }

    let flag = match policy {
        Policy::Any => !regressions.is_empty(),
        Policy::All => ran > 0 && regressions.len() == ran,
    };

    if flag {
        Err(SlugError::regression(regressions.join("; ")))
    } else {
        Ok(())
    }
}

fn evaluate_ewma(history: &[PerfData], options: &CliOptions) -> Result<(), SlugError> {
    ewma(history, options.ewma_alpha, options.ewma_threshold)
}

// Newest measurement of a metric plus the historical distribution it is judged against
struct Sample {
    current: f64,
    n: f64,
    mean: f64,
    std_dev: f64,
}

// Preprocess history
// Returns None if history is too short for meaningful verdict
fn sample(history: &[PerfData], metric: &str, check_name: &str) -> Option<Sample> {
    // We need some baseline before std-dev is meaningful
    if history.len() < 10 {
        println!("\x1b[38;2;255;165;0mToo few samples for {}\x1b[0m", check_name);
        return None;
    }

    let latest = history.last().unwrap();
    let values: Vec<f64> = history[..history.len() - 1].iter()
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

fn evaluate_zscore(history: &[PerfData], options: &CliOptions) -> Result<(), SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC, "Z-Score anomaly detection") else {
        return Ok(());
    };

    // std_dev == 0.0
    if s.std_dev < f64::EPSILON {
        if s.current > s.mean {
             return Err(SlugError::regression(
                format!("Z-Score: {} increased from flat baseline", METRIC)
            ));
        }
        return Ok(());
    }

    let z_score = (s.current - s.mean) / s.std_dev;

    // Z-score > threshold execution time is 3 standard deviations worse than average
    if z_score > options.zscore_threshold {
        println!("\x1b[31mZ-Score anomaly detected ({:.2}, threshold {:.1}) !!!\x1b[0m", z_score, options.zscore_threshold);
        return Err(SlugError::regression(
            format!("Z-Score for {} is {:.2} (threshold {:.1})", METRIC, z_score, options.zscore_threshold)
        ));
    }

    println!("\x1b[32mZ-Score within norm ({:.2})\x1b[0m", z_score);
    Ok(())
}

fn evaluate_confidence(history: &[PerfData], options: &CliOptions) -> Result<(), SlugError> {
    const METRIC: &str = "mean";

    let Some(s) = sample(history, METRIC, "confidence interval check") else {
        return Ok(());
    };

    // Highest value a healthy new measurement should reach:
    // mean + t(level, n-1) * std_dev * sqrt(1 + 1/n)
    // Student's t because mean and std-dev are just estimates from n points
    let t = StudentsT::new(0.0, 1.0, s.n - 1.0)
        .map_err(|e| SlugError::config(format!("confidence check: {}", e)))?
        .inverse_cdf(options.confidence_level);
    let upper_bound = s.mean + t * s.std_dev * (1.0 + 1.0 / s.n).sqrt();

    if s.current > upper_bound {
        println!("\x1b[31m{} {:.2} outside the {:.0}% confidence interval (upper bound {:.2}) !!!\x1b[0m", METRIC, s.current, options.confidence_level * 100.0, upper_bound);
        return Err(SlugError::regression(
            format!("{} {:.2} above the {:.0}% confidence upper bound {:.2}", METRIC, s.current, options.confidence_level * 100.0, upper_bound)
        ));
    }

    println!("\x1b[32m{} within the {:.0}% confidence interval (upper bound {:.2})\x1b[0m", METRIC, options.confidence_level * 100.0, upper_bound);
    Ok(())
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
pub fn ewma(values: &[PerfData], alpha: f64, threshold: f64) -> Result<(), SlugError> {
    // Same baseline requirement as the other checks
    if values.len() < 10 {
        println!("\x1b[38;2;255;165;0mToo few samples for exponential moving average\x1b[0m");
        return Ok(());
    }

    let avg = ewma_calc(values, alpha);

    // Calculate percentual change in last two values
    let len = avg.len();
    let change = (avg[len-1]-avg[len-2])/avg[len-2];

    if change < threshold {
        println!("\x1b[32mEWMA change {:+.1}% (threshold {:.0}%)\x1b[0m", change * 100.0, threshold * 100.0);
        Ok(())
    } else {
        println!("\x1b[31mEWMA change {:+.1}% (threshold {:.0}%) !!!\x1b[0m", change * 100.0, threshold * 100.0);
        Err(SlugError::regression(
            format!("EWMA change {:+.1}% exceeds threshold {:.0}%", change * 100.0, threshold * 100.0)
        ))
    }
}
