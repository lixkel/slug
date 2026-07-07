// Naming convention
// -----------------
// Function name:  <library>_<major>_<minor>_<patch>    This is a Rust
//                 identifier, so hyphens become underscores, e.g.
//                 google_benchmark_1_8_3). The version is *minimum* library
//                 version the parser supports, The parser registry in
//                 parser.rs picks the closest parser whose version is <=
//                 than the one requsted.
//
// Registry key /  "name@version"   (e.g. `-t google-benchmark@1.8.3`) The name
// CLI flag:       keeps the library's name spelling and may contain hyphens,
//                 only the '@' separates name and version. Use the bare
//                 library name when it is distinctive (pytest, pyperf,
//                 criterion). Add language prefix only when the library's own
//                 name is generic (Go's `testing` -> `go-testing`).
//
// Which metrics to record
// ------------------------
// The most important metric is "mean" the statistical tests (z-score, EWMA)
// depend on it. Save every metric the tool prints (lower, upper, stddev, ...),
// as future statistical tests could use them.
//
// Writing a parser
// ----------------
// Give parse_rows a regex matching one row (a `name` group for the benchmark
// name, plus named group for each number) and list of Metrics (see
// Metric::new / Metric::fixed below), parse_rows does the rest
//
// See google-benchmark below as a typical example

use crate::parser::PerfData;
use crate::errors::SlugError;

use regex::Regex;

// Where a Metric's unit comes from:
//   Group - unit sits in the row, per match -pass- the capture group holding it
//   Fixed - unit is fixed/read from a header -pass- it as an owned string
enum Unit {
    Group(&'static str), // capture group name, e.g. "tunit"
    Fixed(String),       // fixed unit, e.g. read once from a header
}

// One number to pull out of each match, store it under `key`, read the value
// from capture group `value`, with unit resolved from `unit`
// Keys and group names are always known beforehand, so they are 'static'
struct Metric {
    key: &'static str,
    value: &'static str,
    unit: Unit,
}

impl Metric {
    // Unit read from a capture group, per match
    fn new(key: &'static str, value: &'static str, unit_group: &'static str) -> Self {
        Metric { key, value, unit: Unit::Group(unit_group) }
    }

    // Unit known up front (e.g. read once from a header)
    fn fixed(key: &'static str, value: &'static str, unit: impl Into<String>) -> Self {
        Metric { key, value, unit: Unit::Fixed(unit.into()) }
    }
}

fn group<'a>(caps: &'a regex::Captures, name: &str) -> Result<&'a str, SlugError> {
    caps.name(name)
        .map(|m| m.as_str())
        .ok_or_else(|| SlugError::parsing(format!("capture group '{}' not found", name)))
}

// Shared parser skeleton, loops through regex matches and build one PerfData
// per match, fail if nothing matched
// Regex must expose a `name` group plus a group for every Metric's value/unit
fn parse_rows(s: &str, re: &Regex, metrics: &[Metric]) -> Result<Vec<PerfData>, SlugError> {
    let mut results = Vec::new();

    for caps in re.captures_iter(s) {
        let mut data = PerfData::new(group(&caps, "name")?);

        // Walk rows
        for m in metrics {
            let value: f64 = group(&caps, m.value)?.replace(',', "").parse()?;
            let unit = match &m.unit {
                Unit::Group(g) => group(&caps, g)?,
                Unit::Fixed(u) => u.as_str(),
            };
            data.record(m.key, value, unit)?;
        }

        results.push(data);
    }

    if results.is_empty() {
        return Err(SlugError::Parsing("No matches found".to_string()));
    }

    Ok(results)
}

pub fn pytest_7_3_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // pytest-benchmark prints the time unit once in the header, then a table:
    //   Name (time in us)   Min      Max      Mean    StdDev   Median ...
    //   test_fib            600.44   1,160.78 710.69  43.09    711.07 ...
    // Numbers can contain many commas
    let unit_re = Regex::new(r"\(time in (?P<unit>\w+)\)")?;
    let unit = unit_re.captures(s)
        .map(|c| c["unit"].to_string())
        .ok_or_else(|| SlugError::Parsing("Could not find time unit in pytest-benchmark header".to_string()))?;

    // We expect 5 columns: min, max, mean, stddev, median
    let col = |g: &str| format!(r"(?P<{g}>[\d,]+\.\d+)(?:\s+\([\d.]+\))?");
    let row_re = Regex::new(&format!(
        r"(?m)^\s*(?P<name>\w+)\s+{}\s+{}\s+{}\s+{}\s+{}",
        col("min"), col("max"), col("mean"), col("stddev"), col("median")
    ))?;

    let cols = ["min", "max", "mean", "stddev", "median"];
    let metrics = cols.map(|c| Metric::fixed(c, c, &unit));

    parse_rows(s, &row_re, &metrics)
}

