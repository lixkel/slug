use crate::parser::PerfData;

use std::error::Error;

type StatFunction = fn(&PerfData) -> Result<f32, Box<dyn Error>>;

struct StatCalculator {
    name: &'static str,
    function: StatFunction,
    required_keys: Vec<&'static str>,
}

pub fn calculate_stats(perf_data: &mut PerfData) -> Result<(), Box<dyn Error>> {
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

pub fn calculate_average(perf_data: &PerfData) -> Result<f32, Box<dyn Error>> {
    let sum: f32 = perf_data.map.values().sum();
    let count = perf_data.map.len() as f32;
    Ok(sum / count)
}