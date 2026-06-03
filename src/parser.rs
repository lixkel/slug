use crate::cli;
use crate::git;
use crate::units;
use crate::errors::SlugError;

use std::io::{Read};
use std::collections::HashMap;
use std::cmp::Ordering;
use regex::Regex;

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
        let parts: Vec<&str> = s.split('-').collect();

        if parts.len() != 2 {
            return None;
        }

        let version = Version::from_str(parts[1]);

        match version {
            Some(version) => Some(Self { name: parts[0].to_string(), version }),
            None => None,
        }
    }
}

// TODO: add list parsers
pub fn parse(reader: cli::PerfDataReader, lib: &Lib) -> Result<Vec<PerfData>, SlugError> {
    let mut parsers: HashMap<String, Vec<Parser>> = HashMap::new();

    add_libs!(parsers, {
        "pytest" => [(pytest_7_3_0, "7.3.0")],
        "pyperf" => [(pyperf_2_7_0, "2.7.0")],
        "go_testing" => [(go_testing_1_26_4, "1.26.4")]
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

fn pytest_7_3_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
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

fn go_testing_1_26_4(s: &str) -> Result<Vec<PerfData>, SlugError> {
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

fn pyperf_2_7_0(s: &str) -> Result<Vec<PerfData>, SlugError> {
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