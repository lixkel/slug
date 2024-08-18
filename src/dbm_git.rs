use crate::parser::PerfData;
use crate::git;
use crate::dbm_csv;


use std::fs::{OpenOptions, File, create_dir_all};
use std::io::{Write, BufWriter, BufReader};
use std::collections::HashMap;
use std::error::Error;


pub fn insert(data: &PerfData) -> Result<(), Box<dyn Error>> {
    let origin_branch = git::get_cur_branch()?;
    let slug_branch = "slug-".to_string() + &origin_branch;
    let cur_commit = git::get_commit_hash()?;

    git::checkout_branch(&slug_branch)?;

    dbm_csv::insert(&data)?;
    git::commit_data(&cur_commit)?;

    git::checkout_branch(&origin_branch)?;

    Ok(())
}


pub fn get_latest_n(name: &String, n: usize) -> Result<Vec<PerfData>, Box<dyn Error>> {
    let origin_branch = git::get_cur_branch()?;
    let slug_branch = "slug-".to_string() + &origin_branch;

    git::checkout_branch(&slug_branch)?;

    // TODO: think about if i should not only get one data point from each checkout
    let latest_n = dbm_csv::get_latest_n(name, n)?;

    git::checkout_branch(&origin_branch)?;

    Ok(latest_n)
}