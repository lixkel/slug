use crate::cli;
use crate::units;
use crate::errors::SlugError;
use crate::lib_parsers::*;

use std::io::{Read};
use std::collections::HashMap;
use std::cmp::Ordering;

macro_rules! add_libs {
    ($parsers:ident, { $($lib_name:expr => [$(($fn_ptr:ident, $ver:expr)),+ $(,)?]),+ $(,)? }) => {
        $(
            $parsers.insert(
                $lib_name.to_string(),
                vec![
                    $(
                        Parser {
                            version: Version::from_str($ver).expect("Bad library version format!"),
                            parser: $fn_ptr,
                        },
                    )+
                ]
            );
        )+
    };
}

#[derive(Debug)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug)]
pub struct Lib {
    pub name: String,
    pub version: Version,
}

// Type signature for a function that parses a library's raw output into PerfData
type ParserFn = fn(&str) -> Result<Vec<PerfData>, SlugError>;

struct Parser {
    pub version: Version,
    pub parser: ParserFn,
}

// Struct for exporting parsed data
#[derive(Debug)]
pub struct PerfData {
    pub name: String,
    pub commit_hash: String,
    pub map: HashMap<String, f64>, // I think this could be rewritten to something like "&'a str"
}

impl PerfData {
    // Commit hash should be created just once at the beggining of a parser
    pub fn new(name: &str, commit_hash: &str) -> Self {
        Self {
            name: name.to_string(),
            commit_hash: commit_hash.to_string(),
            map: HashMap::new(),
        }
    }

    // Only allowed way to add metric is to normalize it
    pub fn record(&mut self, key: &str, value: f64, unit: &str) -> Result<(), SlugError> {
        self.map.insert(key.to_string(), units::normalize(value, unit)?);
        Ok(())
    }
}

impl Version {
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();

        if parts.len() != 3 {
            return None;
        }

        let major = parts[0].parse::<u32>();
        let minor = parts[1].parse::<u32>();
        let patch = parts[2].parse::<u32>();

        match (major, minor, patch) {
            (Ok(major), Ok(minor), Ok(patch)) => Some(Self { major, minor, patch }),
            _ => None,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.major != other.major {
            self.major.cmp(&other.major)
        } else if self.minor != other.minor {
            self.minor.cmp(&other.minor)
        } else {
            self.patch.cmp(&other.patch)
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch == other.patch
    }
}

impl Eq for Version {}

impl Lib {
    pub fn from_str(s: &str) -> Option<Self> {
        // Identifier is "name@version"
        let (name, ver) = s.split_once('@')?;
        let version = Version::from_str(ver)?;

        Some(Self { name: name.to_string(), version })
    }
}

// TODO: add list parsers
pub fn parse(reader: cli::PerfDataReader, lib: &Lib) -> Result<Vec<PerfData>, SlugError> {
    let mut parsers: HashMap<String, Vec<Parser>> = HashMap::new();

    add_libs!(parsers, {
        "pytest" => [(pytest_7_3_0, "7.3.0")],
        "pyperf" => [(pyperf_2_7_0, "2.7.0")],
        "go-testing" => [(go_testing_1_26_4, "1.26.4")],
        "criterion" => [(criterion_0_5_1, "0.5.1")],
        "google-benchmark" => [(google_benchmark_1_8_3, "1.8.3")],
        "jmh" => [(jmh_1_37_0, "1.37.0")],
        "benchmarkdotnet" => [(benchmarkdotnet_0_14_0, "0.14.0")]
    });

    let versions = parsers.get(&lib.name)
        .ok_or_else(|| SlugError::Parsing(format!("No parser registered for library '{}'", lib.name)))?;

    // Select highest parser version that is <= than the requested 
    // Each parsers version is the minimum library version it supports
    // So the closest lower or equal entry is the correct one
    // If the requested version predates every registered parser no compatible parser exists
    let closest = versions.iter()
        .filter(|p| p.version <= lib.version)
        .max_by(|a, b| a.version.cmp(&b.version))
        .ok_or_else(|| SlugError::Parsing(format!(
            "No parser for '{}' compatible with version {}.{}.{}",
            lib.name, lib.version.major, lib.version.minor, lib.version.patch
        )))?;

    let mut s = String::new();

    match reader { // TODO: move this to cli
        cli::PerfDataReader::Stdin(mut r) => r.read_to_string(&mut s)?,
        cli::PerfDataReader::File(mut r) => r.read_to_string(&mut s)?,
    };

    let data = (closest.parser)(&s)?;
    Ok(data)
}