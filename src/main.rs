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
    } else {
        println!("Dry run, nothing written (use --local or --shared to record)");
        let mut baseline = dbm_git::get_latest_n(dbm_git::Store::Local, &name, BASELINE)?;
        baseline.push(data.into_iter().next().unwrap());
        baseline
    };

    statistics::calculate_stats(&window, &options)?;

    Ok(())
}

fn run_subcommand(options: &cli::CliOptions) -> Result<(), SlugError> {
    let subcommand = options.subcommand.as_deref().unwrap_or("");
    match subcommand {
        "clean" => {
            let removed = git::clean()?;
            if removed.is_empty() {
                println!("Nothing to clean, no slug data found");
            } else {
                println!("Cleaning successful");
            }
            Ok(())
        }
        "history" => {
            let store = if options.local {
                dbm_git::Store::Local
            } else {
                dbm_git::Store::Shared
            };

            let exports = dbm_git::export(store, options.target.as_deref())?;
            if exports.is_empty() {
                println!("No history found");
                return Ok(());
            }

            // One block per test, separated by blank line, each test marked "# name"
            for (i, (name, content)) in exports.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("# {}", name);
                print!("{}", content);
            }
            Ok(())
        }
        _ => Err(SlugError::Cli(format!("Unknown subcommand '{}'", subcommand))),
    }
}