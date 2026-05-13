use crate::parser::PerfData;
use crate::errors::SlugError;

type StatFunction = fn(&PerfData) -> Result<f64, SlugError>;

struct StatCalculator {
    name: &'static str,
    function: StatFunction,
    required_keys: Vec<&'static str>,
}

pub fn calculate_stats(perf_data: &mut PerfData) -> Result<(), SlugError> {
    let stat_functions = vec![
        StatCalculator {
            name: "average",
            function: calculate_average,
            required_keys: vec!["min", "max"],
        },
    ];

    for calculator in stat_functions {
        if calculator.required_keys.iter().all(|&key| perf_data.map.contains_key(key)) {
            if let Ok(result) = (calculator.function)(perf_data) {
                perf_data.map.insert(calculator.name.to_string(), result);
            }
        }
    }
    Ok(())
}

pub fn calculate_average(perf_data: &PerfData) -> Result<f64, SlugError> {
    let sum: f64 = perf_data.map.values().sum();
    let count = perf_data.map.len() as f64;
    Ok(sum / count)
}

// Exponential weighted moving average calculator
pub fn ewma_calc(values: &Vec<PerfData>, alpha: f64) -> Vec<f64> {
    let mut averages = Vec::new();

    let mut previous_average = values[0].map["mean"];

    averages.push(previous_average);

    for value in values.iter() { // TODO: add verbose option
        //println!("{}", value.map["mean"]);
        let current_average = alpha * value.map["mean"] + (1.0 - alpha) * previous_average;
        averages.push(current_average);
        previous_average = current_average;
    }

    averages
}

// Exponential weighted moving average evaluation
pub fn ewma(values: &Vec<PerfData>, alpha: f64) -> Result<(), SlugError> {
    if values.len() <= 1 {
        println!("\x1b[38;2;255;165;0mToo few samples for exponential moving average\x1b[0m");
        return Ok(());
    }

    let avg = ewma_calc(&values, alpha);

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