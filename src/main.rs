mod cli;
mod config;
mod git;
mod parser;
mod lib_parsers;
mod units;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod errors;
#[cfg(test)]
mod tests;

use errors::SlugError;
use std::io::{self, Write};
use std::process::ExitCode;

// Exit codes, to differentiate between regression error and generic error
enum Exit {
    Success = 0,
    Error = 1,
    Regression = 2,
}

impl Exit {
    fn trigger(self) -> ExitCode {
        ExitCode::from(self as u8)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => Exit::Success.trigger(),
        Err(SlugError::PerformanceRegression(msg)) => {
            eprintln!("Performance regression: {}", msg);
            Exit::Regression.trigger()
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
            Exit::Error.trigger()
        }
    }
}

fn run() -> Result<(), SlugError> {
    let config = config::load_or_default()?;
    let options = cli::parse_args(&config)?;

    if options.subcommand.is_some() {
        return run_subcommand(&options);
    }

    let reader = cli::get_reader(&options.file)?;

    let lib_str = options.library_type.as_ref().ok_or_else(|| SlugError::Cli("Missing mandatory option -t".to_string()))?;
    let lib = parser::Lib::from_str(lib_str).ok_or_else(|| SlugError::Parsing("Library in bad format".to_string()))?;

    let mut data = parser::parse(reader, &lib)?;

    // Stamp every benchmark with commit it was measured on
    let commit_hash = git::get_commit_hash()?;
    for entry in &mut data {
        entry.commit_hash = commit_hash.clone();
    }

    // TODO: check if here shouldnt be loop
    let name = data[0].name.clone();

    // Open repo slug was called in
    let slug_git = options.storage.open()?;

    // TODO: maybe put this after insert
    let mut window = dbm_git::get_latest_n(&slug_git, &name, config.max_window())?;

    if options.write {
        dbm_git::insert(&slug_git, &data)?;
        println!("Recorded to {}", slug_git.notes_ref);
    } else {
        println!("Dry run, nothing written (use --record to store)");
    }

    window.push(data.into_iter().next().unwrap());

    statistics::calculate_stats(&window, &options, &config)?;

    Ok(())
}

fn run_subcommand(options: &cli::CliOptions) -> Result<(), SlugError> {
    let subcommand = options.subcommand.as_deref().unwrap_or("");
    match subcommand {
        "clean" => clean(options),
        "prune" => prune(),
        "history" => history(options),
        "setup" => config::write_example(),
        _ => Err(SlugError::Cli(format!("Unknown subcommand '{}'", subcommand))),
    }
}

// Ask yes/no question, anything except yes means no
fn confirm(question: &str) -> Result<bool, SlugError> {
    print!("{} [y/N] ", question);
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

fn clean(options: &cli::CliOptions) -> Result<(), SlugError> {
    let slug_git = options.storage.open()?;
    if !confirm(&format!("Delete all Slug history in {}?", slug_git.notes_ref))? {
        println!("Aborted, nothing deleted");
        return Ok(());
    }
    if !slug_git.clean()? {
        println!("Nothing to clean, no slug data found");
        return Ok(());
    }
    println!("Cleaning successful");
    if matches!(options.storage, dbm_git::Store::Shared) {
        println!("To delete the shared history from the remote, run:");
        println!("    git push origin --delete {}", slug_git.notes_ref);
    }
    Ok(())
}

fn prune() -> Result<(), SlugError> {
    let removed = git::prune()?;
    if removed.is_empty() {
        println!("Nothing to prune, everything is still reachable");
    } else {
        println!("Pruned {} unreachable record(s)", removed.len());
        println!("To update the shared history on the remote as well, run:");
        println!("    git push origin refs/notes/slug-shared");
    }
    Ok(())
}

fn history(options: &cli::CliOptions) -> Result<(), SlugError> {
    let slug_git = options.storage.open()?;
    let exports = dbm_git::export(&slug_git, options.target.as_deref())?;
    if exports.is_empty() {
        println!("No history found");
        return Ok(());
    }

    print!("{}", dbm_csv::format_export(&exports));
    Ok(())
}
