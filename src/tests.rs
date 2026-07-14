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
use crate::statistics::{combine, confidence_run, evaluate_confidence, evaluate_ewma, run_checks, CheckReport, CheckVerdict};
use crate::units;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use std::collections::HashMap;

const ALL_CHECKS: [&str; 2] = ["ewma", "confidence"];

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

// The pipeline verdict for one benchmark: enabled checks and policy
fn gate(history: &[PerfData], opts: &CliOptions, config: &Config) -> bool {
    let verdicts = run_checks(history, opts, config).unwrap();
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
        assert!(!gate(&history_with(101.0), &options(), &config_with(&[check])),
            "{} flagged a stable history", check);
    }
}

#[test]
fn every_check_flags_spike() {
    for check in ALL_CHECKS {
        assert!(gate(&history_with(300.0), &options(), &config_with(&[check])),
            "{} missed a 3x spike", check);
    }
}

#[test]
fn every_check_skips_short_history() {
    // Below the 10 sample minimum even a spike stays silent
    for check in ALL_CHECKS {
        let mut history = series(&[100.0; 5]);
        history.push(point("mean", 300.0));
        assert!(!gate(&history, &options(), &config_with(&[check])),
            "{} judged below the sample floor", check);
    }
}

#[test]
fn confidence_controls_sensitivity() {
    // Higher confidence level widens interval flagged at 90%, tolerated at 99.9%
    let config = config_with(&["confidence"]);

    let mut strict = options();
    strict.confidence_level = 0.90;
    assert!(gate(&history_with(103.0), &strict, &config));

    let mut lenient = options();
    lenient.confidence_level = 0.999;
    assert!(!gate(&history_with(103.0), &lenient, &config));
}

#[test]
fn ewma_controls_sensitivity() {
    // Lower threshold tightens check flagged at 5%, tolerated at 50%
    let config = config_with(&["ewma"]);

    let mut strict = options();
    strict.ewma_threshold = 0.05;
    assert!(gate(&history_with(200.0), &strict, &config));

    let mut lenient = options();
    lenient.ewma_threshold = 0.5;
    assert!(!gate(&history_with(200.0), &lenient, &config));
}

#[test]
fn confidence_increase_from_flat_baseline() {
    // With zero spread t-interval collapses to the mean
    let config = config_with(&["confidence"]);

    // Zero spread means any increase flags
    let mut history = series(&[100.0; 15]);
    history.push(point("mean", 100.5));
    assert!(gate(&history, &options(), &config));

    // Staying level is fine
    assert!(!gate(&series(&[100.0; 16]), &options(), &config));
}


// Statistical checks against a pseudo-oracle (Weyuker 1982)
// Expected values were generated by independent implementation (scipy/pandas)

// Ten point baseline with mean exactly 100 and standard deviation 2
fn reference_history(newest: f64) -> Vec<PerfData> {
    series(&[100.0, 102.0, 98.0, 101.0, 99.0, 103.0, 97.0, 100.0, 102.0, 98.0, newest])
}

#[test]
fn confidence_bound_matches_pseudo_oracle() {
    // upper bound = 100 + t(0.95, 9) * 2 * sqrt(1 + 1/10) = 103.8451701269
    // where t(0.95, 9) = 1.8331129327 (scipy.stats.t.ppf(0.95, 9))
    let report = judged(evaluate_confidence(&reference_history(110.0), &options()));
    assert!(close(report.threshold, 103.8451701269));
    assert!(report.flagged); // 110 is above bound

    let report = judged(evaluate_confidence(&reference_history(103.844), &options()));
    assert!(!report.flagged); // just under bound
}

#[test]
fn ewma_change_matches_pseudo_oracle() {
    // smoothed averages 99.8286755840 -> 101.8629404672, change 2.0377563%
    let report = judged(evaluate_ewma(&reference_history(110.0), &options()));
    assert!(close(report.value, 0.020377563));
    assert!(!report.flagged); // 2.04% stays under 10% threshold
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

        assert!(gate(&scaled(300.0), &options(), &config));
        assert!(!gate(&scaled(101.0), &options(), &config));
    }
}

