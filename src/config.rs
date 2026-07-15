use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use crate::errors::SlugError;
use crate::terms;

// Slug config file
// Read slug.toml from the repository root
// Every field has default so config can be partial
// Unknown keys are rejected

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    // Which checks to run
    // Names must match those registered in statistics.rs
    pub enabled: Vec<String>,
    // How to combine the verdicts of the enabled checks
    pub policy: Policy,
    // No check judges until window has this many points
    pub min_samples: usize,
    pub zscore: Zscore,
    pub ewma: Ewma,
    #[serde(rename = "prediction-bound")]
    pub prediction_bound: PredictionBound,
}

// any = check1 OR check2 OR check3 OR ...
// all = check1 AND check2 AND check3 AND ...
#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Any,
    #[default]
    All,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: vec!["zscore".to_string(), "ewma".to_string(), "prediction-bound".to_string()],
            policy: Policy::All,
            min_samples: 10,
            zscore: Zscore::default(),
            ewma: Ewma::default(),
            prediction_bound: PredictionBound::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Zscore {
    pub threshold: f64,
    // How many recent points to evaluate against
    pub window: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ewma {
    pub alpha: f64,
    // control-limit width in standard deviations, lower = more sensitive
    pub limit: f64,
    pub window: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PredictionBound {
    // flag a new measurement above 95% (level = 0.95) of other measurements
    // higher level = fewer false alarms
    pub level: f64,
    pub window: usize,
    // fail when check flags this many commits in a row, regardless of policy
    // catches slow drifts that policy misses (0 = off)
    pub run: usize,
}

impl Default for Zscore {
    fn default() -> Self {
        Zscore { threshold: 3.0, window: 100 }
    }
}

impl Default for PredictionBound {
    fn default() -> Self {
        PredictionBound { level: 0.95, window: 100, run: 2 }
    }
}

impl Default for Ewma {
    fn default() -> Self {
        Ewma { alpha: 0.2, limit: 3.0, window: 100 }
    }
}

impl Config {
    // Largest window any check needs
    pub fn max_window(&self) -> usize {
        self.zscore.window.max(self.ewma.window).max(self.prediction_bound.window)
    }

    pub fn check_is_on(&self, name: &str) -> bool {
        self.enabled.iter().any(|enabled| enabled == name)
    }

    // Reject bad configs
    pub fn validate(&self) -> Result<(), SlugError> {
        let known = crate::statistics::check_names();

        if self.enabled.is_empty() {
            return Err(SlugError::config(format!(
                "No checks enabled, every run would pass\nhelp: available checks: {}", known.join(", "))));
        }

        for name in &self.enabled {
            if !known.contains(&name.as_str()) {
                return Err(SlugError::config(format!(
                    "Unknown check '{}' in enabled\nhelp: known checks: {}", name, known.join(", "))));
            }
        }

        // sample() needs at least two poinst plus fresh one
        if self.min_samples < 3 {
            return Err(SlugError::config("min_samples must be at least 3"));
        }

        for (check, window) in [("zscore", self.zscore.window), ("ewma", self.ewma.window), ("prediction-bound", self.prediction_bound.window)] {
            if window < self.min_samples {
                return Err(SlugError::config(format!(
                    "{} window ({}) is below min_samples ({}), the check would never judge", check, window, self.min_samples)));
            }
        }

        // Zero or negative limit would flag always
        if !(self.ewma.limit > 0.0) {
            return Err(SlugError::config("EWMA limit must be positive"));
        }

        // alpha = 0 freezes average, above 1 is unstable
        if !(self.ewma.alpha > 0.0 && self.ewma.alpha <= 1.0) {
            return Err(SlugError::config("ewma alpha must be in (0, 1]"));
        }

        // Negative flags improvements, NaN never flags
        if !(self.zscore.threshold > 0.0) {
            return Err(SlugError::config("zscore threshold must be positive"));
        }

        // Number(0, 1) has no percentage equivalent
        if !(self.prediction_bound.level > 0.0 && self.prediction_bound.level < 1.0) {
            return Err(SlugError::config("prediction-bound level must be between 0 and 1"));
        }

        if self.prediction_bound.run == 1 {
            return Err(SlugError::config("prediction-bound run must be 0 (off) or at least 2"));
        }

        Ok(())
    }
}

const CONFIG_FILE: &str = "slug.toml";

// If we are not in repo, fall back to cwd
fn config_path() -> PathBuf {
    match git2::Repository::discover(".") {
        Ok(repo) => match repo.workdir() {
            Some(root) => root.join(CONFIG_FILE),
            None => PathBuf::from(CONFIG_FILE),
        },
        Err(_) => PathBuf::from(CONFIG_FILE),
    }
}

pub fn load_or_default() -> Result<Config, SlugError> {
    let path = config_path();
    // Errors name the config file, the user did not mention it on the command line
    let config = match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|e| SlugError::config(format!("{}: {}", path.display(), e)))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(SlugError::config(format!("Cannot read {}: {}", path.display(), e))),
    };
    config.validate()?;
    Ok(config)
}


const EXAMPLE_CONFIG: &str = "\
# Slug configuration

# Which checks to run, and how to combine their verdicts.
enabled = [\"zscore\", \"ewma\", \"prediction-bound\"]
# any = flag if ANY of enabled checks flag
# all = flag only if ALL checks flag
policy = \"all\"

# no check judges until its window has this many points
min_samples = 10

[zscore]
# flag when value is this many standard deviations above the mean
threshold = 3.0
# how many recent points to evaluate against
window = 100

[ewma]
# smoothing factor, higher = more emphasis on recent values
alpha = 0.2
# control-limit width in standard deviations (L); lower = more sensitive
limit = 3.0
window = 100

[prediction-bound]
# flag a new measurement above 95% (level = 0.95) of other measurements
# higher level = fewer false alarms
level = 0.95
window = 100
# fail when check flags this many commits in a row, regardless of policy
# catches slow drifts that policy misses (0 = off)
run = 2
";

// Write the example config to the repository root
pub fn write_example() -> Result<(), SlugError> {
    let path = config_path();
    if path.exists() {
        return Err(SlugError::config(format!("{} already exists", path.display())));
    }
    fs::write(&path, EXAMPLE_CONFIG)
        .map_err(|e| SlugError::config(format!("Cannot write {}: {}", path.display(), e)))?;
    terms::line(&format!("Created {}", path.display()));
    Ok(())
}
