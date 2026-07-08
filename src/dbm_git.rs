use crate::parser::PerfData;
use crate::git::SlugGit;
use crate::dbm_csv;
use csv::Writer;
use std::io::{BufReader, Cursor};
use crate::errors::SlugError;


// Which history to use for read/write
pub enum Store {
    Shared,
    Local,
}

impl Store {
    // Handle to this store for the current branch. Reads resolve the inherited
    // baseline; writes create the branch ref on first record.
    pub fn open(&self) -> Result<SlugGit, SlugError> {
        match self {
            Store::Shared => SlugGit::shared(),
            Store::Local => SlugGit::local(),
        }
    }
}


pub fn insert(slug_git: &SlugGit, data: &Vec<PerfData>) -> Result<(), SlugError> {
    if data.is_empty() {
        return Ok(());
    }

    let mut updates = Vec::new();
    let commit_hash = &data[0].commit_hash;

    for entry in data.iter() {
        let mut csv_buffer = Vec::new();
        let writer = Writer::from_writer(Cursor::new(&mut csv_buffer));
        // Notes are self-describing, every note carries its test's header
        dbm_csv::get_data_entry_string(writer, entry, false)?;

        let csv_string = String::from_utf8(csv_buffer)?;
        updates.push((entry.name.clone(), csv_string));
    }

    // Each commit's own benchmark records are stored directly in its note
    slug_git.record_data(commit_hash, &updates)?;

    // Keep loose objects from piling up
    if commit_hash.ends_with('0') {
        if let Err(e) = slug_git.gc_auto() {
            eprintln!("Warning: git gc --auto failed: {}", e);
        }
    }

    Ok(())
}


pub fn get_latest_n(slug_git: &SlugGit, name: &String, n: usize) -> Result<Vec<PerfData>, SlugError> {
    // Inherited history for this test, or nothing if no ancestor was benchmarked
    let data = match slug_git.read_base_file(name, Some(n))? {
        Some(data) => data,
        None => return Ok(Vec::new()),
    };

    let cursor = Cursor::new(data);
    let reader = BufReader::new(cursor);

    dbm_csv::get_latest_n(reader, name, n)
}


// Dump stored CSV data for export
// If target is None all data is exported
// Returns (test_name, raw_csv)
pub fn export(slug_git: &SlugGit, target: Option<&str>) -> Result<Vec<(String, String)>, SlugError> {
    let mut exports = slug_git.read_all_history()?;

    if let Some(name) = target {
        exports.retain(|(test, _)| test == name);
        if exports.is_empty() {
            return Err(SlugError::parsing(format!("No history for test '{}'", name)));
        }
    }

    Ok(exports)
}