// Unit tests covering:
//  - Unit normalization (units.rs)
//  - Library name and version parsing (parser.rs)
//  - Parsers, end to end testing against real output
//  - Statistical checks (statistics.rs)
//  - slug.toml parsing (config.rs)

use crate::cli;
use crate::config::{Config, Policy};
use crate::errors::SlugError;
use crate::parser::{self, Lib, PerfData, Version};
use crate::statistics::{combine, evaluate_prediction_bound, evaluate_zscore, run_checks, CheckReport, CheckVerdict};
use crate::units;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use std::collections::HashMap;

const ALL_CHECKS: [&str; 2] = ["zscore", "prediction-bound"];

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
    let lib = Lib::from_str(lib).expect("library string needs to be valid");
    let parser = parser::resolve(&lib)?;
    let reader = cli::get_reader(&path)?;
    parser::parse(reader, parser)
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

// One record per value, the shape benchmark history arrives in
fn series(values: &[f64]) -> Vec<PerfData> {
    values.iter().map(|v| point("mean", *v)).collect()
}

// Twenty points alternating +-1 ns around 100 ns
fn noisy_baseline() -> Vec<PerfData> {
    (0..20)
        .map(|i| point("mean", 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 }))
        .collect()
}

// Noisy baseline with one newest measurement appended
fn history_with(newest: f64) -> Vec<PerfData> {
    let mut history = noisy_baseline();
    history.push(point("mean", newest));
    history
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

// The pipeline verdict for one benchmark: enabled checks and policy
fn gate(history: &[PerfData], config: &Config) -> bool {
    let verdicts = run_checks(history, config).unwrap();
    combine(&verdicts, config.policy)
}

// Unwraps check's return value, fails if check declined to judge
fn judged(verdict: Result<CheckVerdict, SlugError>) -> CheckReport {
    match verdict {
        Ok(CheckVerdict::Judged(report)) => report,
        _ => panic!("expected a judged verdict"),
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
    assert_eq!(lib.version, Version::from_str("2.7.0"));

    // Names may contain hyphens, only char @ separates the version
    let hyphenated = Lib::from_str("google-benchmark@1.8.3").unwrap();
    assert_eq!(hyphenated.name, "google-benchmark");
    assert_eq!(hyphenated.version, Version::from_str("1.8.3"));

    // No version means newest parser
    let bare = Lib::from_str("pyperf").unwrap();
    assert_eq!(bare.name, "pyperf");
    assert_eq!(bare.version, None);

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
    assert!(close(fib.map["mean"], 9_565.0));
    assert!(close(fib.map["cpu"], 9_562.0));

    let sort = find_benchmark(&data, "BM_Sort");
    assert!(close(sort.map["mean"], 1_642.0));

    // Template function
    let sort_t = find_benchmark(&data, "BM_SortT<std::vector<int>, 500>");
    assert!(close(sort_t.map["mean"], 1_713.0));
    assert!(close(sort_t.map["cpu"], 1_713.0));
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
fn bare_name_resolves_to_newest_parser() {
    assert!(parse_fixture("pyperf", "pyperf.txt").is_ok());
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
fn every_check_passes_stable_history() {
    for check in ALL_CHECKS {
        assert!(!gate(&history_with(101.0), &config_with(&[check])),
            "{} flagged a stable history", check);
    }
}

#[test]
fn every_check_flags_spike() {
    for check in ALL_CHECKS {
        assert!(gate(&history_with(300.0), &config_with(&[check])),
            "{} missed a 3x spike", check);
    }
}

#[test]
fn every_check_skips_short_history() {
    // Below the 10 sample minimum even a spike stays silent
    for check in ALL_CHECKS {
        let mut history = series(&[100.0; 5]);
        history.push(point("mean", 300.0));
        assert!(!gate(&history, &config_with(&[check])),
            "{} judged below the sample floor", check);
    }
}

#[test]
fn prediction_bound_controls_sensitivity() {
    // Higher prediction-bound level widens interval flagged at 90%, tolerated at 99.9%
    let mut config = config_with(&["prediction-bound"]);

    config.prediction_bound.level = 0.90;
    assert!(gate(&history_with(103.0), &config));

    config.prediction_bound.level = 0.999;
    assert!(!gate(&history_with(103.0), &config));
}

#[test]
fn zscore_controls_sensitivity() {
    // Lowering threshold tightens check flagged at 1.5, tolerated at 4.0
    let mut config = config_with(&["zscore"]);

    config.zscore.threshold = 1.5;
    assert!(gate(&history_with(103.0), &config));

    config.zscore.threshold = 4.0;
    assert!(!gate(&history_with(103.0), &config));
}

#[test]
fn prediction_bound_increase_from_flat_baseline() {
    // With zero spread t-interval collapses to the mean
    let config = config_with(&["prediction-bound"]);

    // Zero spread means any increase flags
    let mut history = series(&[100.0; 15]);
    history.push(point("mean", 100.5));
    assert!(gate(&history, &config));

    // Staying level is fine
    assert!(!gate(&series(&[100.0; 16]), &config));
}


// Statistical checks against a pseudo-oracle (Weyuker 1982)
// Expected values were generated by independent implementation (scipy/pandas)

// Ten point baseline with mean exactly 100 and standard deviation 2
fn reference_history(newest: f64) -> Vec<PerfData> {
    series(&[100.0, 102.0, 98.0, 101.0, 99.0, 103.0, 97.0, 100.0, 102.0, 98.0, newest])
}

#[test]
fn prediction_bound_matches_pseudo_oracle() {
    // upper bound = 100 + t(0.999, 9) * 2 * sqrt(1 + 1/10) = 109.0130555959
    // where t(0.999, 9) = 4.2968056627 (scipy.stats.t.ppf(0.999, 9))
    let report = judged(evaluate_prediction_bound(&reference_history(110.0), &Config::default()));
    assert!(close(report.threshold, 109.0130555959));
    assert!(report.flagged); // 110 is above bound

    let report = judged(evaluate_prediction_bound(&reference_history(109.012), &Config::default()));
    assert!(!report.flagged); // just under bound
}

#[test]
fn zscore_matches_pseudo_oracle() {
    // (110 - 100) / 2 = 5.0
    let report = judged(evaluate_zscore(&reference_history(110.0), &Config::default()));
    assert!(close(report.value, 5.0));
    assert!(report.flagged); // 5.0 is above default threshold 3.0
}

#[test]
fn verdicts_are_scale_invariant() {
    // Rescaling history must not change verdicts
    let config = config_with(&ALL_CHECKS);

    for scale in [1.0, 1e6] {
        let scaled = |newest: f64| -> Vec<PerfData> {
            let mut history: Vec<PerfData> = noisy_baseline().iter()
                .map(|entry| point("mean", entry.map["mean"] * scale))
                .collect();
            history.push(point("mean", newest * scale));
            history
        };

        assert!(gate(&scaled(300.0), &config));
        assert!(!gate(&scaled(101.0), &config));
    }
}

// Statistical properties of checks on synthetic streams

// Feed points one at a time, while counting flags
fn flags_on_stream(values: &[f64], config: &Config) -> (usize, usize) {
    let history = series(values);

    let mut flagged = 0;
    let mut judged = 0;
    for i in 9..history.len() {
        judged += 1;
        if gate(&history[..=i], config) {
            flagged += 1;
        }
    }
    (flagged, judged)
}

// 3000 healthy points around 100 ns with sigma 3, ChaCha8 is seed stable
fn healthy_stream() -> Vec<f64> {
    let mut rng = ChaCha8Rng::seed_from_u64(0x5eed_cafe);
    let normal = Normal::new(100.0, 3.0).unwrap();
    (0..3000).map(|_| normal.sample(&mut rng)).collect()
}

#[test]
fn prediction_bound_false_alarm_rate_matches_level() {
    let stream = healthy_stream();

    // Default level 0.999: about one healthy point in 1000 (0.1%)
    let (flagged, judged) = flags_on_stream(&stream, &config_with(&["prediction-bound"]));
    let rate = flagged as f64 / judged as f64;
    println!("prediction_bound 0.999 false alarm rate: {:.4}", rate);
    assert!(rate < 0.004, "rate {} not under 0.4%", rate);

    // 5% at 0.95
    let mut strict = config_with(&["prediction-bound"]);
    strict.prediction_bound.level = 0.95;
    let (flagged, judged) = flags_on_stream(&stream, &strict);
    let rate = flagged as f64 / judged as f64;
    println!("prediction_bound 0.95 false alarm rate: {:.4}", rate);
    assert!(rate > 0.035 && rate < 0.065, "rate {} outside [3.5%, 6.5%]", rate);

    // 10% at 0.90
    let mut lenient = config_with(&["prediction-bound"]);
    lenient.prediction_bound.level = 0.90;
    let (flagged, judged) = flags_on_stream(&stream, &lenient);
    let rate = flagged as f64 / judged as f64;
    println!("prediction_bound 0.90 false alarm rate: {:.4}", rate);
    assert!(rate > 0.08 && rate < 0.12, "rate {} outside [8%, 12%]", rate);
}

#[test]
fn zscore_false_alarms_are_rare() {
    // Three standard deviations should be crossed by under 1% of healthy points
    let (flagged, judged) = flags_on_stream(&healthy_stream(), &config_with(&["zscore"]));
    let rate = flagged as f64 / judged as f64;
    println!("zscore 3.0 false alarm rate: {:.4}", rate);
    assert!(rate < 0.01, "rate {} not under 1%", rate);
}

// 50 points alternating +-3 around 100, sample std dev 3.03
fn step_baseline() -> Vec<f64> {
    (0..50).map(|i| 100.0 + if i % 2 == 0 { 3.0 } else { -3.0 }).collect()
}

#[test]
fn mean_shift_size_determines_whether_checks_flag() {
    // Both point checks sit near three standard deviations, about +9% on this baseline
    let flags = |step_pct: f64, enabled: &[&str]| {
        let mut values = step_baseline();
        values.push(100.0 * (1.0 + step_pct / 100.0));
        gate(&series(&values), &config_with(enabled))
    };

    // +6% (two standard deviations): neither check flags
    assert!(!flags(6.0, &["prediction-bound"]));
    assert!(!flags(6.0, &["zscore"]));

    // +12%: both flag
    assert!(flags(12.0, &["prediction-bound"]));
    assert!(flags(12.0, &["zscore"]));

    // +50%: large enough for both = trip policy all
    assert!(flags(50.0, &["prediction-bound"]));
    assert!(flags(50.0, &["zscore"]));
    assert!({
        let mut values = step_baseline();
        values.push(150.0);
        gate(&series(&values), &config_with(&ALL_CHECKS))
    });
}

#[test]
fn improvement_never_flags() {
    // Running faster is not regression
    let mut config = config_with(&ALL_CHECKS);
    config.policy = Policy::Any;
    assert!(!gate(&history_with(30.0), &config));
}

// Statistical check selection and verdict combination

#[test]
fn policy_decides() {
    // 103.2: prediction-bound passes, zscore flags, policy decides end result
    let mut config = config_with(&["zscore", "prediction-bound"]);
    config.policy = Policy::Any;
    assert!(gate(&history_with(103.2), &config));

    config.policy = Policy::All;
    assert!(!gate(&history_with(103.2), &config));
}

#[test]
fn config_window_limits_history() {
    // Same data, different window the flat recent points flag small
    // increase while the noisy old points do not
    let mut history: Vec<PerfData> = (0..20)
        .map(|i| point("mean", if i % 2 == 0 { 150.0 } else { 50.0 }))
        .collect();
    history.extend(series(&[100.0; 10]));
    history.push(point("mean", 100.5));

    let mut config = config_with(&["prediction-bound"]);
    config.prediction_bound.window = 11;
    assert!(gate(&history, &config));

    config.prediction_bound.window = 100;
    assert!(!gate(&history, &config));
}

// Configuration file parsing

#[test]
fn partial_config_defaults() {
    let config: Config = toml::from_str("[prediction-bound]\nlevel = 0.99").unwrap();

    assert_eq!(config.prediction_bound.level, 0.99);
    assert_eq!(config.prediction_bound.window, 100); // untouched default
    assert_eq!(config.zscore.threshold, 3.0);
    assert!(config.enabled.iter().any(|name| name == "prediction-bound"));
}

#[test]
fn config_rejects_unknown() {
    // Typos must fail loudly
    assert!(toml::from_str::<Config>("thresold = 3.0").is_err());
    assert!(toml::from_str::<Config>("[prediction-bound]\nthresold = 3.0").is_err());
}

#[test]
fn config_rejects_empty_enabled() {
    let config = config_with(&[]);
    assert!(config.validate().is_err());
}
