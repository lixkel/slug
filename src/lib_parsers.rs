use crate::git;
use crate::parser::PerfData;
use crate::errors::SlugError;

use regex::Regex;

pub fn pytest_7_3_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // pytest-benchmark prints the time unit once in header
    let unit_re = Regex::new(r"\(time in (?P<unit>\w+)\)")?;
    let unit = unit_re.captures(s)
        .map(|c| c["unit"].to_string())
        .ok_or_else(|| SlugError::Parsing("Could not find time unit in pytest-benchmark header".to_string()))?;

    // We expect 5 columns: min, max, mean, stddev, median
    // The regex matches numbers that look like this:
    // "12.34"
    // "1,234.56"           (can have commas)
    // "12.34 (1.0)"        (pytest sometimes adds extra stuff in parentheses)
    let num = r"([\d,]+\.\d+)(?:\s+\([\d.]+\))?";
    let row_re = Regex::new(&format!(r"(?m)^\s*(?P<name>\w+)\s+{n}\s+{n}\s+{n}\s+{n}\s+{n}", n = num))?;

    let metrics = ["min", "max", "mean", "stddev", "median"];
    let commit_hash = git::get_commit_hash()?;
    let mut results = Vec::new();

    for caps in row_re.captures_iter(s) {
        let mut data = PerfData::new(&caps["name"], &commit_hash);

        for (i, metric) in metrics.iter().enumerate() {
            // Actual numbers start from group 2
            let raw = caps.get(i + 2).unwrap().as_str().replace(',', "");
            let value: f64 = raw.parse()?;
            data.record(metric, value, &unit)?;
        }

        results.push(data);
    }

    if results.is_empty() {
        return Err(SlugError::Parsing("No matches found".to_string()));
    }

    Ok(results)
}

pub fn go_testing_1_26_4(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // go testing prints one line per benchmark:
    //   BenchmarkFib-16   37124   32459 ns/op   0 B/op   0 allocs/op
    // Name has thread count suffix which we drop
    let regex = Regex::new(
        r"(?m)^(?P<name>Benchmark\w+)(?:-\d+)?\s+\d+\s+(?P<value>[\d.]+)\s+(?P<unit>\S+/op)"
    )?;

    let commit_hash = git::get_commit_hash()?;
    let mut results = Vec::new();

    for found in regex.captures_iter(s) {
        let mut data = PerfData::new(&found["name"], &commit_hash);

        let value: f64 = found["value"].parse()?;

        data.record("mean", value, &found["unit"])?;

        results.push(data);
    }

    if results.is_empty() {
        return Err(SlugError::Parsing("No matches found".to_string()));
    }

    Ok(results)
}

pub fn criterion_0_5_1(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // criterion prints confidence interval per benchmark:
    //   fib                     time:   [22.624 µs 22.702 µs 22.775 µs]
    // The numbers are lower bound, point estimate, upper bound
    let regex = Regex::new(
        r"(?m)^(?P<name>\w+)\s+time:\s+\[(?P<low>[\d.]+) (?P<lowunit>\S+) (?P<mid>[\d.]+) (?P<midunit>\S+) (?P<high>[\d.]+) (?P<highunit>\S+)\]"
    )?;

    let commit_hash = git::get_commit_hash()?;
    let mut results = Vec::new();

    for found in regex.captures_iter(s) {
        let mut data = PerfData::new(&found["name"], &commit_hash);

        data.record("lower", found["low"].parse()?, &found["lowunit"])?;
        data.record("mean", found["mid"].parse()?, &found["midunit"])?;
        data.record("upper", found["high"].parse()?, &found["highunit"])?;

        results.push(data);
    }

    if results.is_empty() {
        return Err(SlugError::Parsing("No matches found".to_string()));
    }

    Ok(results)
}

pub fn pyperf_2_7_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // pyperf scales unit per benchmark (line), so the unit must be read from each line
    // mean and stddev each carry their own unit
    let regex = Regex::new(
        r"(?P<name>\w+): Mean \+- std dev: (?P<mean>\d+(?:\.\d+)?) (?P<munit>\w+) \+- (?P<stddev>\d+(?:\.\d+)?) (?P<sunit>\w+)"
    )?;

    let commit_hash = git::get_commit_hash()?;
    let mut results = Vec::new();

    for found in regex.captures_iter(s) {
        let mut data = PerfData::new(&found["name"], &commit_hash);

        let mean: f64 = found["mean"].parse()?;
        let stddev: f64 = found["stddev"].parse()?;

        data.record("mean", mean, &found["munit"])?;
        data.record("stddev", stddev, &found["sunit"])?;

        results.push(data);
    }

    if results.is_empty() {
        return Err(SlugError::Parsing("No matches found".to_string()));
    }

    Ok(results)
}
