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
    // Writable, creates the ref if missing
    fn open_write(&self) -> Result<SlugGit, SlugError> {
        match self {
            Store::Shared => SlugGit::shared(),
            Store::Local => SlugGit::local(),
        }
    }

    // Readonly, doesn't create ref if missing
    fn open_read(&self) -> Result<SlugGit, SlugError> {
        match self {
            Store::Shared => SlugGit::open_shared(),
            Store::Local => SlugGit::open_local(),
        }
    }
}


// TODO: dont checkout just write directly the data to branch
pub fn insert(store: Store, data: &Vec<PerfData>) -> Result<(), SlugError> {
    if data.is_empty() {
        return Ok(());
    }

    let slug_git = store.open_write()?;

    let mut updates = Vec::new();
    let commit_hash = &data[0].commit_hash;

    for entry in data.iter() {
        let mut csv_buffer = Vec::new();
        let writer = Writer::from_writer(Cursor::new(&mut csv_buffer));
        dbm_csv::get_data_entry_string(writer, entry, slug_git.file_exists(&entry.name)?)?;

        let csv_string = String::from_utf8(csv_buffer).map_err(|e| SlugError::Parsing(e.to_string()))?;
        updates.push((entry.name.clone(), csv_string));
    }

    let slug_commit_hash = slug_git.edit_branch_slug(commit_hash, &updates)?;

    let note_message = format!("Benchmark-Results: {}", slug_commit_hash);
    slug_git.add_note(commit_hash, &note_message)?;

    Ok(())
}


pub fn get_latest_n(store: Store, name: &String, n: usize) -> Result<Vec<PerfData>, SlugError> {
    let slug_git = store.open_read()?;

    // Check if historical data exists for this test
    if !slug_git.file_exists(name)? {
        return Ok(Vec::new());
    }

    let cursor = Cursor::new(slug_git.read_file_slug(name)?);
    let reader = BufReader::new(cursor);

    // TODO: think about if i should not only get one data point from each checkout
    dbm_csv::get_latest_n(reader, name, n)
}


// Dump stored CSV data for export. If target is None data for all tests are exported
// Returns (test_name, raw_csv)
pub fn export(store: Store, target: Option<&str>) -> Result<Vec<(String, String)>, SlugError> {
    let slug_git = store.open_read()?;

    let names = match target {
        Some(name) => {
            let name = name.to_string();
            if !slug_git.file_exists(&name)? {
                return Err(SlugError::Parsing(format!("No history for test '{}'", name)));
            }
            vec![name]
        }
        None => slug_git.list_files()?,
    };

    let mut exports = Vec::new();
    for name in names {
        let content = String::from_utf8(slug_git.read_file_slug(&name)?)
            .map_err(|e| SlugError::Parsing(e.to_string()))?;
        exports.push((name, content));
    }

    Ok(exports)
}