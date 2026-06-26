use crate::parser::PerfData;

use std::io::BufReader;
use std::collections::HashMap;
use csv::{Writer, ReaderBuilder};
use crate::errors::SlugError;

use std::io::Read;


// Render exported (test_name, raw_csv) separated by "# name" and blank line
pub fn format_export(exports: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (name, content)) in exports.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("# {}\n", name));
        out.push_str(content);
    }
    out
}


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


// Parse a test's CSV history and return its latest n records, oldest first
pub fn get_latest_n<R: Read>(reader: BufReader<R>, name: &String, n: usize) -> Result<Vec<PerfData>, SlugError> {
    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader.headers()?.clone();

    let mut records: Vec<PerfData> = Vec::new();

    let results: Vec<_> = csv_reader.records().collect::<Result<Vec<_>, csv::Error>>()?;
    // Records are appended oldest first, so take the last n
    for result in results.iter().rev().take(n) {
        let mut commit_hash = String::new();
        let mut map: HashMap<String, f64> = HashMap::new();
        for (header, value) in headers.iter().zip(result.iter()) {
            if header == "commit_hash" {
                commit_hash = value.to_string();
                continue;
            }

            map.insert(header.to_string(), value.parse::<f64>()?);
        }

        records.push(PerfData {
            name: name.clone(),
            commit_hash,
            map,
        });
    }

    // Flip back to oldest first
    records.reverse();

    Ok(records)
}