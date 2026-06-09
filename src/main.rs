mod cli;
mod git;
mod parser;
mod lib_parsers;
mod units;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod errors;

use errors::SlugError;

fn main() -> Result<(), SlugError> {
    let options = cli::parse_args()?;

    if options.subcommand.is_some() {
        return run_subcommand(&options);
    }

    let reader = cli::get_reader(&options.file)?;

    let lib_str = options.library_type.as_ref().ok_or_else(|| SlugError::Cli("Missing mandatory option -t".to_string()))?;
    let lib = parser::Lib::from_str(lib_str).ok_or_else(|| SlugError::Parsing("Library in bad format".to_string()))?;

    let data = parser::parse(reader, &lib)?;

    let name = data[0].name.clone();
    const BASELINE: usize = 3;

    let window = match options.mode {
        cli::Mode::Shared => {
            let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Shared, &name, BASELINE)?;
            dbm_git::insert(dbm_git::Store::Shared, &data)?;
            println!("Recorded to shared git history (refs/slug/shared)");
            baseline.push(data.into_iter().next().unwrap());
            baseline
        }
        cli::Mode::Local => {
            let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Local, &name, BASELINE)?;
            dbm_git::insert(dbm_git::Store::Local, &data)?;
            println!("Recorded to local history (refs/slug-local)");
            baseline.push(data.into_iter().next().unwrap());
            baseline
        }
        cli::Mode::DryRun => {
            println!("Dry run, nothing written (use --local or --shared to record)");
            let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Local, &name, BASELINE)?;
            baseline.push(data.into_iter().next().unwrap());
            baseline
        }
    };

    statistics::calculate_stats(&window, &options)?;

    Ok(())
}

fn run_subcommand(options: &cli::CliOptions) -> Result<(), SlugError> {
    let subcommand = options.subcommand.as_deref().unwrap_or("");
    match subcommand {
        "clean" => clean(),
        "history" => history(options),
        _ => Err(SlugError::Cli(format!("Unknown subcommand '{}'", subcommand))),
    }
}

fn clean() -> Result<(), SlugError> {
    let removed = git::clean()?;
    if removed.is_empty() {
        println!("Nothing to clean, no slug data found");
    } else {
        println!("Cleaning successful");
    }
    Ok(())
}

fn history(options: &cli::CliOptions) -> Result<(), SlugError> {
    let store = match options.mode {
        cli::Mode::Local => dbm_git::Store::Local,
        _ => dbm_git::Store::Shared,
    };

    let exports = dbm_git::export(store, options.target.as_deref())?;
    if exports.is_empty() {
        println!("No history found");
        return Ok(());
    }

    print!("{}", dbm_csv::format_export(&exports));
    Ok(())
}