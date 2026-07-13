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
    pub confidence: Confidence,
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
            enabled: vec!["ewma".to_string(), "zscore".to_string(), "confidence".to_string()],
            policy: Policy::All,
            min_samples: 10,
            zscore: Zscore::default(),
            ewma: Ewma::default(),
            confidence: Confidence::default(),
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
    // flag when the smoothed change exceeds this value (0.1 = 10%)
    pub threshold: f64,
    pub window: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Confidence {
    // flag a new measurement above 95% (level = 0.95) of other measurements
    // higher level = fewer false alarms
    pub level: f64,
    pub window: usize,
}

impl Default for Zscore {
    fn default() -> Self {
        Zscore { threshold: 3.0, window: 100 }
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence { level: 0.95, window: 100 }
    }
}

impl Default for Ewma {
    fn default() -> Self {
        Ewma { alpha: 0.2, threshold: 0.1, window: 100 }
    }
}

impl Config {
    // Largest window any check needs
    pub fn max_window(&self) -> usize {
        self.zscore.window.max(self.ewma.window).max(self.confidence.window)
    }

    // Reject bad configs
    pub fn validate(&self) -> Result<(), SlugError> {
        let known = crate::statistics::check_names();
        for name in &self.enabled {
            if !known.contains(&name.as_str()) {
                return Err(SlugError::config(format!(
                    "Unknown check '{}' in enabled, known checks: {}", name, known.join(", "))));
            }
        }

        // sample() needs at least two poinst plus fresh one
        if self.min_samples < 3 {
            return Err(SlugError::config("min_samples must be at least 3"));
        }

        for (check, window) in [("zscore", self.zscore.window), ("ewma", self.ewma.window), ("confidence", self.confidence.window)] {
            if window < self.min_samples {
                return Err(SlugError::config(format!(
                    "{} window ({}) is below min_samples ({}), the check would never judge", check, window, self.min_samples)));
            }
        }

        if !(self.ewma.threshold > 0.0) {
            return Err(SlugError::config("EWMA threshold must be positive"));
        }

        // Negative flags improvements, NaN never flags
        if !(self.zscore.threshold > 0.0) {
            return Err(SlugError::config("zscore threshold must be positive"));
        }

        // alpha = 0 freezes average, above 1 is unstable
        if !(self.ewma.alpha > 0.0 && self.ewma.alpha <= 1.0) {
            return Err(SlugError::config("ewma alpha must be in (0, 1]"));
        }

        // Number(0, 1) has no percentage equivalent
        if !(self.confidence.level > 0.0 && self.confidence.level < 1.0) {
            return Err(SlugError::config("confidence level must be between 0 and 1"));
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
    let config = match fs::read_to_string(config_path()) {
        Ok(text) => toml::from_str(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(SlugError::Io(e)),
    };
    config.validate()?;
    Ok(config)
}


const EXAMPLE_CONFIG: &str = "\
# Slug configuration

# Which checks to run, and how to combine their verdicts.
enabled = [\"ewma\", \"zscore\", \"confidence\"]
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
# flag when the smoothed change exceeds this value (0.1 = 10%)
threshold = 0.1
window = 100

[confidence]
# flag a new measurement above 95% (level = 0.95) of other measurements
# higher level = fewer false alarms
level = 0.95
window = 100
";

// Write the example config to the repository root
pub fn write_example() -> Result<(), SlugError> {
    let path = config_path();
    if path.exists() {
        return Err(SlugError::config(format!("{} already exists", path.display())));
    }
    fs::write(&path, EXAMPLE_CONFIG)?;
    terms::line(&format!("Created {}", path.display()));
    Ok(())
}
