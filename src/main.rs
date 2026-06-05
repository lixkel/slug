mod cli;
mod git;
mod parser;
mod lib_parsers;
mod units;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod dbm_amend;
mod errors;

use errors::SlugError;

fn main() -> Result<(), SlugError> {
    let options = cli::parse_args()?;

    let reader = cli::get_reader(&options.file)?;

    let lib_str = options.library_type.as_ref().ok_or_else(|| SlugError::Cli("Missing mandatory option -t".to_string()))?;
    let lib = parser::Lib::from_str(lib_str).ok_or_else(|| SlugError::Parsing("Library in bad format".to_string()))?;

    let data = parser::parse(reader, &lib)?;

    let name = data[0].name.clone();
    const BASELINE: usize = 3;

    let window = if options.shared {
        let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Shared, &name, BASELINE)?;
        dbm_git::insert(dbm_git::Store::Shared, &data)?;
        println!("Recorded to shared git history (refs/heads/slug)");
        baseline.push(data.into_iter().next().unwrap());
        baseline
    } else if options.local {
        let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Local, &name, BASELINE)?;
        dbm_git::insert(dbm_git::Store::Local, &data)?;
        println!("Recorded to local history (refs/slug-local)");
        baseline.push(data.into_iter().next().unwrap());
        baseline
    } else if options.amend {
        dbm_amend::insert(&data)?;
        dbm_amend::get_latest_n(&name, BASELINE)?
    } else {
        println!("Dry run, nothing written (use --local or --shared to record)");
        let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Local, &name, BASELINE)?;
        baseline.push(data.into_iter().next().unwrap());
        baseline
    };

    statistics::calculate_stats(&window, &options)?;

    Ok(())
}