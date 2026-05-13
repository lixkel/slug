use crate::parser::PerfData;
use crate::errors::SlugError;

// Type signature for a function that evaluates history and returns an error if degradation is found
type StatEvaluator = fn(&[PerfData]) -> Result<(), SlugError>;

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

pub fn calculate_stats(history: &[PerfData]) -> Result<(), SlugError> {
    if history.is_empty() {
        return Ok(());
    }

    let mut checks: Vec<StatCheck> = Vec::new();

    add_stat_checks!(checks, {
        "ewma" => (evaluate_ewma, ["mean"]),
    });

    let latest = history.last().unwrap();

    for check in checks {
        if check.required_keys.iter().all(|&k| latest.map.contains_key(k)) {
            (check.evaluator)(history)?;
        }
    }

    Ok(())
}

fn evaluate_ewma(history: &[PerfData]) -> Result<(), SlugError> {
    ewma(history, 0.2)
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

    for value in values.iter() {
        let current_average = alpha * value.map[METRIC] + (1.0 - alpha) * previous_average;
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
