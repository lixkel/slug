use std::collections::HashMap;
use std::error::Error;

type StatFunction = fn(&PerfData) -> Result<f32, Box<dyn Error>>;

struct StatCalculator {
    function: StatFunction,
    required_keys: Vec<&'static str>,
}

pub fn calculate_stats(perf_data: &PerfData) -> Result<HashMap<String, f32>, Box<dyn Error>> {
    let stat_functions = vec![
        StatCalculator {
            function: calculate_average,
            required_keys: vec!["min", "max"],
        },
    ];

    let mut stats = HashMap::new();

    for calculator in stat_functions {
        if calculator.required_keys.iter().all(|&key| perf_data.contains_key(key)) {
            if let Ok(result) = (calculator.function)(perf_data) {
                stats.insert(format!("{:?}", calculator.function), result);
            }
        }
    }

    Ok(stats)
}

pub fn calculate_average(perf_data: &PerfData) -> Result<f32, Box<dyn Error>> {
    let sum: f32 = perf_data.values().sum();
    let count = perf_data.len() as f32;
    Ok(sum / count)
}