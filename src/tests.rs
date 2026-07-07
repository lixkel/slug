// Unit tests covering:
//  - Unit normalization (units.rs)
//  - Library name and version parsing (parser.rs)
//  - Parsers, end to end testing against real output
//  - Statistical checks (statistics.rs)
//  - slug.toml parsing (config.rs)

use crate::cli::{self, CliOptions};
use crate::config::{Config, Policy};
use crate::dbm_git::Store;
use crate::errors::SlugError;
use crate::parser::{self, Lib, PerfData, Version};
use crate::statistics::calculate_stats;
use crate::units;
use std::collections::HashMap;

// Checks if two floats are "close enough" to being equal
fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-6 * expected.abs().max(1.0)
}

// Absolute path to a fixture under samples/
fn fixture(relative: &str) -> String {
    format!("{}/samples/{}", env!("CARGO_MANIFEST_DIR"), relative)
}

// Fully parse and process fixture test file with the given `name@version` library string
fn parse_fixture(lib: &str, relative: &str) -> Result<Vec<PerfData>, SlugError> {
    let path = Some(fixture(relative));
    let reader = cli::get_reader(&path)?;
    let lib = Lib::from_str(lib).expect("library string needs to be valid");
    parser::parse(reader, &lib)
}

// Finds a specific benchmark by name, fails if it is missing
fn find_benchmark<'a>(data: &'a [PerfData], name: &str) -> &'a PerfData {
    data.iter().find(|d| d.name == name).expect("benchmark not found in parsed output")
}

// One historical benchmark record carrying only the given metric
fn point(metric: &str, value: f64) -> PerfData {
    let mut map = HashMap::new();
    map.insert(metric.to_string(), value);
    PerfData { name: "bench".to_string(), commit_hash: "0000000".to_string(), map }
}

// Twenty points alternating +-1 ns around 100 ns
fn noisy_baseline() -> Vec<PerfData> {
    (0..20)
        .map(|i| point("mean", 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 }))
        .collect()
}

// Slug default run options
fn options() -> CliOptions {
    let config = Config::default();
    CliOptions {
        file: None,
        library_type: None,
        storage: Store::Local,
        write: false,
        subcommand: None,
        target: None,
        zscore_threshold: config.zscore.threshold,
        ewma_alpha: config.ewma.alpha,
        ewma_threshold: config.ewma.threshold,
        confidence_level: config.confidence.level,
    }
}

// Slug default config with only the given checks enabled
fn config_with(enabled: &[&str]) -> Config {
    let mut config = Config::default();
    config.enabled = Vec::new();
    for name in enabled {
        config.enabled.push(name.to_string());
    }
    config
}

// Fails unless the verdict is specifically a performance regression
fn assert_regression(verdict: Result<(), SlugError>) {
    match verdict {
        Err(SlugError::PerformanceRegression(_)) => {}
        _ => panic!("expected PerformanceRegression verdict"),
    }
}

// Unit normalization

#[test]
fn units_normalize_to_nanoseconds() {
    assert!(close(units::normalize(1.0, "ns").unwrap(), 1.0));
    assert!(close(units::normalize(1.0, "us").unwrap(), 1_000.0));
    assert!(close(units::normalize(1.0, "ms").unwrap(), 1_000_000.0));
    assert!(close(units::normalize(1.0, "s").unwrap(), 1_000_000_000.0));
}

#[test]
fn units_lookup_case_insensitive() {
    assert!(close(units::normalize(2.0, "NS").unwrap(), 2.0));
    assert!(close(units::normalize(2.0, "US").unwrap(), 2_000.0));
    assert!(close(units::normalize(2.0, "MS").unwrap(), 2_000_000.0));
    assert!(close(units::normalize(2.0, "S").unwrap(), 2_000_000_000.0));
}

#[test]
fn units_lookup_trims() {
    assert!(close(units::normalize(2.0, "ns ").unwrap(), 2.0));
    assert!(close(units::normalize(2.0, " ms ").unwrap(), 2_000_000.0));
}

