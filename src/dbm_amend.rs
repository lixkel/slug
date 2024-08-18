use crate::parser::PerfData;
use crate::git;
use crate::dbm_csv;

use std::error::Error;


pub fn insert(data: &PerfData) -> Result<(), Box<dyn Error>> {
    dbm_csv::insert(data)?;
    git::amend_slug()?;
    Ok(())
}


pub fn get_latest_n(name: &String, n: usize) -> Result<Vec<PerfData>, Box<dyn Error>> {
    dbm_csv::get_latest_n(name, n)
}