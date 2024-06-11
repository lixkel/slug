use crate::parser::PerfData;

use std::error::Error;

type StatFunction = fn(&PerfData) -> Result<f64, Box<dyn Error>>;

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

pub fn calculate_average(perf_data: &PerfData) -> Result<f64, Box<dyn Error>> {
    let sum: f64 = perf_data.map.values().sum();
    let count = perf_data.map.len() as f64;
    Ok(sum / count)
}


//exponential weighted moving average
pub fn ewma_calc(values: &Vec<PerfData>, alpha: f64) -> Vec<f64> {
    let mut averages = Vec::new();

    let mut previous_average = match values.last() {
        Some(val) => val.map["mean"],
        None => return averages, // If empty
    };

    averages.push(previous_average);

    for value in values.iter().rev() {
        let current_average = alpha * value.map["mean"] + (1.0 - alpha) * previous_average;
        averages.push(current_average);
        previous_average = current_average;
    }

    averages
}

//exponential weighted moving average
pub fn ewma(values: &Vec<PerfData>, alpha: f64) {
    let avg = ewma_calc(&values, alpha);

    if avg.len() <= 1 {
        println!("Too few samples for exponential moving average");
        return;
    }
    
    let len = avg.len();
    let change = (avg[len-1]-avg[len-2])/avg[len-2];

    if change < 0.2 {
        println!("All within norm in exponential moving average");
        return;
    }

    println!("Significant performance degradation");
}