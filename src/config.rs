use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use crate::errors::SlugError;

// Slug config file
// Read slug.toml from the repository root
// Every field has default so config can be partial
// Unknown keys are rejected

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub zscore: Zscore,
    pub ewma: Ewma,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Zscore {
    pub threshold: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ewma {
    pub alpha: f64,
}

impl Default for Zscore {
    fn default() -> Self {
        Zscore { threshold: 3.0 }
    }
}

impl Default for Ewma {
    fn default() -> Self {
        Ewma { alpha: 0.2 }
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
