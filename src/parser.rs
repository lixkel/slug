use crate::cli;

use std::io::{self, Read, BufReader};
use std::collections::HashMap;
use std::cmp::Ordering;
use std::error::Error;
use regex::Regex;

macro_rules! add_libs {
    ($parsers:ident, $(($fn_ptr:ident, $ver:expr)),+ $(,)?) => {
        $(
            $parsers.insert(
                "pytest".to_string(),
                vec![
                    Parser {
                        version: Version::from_str($ver).expect("Bad library version format!"),
                        parser: $fn_ptr,
                    },
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

struct Parser {
    pub version: Version,
    pub parser: fn(&str) -> Result<PerfData, Box<dyn Error>>,
}

#[derive(Debug)]
pub struct PerfData {
    pub min: f32,
    pub max: f32,
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

pub fn parse(reader: cli::PerfDataReader, lib: &Lib) -> Result<PerfData, Box<dyn Error>> {
    let mut parsers: HashMap<String, Vec<Parser>> = HashMap::new();

    add_libs!(parsers, (pytest_7_3_0, "7.3.0"));

    let versions = if parsers.contains_key(&lib.name) {
        &parsers[&lib.name]
    } else {
        return Err("Library not found")?;
    };

    let mut closest: &Parser = &versions[0];

    for v in versions {
        if closest.version < v.version && v.version < lib.version {
            closest = v;
        }
    }

    let mut s = String::new();

    let result = match reader {
        cli::PerfDataReader::Stdin(mut r) => r.read_to_string(&mut s)?,
        cli::PerfDataReader::File(mut r) => r.read_to_string(&mut s)?,
    };

    (closest.parser)(&s)
}

fn pytest_7_3_0(s: &str) -> Result<PerfData, Box<dyn Error>> {
    let regex = Regex::new(
        r"\S+\s+(?P<min>\d+\.\d+)\s+(?P<max>\d+\.\d+)\s+(?P<mean>\d+\.\d+)\s+(?P<stddev>\d+\.\d+)\s+(?P<median>\d+\.\d+)"
    )?;

    let found = regex.captures(s).ok_or("No match found")?;

    println!("Min: {}", &found["min"]);
    println!("Max: {}", &found["max"]);
    println!("Mean: {}", &found["mean"]);
    println!("StdDev: {}", &found["stddev"]);
    println!("Median: {}", &found["median"]);

    Ok(PerfData {
        min: found["min"].parse()?,
        max : found["max"].parse()?,
    })
}