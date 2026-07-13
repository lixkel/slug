mod cli;
mod config;
mod git;
mod parser;
mod lib_parsers;
mod units;
mod statistics;
mod terms;
mod dbm_csv;
mod dbm_git;
mod errors;
#[cfg(test)]
mod tests;

use errors::SlugError;
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
        Ok(None) => Exit::Success.trigger(),
        Ok(Some(msg)) => {
            terms::error(&format!("Performance regression {}", msg));
            Exit::Regression.trigger()
        }
        Err(e) => {
            terms::error(&e.to_string());
            Exit::Error.trigger()
        }
    }
}

// Ok(Some(report)) = Slug found regression
// Err = Slug itself failed
fn run() -> Result<Option<String>, SlugError> {
    let config = config::load_or_default()?;
    let options = cli::parse_args(&config)?;

    if options.subcommand.is_some() {
        return run_subcommand(&options).map(|()| None);
    }

    let reader = cli::get_reader(&options.file)?;

    let lib_str = options.library_type.as_ref().ok_or_else(|| SlugError::cli("Missing mandatory option -t"))?;
    let lib = parser::Lib::from_str(lib_str).ok_or_else(|| SlugError::parsing("Library in bad format"))?;

    let mut data = parser::parse(reader, &lib)?;

    if data.is_empty() {
        return Err(SlugError::parsing(format!("No benchmarks found in input, is -t {} right?", lib_str)));
    }

    // Stamp every benchmark with commit it was measured on
    let commit_hash = git::get_commit_hash()?;
    for entry in &mut data {
        entry.commit_hash = commit_hash.clone();
    }

    // Open repo slug was called in
    let slug_git = options.storage.open()?;

    if options.write {
        dbm_git::insert(&slug_git, &data)?;
        terms::line(&format!("Recorded to {}", slug_git.notes_ref));
    } else {
        terms::line("Dry run, nothing written (use --record to store)");
    }

    // Check every benchmark against its own history (reads exclude HEAD's fresh note)
    let total = data.len();
    let mut failed: Vec<String> = Vec::new();
    for entry in data {
        let name = entry.name.clone();
        terms::section(&format!("Checks for benchmark {}", name));

        let mut window = dbm_git::get_latest_n(&slug_git, &name, config.max_window())?;
        window.push(entry);

        let mut verdicts = statistics::run_checks(&window, &options, &config)?;
        let mut report = statistics::combine(&verdicts, config.policy);

        // Run rule fires regardless of policy
        if !report.flagged {
            if let Some(reason) = statistics::confidence_run(&window, &options, &config)? {
                report.flagged = true;
                report.reasons.push(reason.clone());
                let run = config.confidence.run as f64;
                verdicts.push(statistics::CheckVerdict::flagged(run, run, reason));
            }
        }

        terms::print_verdicts(&verdicts);

        if report.flagged {
            failed.push(format!("{}: {}", name, report.reasons.join("; ")));
        }
    }

    if failed.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("in {} of {} benchmarks:\n  {}", failed.len(), total, failed.join("\n  "))))
}

fn run_subcommand(options: &cli::CliOptions) -> Result<(), SlugError> {
    let subcommand = options.subcommand.as_deref().unwrap_or("");
    match subcommand {
        "clean" => clean(options),
        "prune" => prune(),
        "history" => history(options),
        "setup" => config::write_example(),
        _ => Err(SlugError::cli(format!("Unknown subcommand '{}'", subcommand))),
    }
}

fn clean(options: &cli::CliOptions) -> Result<(), SlugError> {
    let slug_git = options.storage.open()?;
    if !terms::confirm(&format!("Delete all Slug history in {}?", slug_git.notes_ref))? {
        terms::line("Aborted, nothing deleted");
        return Ok(());
    }
    if !slug_git.clean()? {
        terms::line("Nothing to clean, no slug data found");
        return Ok(());
    }
    terms::line("Cleaning successful");
    if matches!(options.storage, dbm_git::Store::Shared) {
        terms::line("To delete the shared history from the remote, run:");
        terms::line(&format!("    git push origin --delete {}", slug_git.notes_ref));
    }
    Ok(())
}

fn prune() -> Result<(), SlugError> {
    let removed = git::prune()?;
    if removed.is_empty() {
        terms::line("Nothing to prune, everything is still reachable");
    } else {
        terms::line(&format!("Pruned {} unreachable record(s)", removed.len()));
        terms::line("To update the shared history on the remote as well, run:");
        terms::line("    git push origin refs/notes/slug-shared");
    }
    Ok(())
}

fn history(options: &cli::CliOptions) -> Result<(), SlugError> {
    let slug_git = options.storage.open()?;
    let exports = dbm_git::export(&slug_git, options.target.as_deref())?;
    if exports.is_empty() {
        terms::line("No history found");
        return Ok(());
    }

    terms::raw(&dbm_csv::format_export(&exports));
    Ok(())
}
