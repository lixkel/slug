use crate::parser::PerfData;
use crate::git::SlugGit;
use crate::dbm_csv;
use csv::Writer;
use std::io::{BufReader, Cursor};
use crate::errors::SlugError;


// TODO: dont checkout just write directly the data to branch
pub fn insert(slug_git: &SlugGit, data: &Vec<PerfData>) -> Result<(), SlugError> {
    if data.is_empty() {
        return Ok(());
    }

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


pub fn get_latest_n(slug_git: &SlugGit, name: &String, n: usize) -> Result<Vec<PerfData>, SlugError> {
    let cursor = Cursor::new(slug_git.read_file_slug(&name)?);
    let reader = BufReader::new(cursor);

    // TODO: think about if i should not only get one data point from each checkout
    dbm_csv::get_latest_n(reader, name, n)
}