pub fn go_testing_1_26_4(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // go testing prints one line per benchmark:
    //   BenchmarkFib-16   37124   32459 ns/op   0 B/op   0 allocs/op
    // Name has thread count suffix which we drop
    let regex = Regex::new(
        r"(?m)^(?P<name>Benchmark\w+)(?:-\d+)?\s+\d+\s+(?P<value>[\d.]+)\s+(?P<unit>\S+/op)"
    )?;

    parse_rows(s, &regex, &[Metric::new("mean", "value", "unit")])
}

pub fn criterion_0_5_1(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // criterion prints confidence interval per benchmark:
    //   fib                     time:   [22.624 µs 22.702 µs 22.775 µs]
    // The numbers are lower bound, point estimate, upper bound
    let regex = Regex::new(
        r"(?m)^(?P<name>\w+)\s+time:\s+\[(?P<low>[\d.]+) (?P<lowunit>\S+) (?P<mid>[\d.]+) (?P<midunit>\S+) (?P<high>[\d.]+) (?P<highunit>\S+)\]"
    )?;

    parse_rows(s, &regex, &[
        Metric::new("lower", "low", "lowunit"),
        Metric::new("mean", "mid", "midunit"),
        Metric::new("upper", "high", "highunit"),
    ])
}

pub fn google_benchmark_1_8_3(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // google-benchmark prints all the data on one line per benchmark:
    //  BM_Fib          12194 ns        12150 ns        55767
    // columns are name, Time, CPU, Iterations
    let regex = Regex::new(
        r"(?m)^(?P<name>\S+)\s+(?P<time>[\d.]+)\s+(?P<tunit>\w+)\s+(?P<cpu>[\d.]+)\s+(?P<cunit>\w+)\s+\d+\s*$"
    )?;

    parse_rows(s, &regex, &[
        Metric::new("mean", "time", "tunit"),
        Metric::new("cpu", "cpu", "cunit"),
    ])
}

pub fn pyperf_2_7_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // pyperf scales unit per benchmark (line), so the unit must be read from each line
    // mean and stddev each carry their own unit
    let regex = Regex::new(
        r"(?P<name>\w+): Mean \+- std dev: (?P<mean>\d+(?:\.\d+)?) (?P<munit>\w+) \+- (?P<stddev>\d+(?:\.\d+)?) (?P<sunit>\w+)"
    )?;

    parse_rows(s, &regex, &[
        Metric::new("mean", "mean", "munit"),
        Metric::new("stddev", "stddev", "sunit"),
    ])
}

pub fn jmh_1_37_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // JMH prints a summary table at the end, one line per benchmark:
    //   Benchmark         Mode  Cnt      Score      Error  Units
    //   MyBenchmark.fib   avgt   25  23516.718 ± 1096.241  ns/op
    let regex = Regex::new(
        r"(?m)^(?P<name>\S+)\s+avgt\s+\d+\s+(?P<score>[\d.,]+)\s+±\s+(?P<error>[\d.,]+)\s+(?P<unit>\S+)\s*$"
    )?;

    parse_rows(s, &regex, &[
        Metric::new("mean", "score", "unit"),
        Metric::new("error", "error", "unit"),
    ])
}

pub fn benchmarkdotnet_0_14_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
    // BenchmarkDotNet prints a markdown summary table, one row per benchmark:
    //   | Method    | Mean      | Error     | StdDev    |
    //   | FibBench  | 24.783 us | 0.1982 us | 0.2646 us |
    let regex = Regex::new(
        r"(?m)^\|\s*(?P<name>\w+)\s*\|\s*(?P<mean>[\d.,]+)\s+(?P<munit>\w+)\s*\|\s*(?P<error>[\d.,]+)\s+(?P<eunit>\w+)\s*\|\s*(?P<stddev>[\d.,]+)\s+(?P<sunit>\w+)\s*\|"
    )?;

    parse_rows(s, &regex, &[
        Metric::new("mean", "mean", "munit"),
        Metric::new("error", "error", "eunit"),
        Metric::new("stddev", "stddev", "sunit"),
    ])
}
