use crate::parser::PerfData;

use std::fs::{OpenOptions, File, create_dir_all};
use std::io::{Write, BufWriter, BufReader};
use std::collections::HashMap;
use std::error::Error;
use csv::{Writer, Reader, ReaderBuilder};


pub fn insert(data: &PerfData) -> Result<(), Box<dyn Error>> {
    let folder_path = ".slug";
    create_dir_all(folder_path)?;

    let file_path = format!("{}/{}.csv", folder_path, data.name);
    
    let file_exists = std::path::Path::new(&file_path).exists();
    
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(&file_path)?;
    let mut writer = Writer::from_writer(BufWriter::new(file));

    // Sort keys
    let mut keys: Vec<&String> = data.map.keys().collect();
    keys.sort_unstable();

    if !file_exists {
        writer.write_record(&keys)?;
    }

    let values = keys
        .iter()
        .map(|&key| data.map.get(key).unwrap().to_string())
        .collect::<Vec<String>>();

    writer.write_record(&values)?;

    writer.flush()?;
    Ok(())
}


pub fn get_latest_n(name: &String, n: usize) -> Result<Vec<PerfData>, Box<dyn Error>> {
    let folder_path = ".slug";
    let file_path = format!("{}/{}.csv", folder_path, name);
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader.headers()?.clone();

    let mut records: Vec<PerfData> = Vec::new();

    // TODO: there has to be way to do this without loading it all into vector
    let results: Vec<_> = csv_reader.records().collect::<Result<Vec<_>, csv::Error>>()?;
    for result in results.iter().rev().take(n) {
        
        let mut map: HashMap<String, f64> = HashMap::new();
        for (header, value) in headers.iter().zip(result.iter()) {
            map.insert(header.to_string(), value.parse::<f64>()?);
        }

        records.push(PerfData {
            name: name.clone(),
            map,
        });
    }

    // TODO: check the proper order
    records.reverse();

    Ok(records)
}