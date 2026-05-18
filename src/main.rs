mod cli;
mod git;
mod parser;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod dbm_amend;
mod errors;

use errors::SlugError;

fn main() -> Result<(), SlugError> {
    let options = cli::parse_args()?;

    let reader = cli::get_reader(&options.file)?;

    let lib_str = &options.library_type.ok_or_else(|| SlugError::Cli("Missing mandatory option -t".to_string()))?;
    let lib = parser::Lib::from_str(lib_str).ok_or_else(|| SlugError::Parsing("Library in bad format".to_string()))?;

    let data = parser::parse(reader, &lib)?;

    /*
        match statistics::calculate_stats(&mut data) {
        Ok(_) => {},
        Err(e) => panic!("Error occurred while parsing: {}", e),
    };
    */

    let ldata;
    let slug_git = git::SlugGit::new()?;

    if options.amend {
        dbm_amend::insert(&slug_git, &data)?;
        ldata = dbm_amend::get_latest_n(&data[0].name, 3)?;
    } else {
        dbm_git::insert(&slug_git, &data)?;
        ldata = dbm_git::get_latest_n(&slug_git, &data[0].name, 3)?;
    }

    statistics::calculate_stats(&ldata, &options)?;

    Ok(())
    }