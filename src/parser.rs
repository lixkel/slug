use crate::cli;

use std::io::{self, Read, BufReader};
use regex::Regex;

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

impl Lib {
    pub fn from_string(s: &str) -> Option<Self> {
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

pub fn parse(reader: cli::PerfDataReader, lib: Lib) {
    
}