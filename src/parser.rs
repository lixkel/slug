use crate::cli;
use crate::git;

use std::io::{Read};
use std::collections::HashMap;
use std::cmp::Ordering;
use std::error::Error;
use regex::Regex;

// TODO: this macro is nonfunctional
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
    pub parser: fn(&str) -> Result<Vec<PerfData>, Box<dyn Error>>,
}

#[derive(Debug)]
pub struct PerfData {
    pub name: String,
    pub commit_hash: String,
    pub map: HashMap<String, f64>, // I think this could be rewritten to something like "&'a str"
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
pub fn parse(reader: cli::PerfDataReader, lib: &Lib) -> Result<Vec<PerfData>, Box<dyn Error>> {
    let mut parsers: HashMap<String, Vec<Parser>> = HashMap::new();

    parsers.insert(
        "pytest".to_string(),
        vec![
            Parser {
                version: Version::from_str("7.3.0").expect("Bad library version format!"),
                parser: pytest_7_3_0,
            }
        ]
    );

    parsers.insert(
        "pyperf".to_string(),
        vec![
            Parser {
                version: Version::from_str("2.7.0").expect("Bad library version format!"),
                parser: pyperf_2_7_0,
            }
        ]
    );

    let versions = parsers.get(&lib.name).ok_or("Library not found")?;

    let mut closest: &Parser = &versions[0];

    for v in versions {
        if closest.version > v.version && v.version <= lib.version {
            closest = v;
        }
    }

    let mut s = String::new();

    match reader { // TODO: move this to cli
        cli::PerfDataReader::Stdin(mut r) => r.read_to_string(&mut s)?,
        cli::PerfDataReader::File(mut r) => r.read_to_string(&mut s)?,
    };

    let mut data = (closest.parser)(&s)?;
    Ok(data)
}

fn pytest_7_3_0(s: &str) -> Result<Vec<PerfData>, Box<dyn Error>> {
    let regex = Regex::new(
        r"\S+\s+(?P<name>\w+)\s+(?P<min>\d+\.\d+)\s+(?P<max>\d+\.\d+)\s+(?P<mean>\d+\.\d+)\s+(?P<stddev>\d+\.\d+)\s+(?P<median>\d+\.\d+)"
    )?;

    let mut results = Vec::new();
    
    for caps in regex.captures_iter(s) {
        let mut data = PerfData {
            name: caps["name"].to_string(),
            commit_hash: git::get_commit_hash()?,
            map: HashMap::new(),
        };

        data.map.insert("min".to_string(), caps["min"].parse()?);
        data.map.insert("max".to_string(), caps["max"].parse()?);
        data.map.insert("mean".to_string(), caps["mean"].parse()?);
        data.map.insert("stddev".to_string(), caps["stddev"].parse()?);
        data.map.insert("median".to_string(), caps["median"].parse()?);
        
        results.push(data);
    }
    
    if results.is_empty() {
        return Err(From::from("No matches found"));
    }
    
    Ok(results)
}

fn pyperf_2_7_0(s: &str) -> Result<Vec<PerfData>, Box<dyn Error>> {
    let regex = Regex::new(
        r"(?P<name>\w+): Mean \+- std dev: (?P<mean>\d+\.?\d*) ms \+- (?P<stddev>\d+\.?\d*) (?P<unit>\w+)"
    )?;

    let mut results = Vec::new();

    for found in regex.captures_iter(s) {
        let mut data = PerfData {
            name: found["name"].to_string(),
            commit_hash: git::get_commit_hash()?,
            map: HashMap::new(),
        };

        data.map.insert("mean".to_string(), found["mean"].parse()?);
        data.map.insert("stddev".to_string(), found["stddev"].parse()?);

        results.push(data);
    }

    if results.is_empty() {
        return Err("No matches found".into());
    }

    Ok(results)
}