// Statistical properties of checks on synthetic streams

// Feed points one at a time, while counting flags
fn flags_on_stream(values: &[f64], config: &Config, opts: &CliOptions) -> (usize, usize) {
    let history = series(values);

    let mut flagged = 0;
    let mut judged = 0;
    for i in 9..history.len() {
        judged += 1;
        if gate(&history[..=i], opts, config) {
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
fn confidence_false_alarm_rate_matches_level() {
    // Confidence false alarm rate is 5% at 0.95
    let stream = healthy_stream();

    let (flagged, judged) = flags_on_stream(&stream, &config_with(&["confidence"]), &options());
    let rate = flagged as f64 / judged as f64;
    println!("confidence 0.95 false alarm rate: {:.4}", rate);
    assert!(rate > 0.035 && rate < 0.065, "rate {} outside [3.5%, 6.5%]", rate);

    let mut lenient = options();
    lenient.confidence_level = 0.90;
    let (flagged, judged) = flags_on_stream(&stream, &config_with(&["confidence"]), &lenient);
    let rate = flagged as f64 / judged as f64;
    println!("confidence 0.90 false alarm rate: {:.4}", rate);
    assert!(rate > 0.08 && rate < 0.12, "rate {} outside [8%, 12%]", rate);
}

#[test]
fn ewma_never_false_alarms() {
    // Smoothing suppresses noise completely
    let (flagged, _) = flags_on_stream(&healthy_stream(), &config_with(&["ewma"]), &options());
    assert_eq!(flagged, 0);
}

// 50 points alternating +-3 around 100, sample std dev 3.03
fn step_baseline() -> Vec<f64> {
    (0..50).map(|i| 100.0 + if i % 2 == 0 { 3.0 } else { -3.0 }).collect()
}

#[test]
fn mean_shift_size_determines_which_checks_flag() {
    // Confidence is the most sensitive, ewma needs near 50%
    let flags = |step_pct: f64, enabled: &[&str]| {
        let mut values = step_baseline();
        values.push(100.0 * (1.0 + step_pct / 100.0));
        gate(&series(&values), &options(), &config_with(enabled))
    };

    // +6% (two standard deviations): confidence flags, ewma does not
    assert!(flags(6.0, &["confidence"]));
    assert!(!flags(6.0, &["ewma"]));

    // +10%: confidence still flags, ewma still silent
    assert!(flags(10.0, &["confidence"]));
    assert!(!flags(10.0, &["ewma"]));

    // +25%: not enough for ewma
    assert!(!flags(25.0, &["ewma"]));

    // +50%: large enough for both = trip policy all
    assert!(flags(50.0, &["confidence"]));
    assert!(flags(50.0, &["ewma"]));
    assert!({
        let mut values = step_baseline();
        values.push(150.0);
        gate(&series(&values), &options(), &config_with(&ALL_CHECKS))
    });
}

#[test]
fn gradual_drift_caught_by_confidence_not_ewma() {
    // Drift of +1% per commit
    // ewma - never sees a big enough step
    // confidence - keeps flagging
    let mut values = step_baseline();
    for k in 1..=60 {
        values.push(100.0 + k as f64 + if k % 2 == 0 { 3.0 } else { -3.0 });
    }

    let (confidence, _) = flags_on_stream(&values, &config_with(&["confidence"]), &options());
    let (ewma, _) = flags_on_stream(&values, &config_with(&["ewma"]), &options());
    println!("drift flags: confidence={} ewma={}", confidence, ewma);

    assert_eq!(ewma, 0);
    assert!(confidence > 40, "confidence flagged only {} times", confidence);
}

#[test]
fn improvement_never_flags() {
    // Running faster is not regression
    let mut config = config_with(&ALL_CHECKS);
    config.policy = Policy::Any;
    assert!(!gate(&history_with(30.0), &options(), &config));
}

#[test]
fn detection_after_mean_shift_differs_per_check() {
    // Thirty points at +10% per level
    // ewma - never flags, smoothed average climbs in too small steps
    // confidence - keeps flagging while window remembers healthy level
    let mut values = step_baseline();
    for i in 0..30 {
        values.push(110.0 + if i % 2 == 0 { 3.0 } else { -3.0 });
    }

    // The baseline never flags, so every count comes from after the shift
    let (confidence, _) = flags_on_stream(&values, &config_with(&["confidence"]), &options());
    let (ewma, _) = flags_on_stream(&values, &config_with(&["ewma"]), &options());
    println!("after +10% shift: confidence={} ewma={}", confidence, ewma);

    assert_eq!(ewma, 0);
    assert!(confidence > 10, "confidence must keep flagging after the shift");
}

// Statistical check selection and verdict combination

#[test]
fn policy_decides() {
    // 102.5: ewma passes, confidence flags, policy decides end result
    let mut config = config_with(&["ewma", "confidence"]);
    config.policy = Policy::Any;
    assert!(gate(&history_with(102.5), &options(), &config));

    config.policy = Policy::All;
    assert!(!gate(&history_with(102.5), &options(), &config));
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

    let mut config = config_with(&["confidence"]);
    config.confidence.window = 11;
    assert!(gate(&history, &options(), &config));

    config.confidence.window = 100;
    assert!(!gate(&history, &options(), &config));
}

// Run rule

// Baseline with n newest commits at value
fn streak(n: usize, value: f64) -> Vec<PerfData> {
    let mut values = step_baseline();
    values.extend(vec![value; n]);
    series(&values)
}

#[test]
fn run_rule_fires_after_n_consecutive_flags() {
    let history = streak(2, 120.0);
    let config = config_with(&["confidence"]);

    let reason = confidence_run(&history, &options(), &config).unwrap();
    assert_eq!(reason.unwrap(), "confidence check flagged the last 2 commits in a row");
}

#[test]
fn run_rule_ignores_isolated_flag() {
    // We need at least two consecutive flags
    let history = streak(1, 120.0);
    let config = config_with(&["confidence"]);

    let reason = confidence_run(&history, &options(), &config).unwrap();
    assert!(reason.is_none());
}

#[test]
fn run_rule_disarmed_by_config() {
    // run = 0 turns the rule off
    let history = streak(2, 120.0);

    let mut config = config_with(&["confidence"]);
    config.confidence.run = 0;
    let reason = confidence_run(&history, &options(), &config).unwrap();
    assert!(reason.is_none());

    let config = config_with(&[]);
    let reason = confidence_run(&history, &options(), &config).unwrap();
    assert!(reason.is_none());
}

#[test]
fn run_rule_streak_needs_warmed_up_prefixes() {
    // Ten points, older prefix has nine, below min_samples = broken streak
    let mut values = vec![100.0; 8];
    values.extend([120.0, 120.0]);
    let history = series(&values);
    let config = config_with(&["confidence"]);

    let reason = confidence_run(&history, &options(), &config).unwrap();
    assert!(reason.is_none());
}

// Configuration file parsing

#[test]
fn partial_config_defaults() {
    let config: Config = toml::from_str("[confidence]\nlevel = 0.99").unwrap();

    assert_eq!(config.confidence.level, 0.99);
    assert_eq!(config.confidence.window, 100); // untouched default
    assert_eq!(config.ewma.alpha, 0.2);
    assert!(config.enabled.iter().any(|name| name == "ewma"));
}

#[test]
fn config_rejects_unknown() {
    // Typos must fail loudly
    assert!(toml::from_str::<Config>("thresold = 3.0").is_err());
    assert!(toml::from_str::<Config>("[confidence]\nthresold = 3.0").is_err());
}

#[test]
fn config_rejects_empty_enabled() {
    let config = config_with(&[]);
    assert!(config.validate().is_err());
}