#[test]
fn units_unknown_fail_loud() {
    assert!(units::normalize(1.0, "klii").is_err());
}

// Library name and version parsing

#[test]
fn version_parsing() {
    assert!(Version::from_str("2.7.0").is_some());
    assert!(Version::from_str("2.7").is_none());
    assert!(Version::from_str("a.b.c").is_none());
}

#[test]
fn version_ordering_is_semantic() {
    let v = |s| Version::from_str(s).unwrap();
    assert!(v("1.2.3") < v("1.3.0"));
    assert!(v("1.2.3") < v("2.0.0"));
    assert!(v("1.2.3") == v("1.2.3"));
}

#[test]
fn lib_splits_name_version() {
    let lib = Lib::from_str("pyperf@2.7.0").unwrap();
    assert_eq!(lib.name, "pyperf");
    assert_eq!(lib.version, Version::from_str("2.7.0").unwrap());

    // Names may contain hyphens, only char @ separates the version
    let hyphenated = Lib::from_str("google-benchmark@1.8.3").unwrap();
    assert_eq!(hyphenated.name, "google-benchmark");
    assert_eq!(hyphenated.version, Version::from_str("1.8.3").unwrap());

    assert!(Lib::from_str("pyperf").is_none());
    assert!(Lib::from_str("pyperf@2.7").is_none());
}

// Parsers, end to end testing against real output

#[test]
fn pyperf_2_7_0() {
    let data = parse_fixture("pyperf@2.7.0", "pyperf.txt").unwrap();

    let fib = find_benchmark(&data, "fib");
    assert!(close(fib.map["mean"], 760_000.0));
    assert!(close(fib.map["stddev"], 23_000.0));

    let sort = find_benchmark(&data, "sort");
    assert!(close(sort.map["mean"], 8_040.0));
}

#[test]
fn pytest_7_3_0() {
    let data = parse_fixture("pytest@7.3.0", "pytest.txt").unwrap();

    let sort = find_benchmark(&data, "test_sort");
    assert!(close(sort.map["min"], 7_484.0));
    assert!(close(sort.map["mean"], 7_987.5));

    let fib = find_benchmark(&data, "test_fib");
    assert!(close(fib.map["mean"], 710_699.4));
    assert!(close(fib.map["max"], 1_160_789.0));
}

#[test]
fn go_testing_1_26_4() {
    let data = parse_fixture("go-testing@1.26.4", "go-testing.txt").unwrap();

    let fib = find_benchmark(&data, "BenchmarkFib");
    assert!(close(fib.map["mean"], 32_459.0));

    let sort = find_benchmark(&data, "BenchmarkSort");
    assert!(close(sort.map["mean"], 643.6));
}

#[test]
fn criterion_0_5_1() {
    let data = parse_fixture("criterion@0.5.1", "criterion.txt").unwrap();

    let fib = find_benchmark(&data, "fib");
    assert!(close(fib.map["lower"], 22_624.0));
    assert!(close(fib.map["mean"], 22_702.0));
    assert!(close(fib.map["upper"], 22_775.0));

    let sort = find_benchmark(&data, "sort");
    assert!(close(sort.map["mean"], 341.86));
}

#[test]
fn google_benchmark_1_8_3() {
    let data = parse_fixture("google-benchmark@1.8.3", "google-benchmark.txt").unwrap();

    let fib = find_benchmark(&data, "BM_Fib");
    assert!(close(fib.map["mean"], 12_194.0));
    assert!(close(fib.map["cpu"], 12_150.0));

    let sort = find_benchmark(&data, "BM_Sort");
    assert!(close(sort.map["mean"], 1_710.0));
}

#[test]
fn jmh_1_37_0() {
    let data = parse_fixture("jmh@1.37.0", "jmh.txt").unwrap();

    let fib = find_benchmark(&data, "MyBenchmark.fib");
    assert!(close(fib.map["mean"], 23_516.718));
    assert!(close(fib.map["error"], 1_096.241));

    let sort = find_benchmark(&data, "MyBenchmark.sort");
    assert!(close(sort.map["mean"], 725.168));
}

