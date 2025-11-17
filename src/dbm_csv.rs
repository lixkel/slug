use crate::parser::PerfData;

use std::fs::{OpenOptions, File, create_dir_all};
use std::io::{BufWriter, BufReader, Cursor};
use std::collections::HashMap;
use csv::{Writer, ReaderBuilder};
use crate::errors::SlugError;

use crate::git;
use crate::git::SlugGit;
use std::io::Read;


pub fn get_data_entry_string<W: std::io::Write>(mut writer: Writer<W>, data: &PerfData, file_exists: bool) -> Result<(), SlugError> {
    // Sort keys
    let mut keys: Vec<&String> = data.map.keys().collect();
    keys.sort_unstable();
    
    // TODO: make this a PerfData function
    let mut values = keys
        .iter()
        .map(|&key| data.map.get(key).unwrap().to_string())
        .collect::<Vec<String>>();

    // Add commit_hash to output
    let commit_hash_str = "commit_hash".to_string();
    keys.push(&commit_hash_str);
    values.push(data.commit_hash.clone());

    if !file_exists {
        writer.write_record(&keys)?;
    }

    // Write records to writer
    writer.write_record(&values)?;
    writer.flush()?;

    Ok(())
}


pub fn get_latest_n<R: Read>(reader: BufReader<R>, name: &String, n: usize)  -> Result<Vec<PerfData>, SlugError> {
    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader.headers()?.clone();

    let mut records: Vec<PerfData> = Vec::new();

    // TODO: there has to be way to do this without loading it all into vector
    let results: Vec<_> = csv_reader.records().collect::<Result<Vec<_>, csv::Error>>()?;
    for result in results.iter().rev().take(n) {
        
        let mut commit_hash = String::new();
        let mut map: HashMap<String, f64> = HashMap::new();
        for (header, value) in headers.iter().zip(result.iter()) {
            let header_str = header.to_string();
            if header == "commit_hash" {
                commit_hash = value.to_string();
                continue;
            }

            map.insert(header_str, value.parse::<f64>()?);
        }

        records.push(PerfData {
            name: name.clone(),
            commit_hash: commit_hash,
            map,
        });
    }

    // TODO: check the proper order
    records.reverse();

    Ok(records)
}