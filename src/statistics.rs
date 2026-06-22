use crate::parser::PerfData;
use crate::errors::SlugError;
use crate::cli::CliOptions;
use crate::config::{Config, Policy};

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
        Err(SlugError::PerformanceRegression(regressions.join("; ")))
    } else {
        Ok(())
    }
}

fn evaluate_ewma(history: &[PerfData], options: &CliOptions) -> Result<(), SlugError> {
    ewma(history, options.ewma_alpha)
}

fn evaluate_zscore(history: &[PerfData], options: &CliOptions) -> Result<(), SlugError> {
    const METRIC: &str = "mean";
    
    // We need >2 history items to calculate std_dev
    if history.len() < 3 {
        println!("\x1b[38;2;255;165;0mToo few samples for Z-Score anomaly detection\x1b[0m");
        return Ok(());
    }

    let latest = history.last().unwrap();
    let historical_entries = &history[..history.len() - 1];

    let current_val = latest.map[METRIC];
    let values: Vec<f64> = historical_entries.iter()
        .filter_map(|entry| entry.map.get(METRIC).copied())
        .collect();

    // Standard deviation needs >2 elements (N - 1)
    if values.len() < 2 {
        return Ok(());
    }

    let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
    
    // Calculate sample standard deviation
    let mut variance_sum = 0.0;
    for v in &values {
        let diff = v - mean;
        variance_sum += diff * diff;
    }
    let variance = variance_sum / (values.len() - 1) as f64; // Bessel's correction
    let std_dev = variance.sqrt();

    // std_dev == 0.0
    if std_dev < f64::EPSILON {
        if current_val > mean {
             return Err(SlugError::PerformanceRegression(
                format!("Z-Score: {} increased from flat baseline", METRIC)
            ));
        }
        return Ok(());
    }

    let z_score = (current_val - mean) / std_dev;

    // Z-score > threshold execution time is 3 standard deviations worse than average
    if z_score > options.zscore_threshold {
        println!("\x1b[31mZ-Score anomaly detected!!!\x1b[0m");
        return Err(SlugError::PerformanceRegression(
            format!("Z-Score for {} is {:.2} (threshold {:.1})", METRIC, z_score, options.zscore_threshold)
        ));
    }

    println!("\x1b[32mZ-Score within norm ({:.2}) for {}\x1b[0m", z_score, METRIC);
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
pub fn ewma(values: &[PerfData], alpha: f64) -> Result<(), SlugError> {
    if values.len() <= 1 {
        println!("\x1b[38;2;255;165;0mToo few samples for exponential moving average\x1b[0m");
        return Ok(());
    }

    let avg = ewma_calc(values, alpha);

    // Calculate percentual change in last two values
    let len = avg.len();
    let change = (avg[len-1]-avg[len-2])/avg[len-2];

    if change < 0.2 {
        println!("\x1b[32mAll within norm in exponential moving average\x1b[0m");
        Ok(())
    } else {
        println!("\x1b[31mSignificant performance degradation!!!\x1b[0m");
        Err(SlugError::PerformanceRegression(
            format!("Performance degraded by {:.2}%", change * 100.0)
        ))
    }
}
