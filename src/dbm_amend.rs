use std::fs::{OpenOptions, File, create_dir_all};
use std::io::{BufWriter, BufReader, Cursor};
use crate::parser::PerfData;
use csv::Writer;
use crate::git;
use crate::git::SlugGit;
use crate::dbm_csv;
use crate::errors::SlugError;


pub fn insert(slug_git: &SlugGit, data: &Vec<PerfData>) -> Result<(), SlugError> {
    for entry in data.iter() {
        let folder_path = ".slug";
        create_dir_all(folder_path)?;

        let file_path = format!("{}/{}.csv", folder_path, entry.name);
        
        let file_exists = std::path::Path::new(&file_path).exists();
        
        let file = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&file_path)?;
        let mut writer = Writer::from_writer(BufWriter::new(file));

        dbm_csv::get_data_entry_string(writer, entry, file_exists)?;
    }

    slug_git.amend_slug()?;
    Ok(())
}


pub fn get_latest_n(name: &String, n: usize) -> Result<Vec<PerfData>, SlugError> {
    let folder_path = ".slug";
    let file_path = format!("{}/{}.csv", folder_path, name);
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);


    dbm_csv::get_latest_n(reader, name, n)
}