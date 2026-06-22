use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use crate::errors::SlugError;

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
    pub zscore: Zscore,
    pub ewma: Ewma,
}

// any = check1 OR check2 OR check3 OR ...
// all = check1 AND check2 AND check3 AND ...
#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    #[default]
    Any,
    All,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: vec!["ewma".to_string(), "zscore".to_string()],
            policy: Policy::Any,
            zscore: Zscore::default(),
            ewma: Ewma::default(),
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
    // flag when the smoothed change exceeds this value (0.2 = 20%)
    pub threshold: f64,
    pub window: usize,
}

impl Default for Zscore {
    fn default() -> Self {
        Zscore { threshold: 3.0, window: 100 }
    }
}

impl Default for Ewma {
    fn default() -> Self {
        Ewma { alpha: 0.2, threshold: 0.2, window: 100 }
    }
}

impl Config {
    // Largest window any check needs
    pub fn max_window(&self) -> usize {
        self.zscore.window.max(self.ewma.window)
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
    match fs::read_to_string(config_path()) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(SlugError::Io(e)),
    }
}


const EXAMPLE_CONFIG: &str = "\
# Slug configuration

# Which checks to run, and how to combine their verdicts.
enabled = [\"ewma\", \"zscore\"]
# any = flag if ANY of enabled checks flag
# all = flag only if ALL checks flag
policy = \"any\"

[zscore]
# flag when value is this many standard deviations above the mean
threshold = 3.0
# how many recent points to evaluate against
window = 100

[ewma]
# smoothing factor, higher = more emphasis on recent values
alpha = 0.2
# flag when the smoothed change exceeds this value (0.2 = 20%)
threshold = 0.2
window = 100
";

// Write the example config to the repository root
pub fn write_example() -> Result<(), SlugError> {
    let path = config_path();
    if path.exists() {
        return Err(SlugError::Config(format!("{} already exists", path.display())));
    }
    fs::write(&path, EXAMPLE_CONFIG)?;
    println!("Created {}", path.display());
    Ok(())
}