#[test]
fn benchmarkdotnet_0_14_0() {
    let data = parse_fixture("benchmarkdotnet@0.14.0", "benchmarkdotnet.txt").unwrap();

    let fib = find_benchmark(&data, "FibBench");
    assert!(close(fib.map["mean"], 24_783.0));
    assert!(close(fib.map["error"], 198.2));
    assert!(close(fib.map["stddev"], 264.6));

    let sort = find_benchmark(&data, "SortBench");
    assert!(close(sort.map["mean"], 4_253.0));
}

#[test]
fn bad_input_fails_loud() {
    // Bad library output must fail loudly
    assert!(parse_fixture("go-testing@1.26.4", "pyperf.txt").is_err());
}

// Version resolution in the registry

#[test]
fn newer_request_resolves_to_closest_lower_parser() {
    // No parser for 2.9.9 => 2.7.0 parser must be selected
    assert!(parse_fixture("pyperf@2.9.9", "pyperf.txt").is_ok());
}

#[test]
fn request_below_minimum_version_has_no_parser() {
    // 2.6.0 predates only registered parser (2.7.0)
    assert!(parse_fixture("pyperf@2.6.0", "pyperf.txt").is_err());
}

#[test]
fn unknown_library_is_rejected() {
    assert!(parse_fixture("nosuchlib@1.0.0", "pyperf.txt").is_err());
}

// Statistical checks

#[test]
fn confidence_passes_stable_history() {
    let mut history = noisy_baseline();
    history.push(point("mean", 101.0));

    let config = config_with(&["confidence"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn confidence_flags_spike() {
    let mut history = noisy_baseline();
    history.push(point("mean", 300.0));

    let config = config_with(&["confidence"]);
    assert_regression(calculate_stats(&history, &options(), &config));
}

#[test]
fn confidence_skips_short_history() {
    // Below the 10 sample minimum even a spike stays silent
    let mut history: Vec<PerfData> = (0..5).map(|_| point("mean", 100.0)).collect();
    history.push(point("mean", 300.0));

    let config = config_with(&["confidence"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn confidence_controls_sensitivity() {
    // Higher confidence level widens the interval flagged at 90%, tolerated at 99.9%
    let mut history = noisy_baseline();
    history.push(point("mean", 103.0));

    let config = config_with(&["confidence"]);

    let mut strict = options();
    strict.confidence_level = 0.90;
    assert_regression(calculate_stats(&history, &strict, &config));

    let mut lenient = options();
    lenient.confidence_level = 0.999;
    assert!(calculate_stats(&history, &lenient, &config).is_ok());
}

#[test]
fn zscore_passes_stable_history() {
    let mut history = noisy_baseline();
    history.push(point("mean", 101.0));

    let config = config_with(&["zscore"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn zscore_flags_spike() {
    let mut history = noisy_baseline();
    history.push(point("mean", 300.0));

    let config = config_with(&["zscore"]);
    assert_regression(calculate_stats(&history, &options(), &config));
}

#[test]
fn zscore_skips_short_history() {
    // Below 10 sample minimum even a spike stays silent
    let mut history: Vec<PerfData> = (0..5).map(|_| point("mean", 100.0)).collect();
    history.push(point("mean", 300.0));

    let config = config_with(&["zscore"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn zscore_controls_sensitivity() {
    // Lowering threshold tightens the check flagged at 1.5, tolerated at 4.0
    let mut history = noisy_baseline();
    history.push(point("mean", 103.0));

    let config = config_with(&["zscore"]);

    let mut strict = options();
    strict.zscore_threshold = 1.5;
    assert_regression(calculate_stats(&history, &strict, &config));

    let mut lenient = options();
    lenient.zscore_threshold = 4.0;
    assert!(calculate_stats(&history, &lenient, &config).is_ok());
}

#[test]
fn zscore_increase_from_flat_baseline() {
    let config = config_with(&["zscore"]);

    // Zero spread means any increase flags
    let mut history: Vec<PerfData> = (0..15).map(|_| point("mean", 100.0)).collect();
    history.push(point("mean", 100.5));
    assert_regression(calculate_stats(&history, &options(), &config));

    // Staying level is fine
    let mut same: Vec<PerfData> = (0..15).map(|_| point("mean", 100.0)).collect();
    same.push(point("mean", 100.0));
    assert!(calculate_stats(&same, &options(), &config).is_ok());
}

#[test]
fn ewma_passes_stable_history() {
    let mut history = noisy_baseline();
    history.push(point("mean", 101.0));

    let config = config_with(&["ewma"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn ewma_flags_spike() {
    // 300 on a ~100 baseline moves the smoothed average ~40%, while the threshold is 20%
    let mut history = noisy_baseline();
    history.push(point("mean", 300.0));

    let config = config_with(&["ewma"]);
    assert_regression(calculate_stats(&history, &options(), &config));
}

#[test]
fn ewma_skips_short_history() {
    // Below 5 sample minimum refuse to measure
    let mut history: Vec<PerfData> = (0..3).map(|_| point("mean", 100.0)).collect();
    history.push(point("mean", 300.0));

    let config = config_with(&["ewma"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn ewma_controls_sensitivity() {
    // Lower threshold tightens the check flagged at 5%, tolerated at 50%
    let mut history = noisy_baseline();
    history.push(point("mean", 200.0));

    let config = config_with(&["ewma"]);

    let mut strict = options();
    strict.ewma_threshold = 0.05;
    assert_regression(calculate_stats(&history, &strict, &config));

    let mut lenient = options();
    lenient.ewma_threshold = 0.5;
    assert!(calculate_stats(&history, &lenient, &config).is_ok());
}

// Statistical check selection and verdict combination

#[test]
fn policy_decides() {
    // 102.5: zscore passes, confidence flags, policy decides end result
    let mut history = noisy_baseline();
    history.push(point("mean", 102.5));

    let mut config = config_with(&["zscore", "confidence"]);
    config.policy = Policy::Any;
    assert_regression(calculate_stats(&history, &options(), &config));

    config.policy = Policy::All;
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn config_window_limits_history() {
    // Same data, different window the flat recent points flag small
    // increase while the noisy old points do not
    let mut history: Vec<PerfData> = (0..20)
        .map(|i| point("mean", if i % 2 == 0 { 150.0 } else { 50.0 }))
        .collect();
    history.extend((0..10).map(|_| point("mean", 100.0)));
    history.push(point("mean", 100.5));

    let mut config = config_with(&["zscore"]);
    config.zscore.window = 11;
    assert_regression(calculate_stats(&history, &options(), &config));

    config.zscore.window = 100;
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

#[test]
fn check_required_metric_missing() {
    // Every check requires "mean" key in history
    let mut history: Vec<PerfData> = (0..20).map(|_| point("min", 100.0)).collect();
    history.push(point("min", 300.0));

    let config = config_with(&["zscore", "ewma", "confidence"]);
    assert!(calculate_stats(&history, &options(), &config).is_ok());
}

// Configuration file parsing

#[test]
fn partial_config_defaults() {
    let config: Config = toml::from_str("[zscore]\nthreshold = 4.5").unwrap();

    assert_eq!(config.zscore.threshold, 4.5);
    assert_eq!(config.zscore.window, 100); // untouched default
    assert_eq!(config.confidence.level, 0.95);
    assert!(config.enabled.iter().any(|name| name == "ewma"));
}

#[test]
fn config_rejects_unknown() {
    // Typos must fail loudly
    assert!(toml::from_str::<Config>("thresold = 3.0").is_err());
    assert!(toml::from_str::<Config>("[zscore]\nthresold = 3.0").is_err());
}
