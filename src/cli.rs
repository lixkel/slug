extern crate getopts;

use std::io::{self, IsTerminal};
use std::fs::File;
use getopts::Options;
use std::env;
use crate::errors::SlugError;
use crate::dbm_git::Store;
use crate::config::Config;
use crate::terms;

pub struct CliOptions {
    pub file: Option<String>,
    pub library_type: Option<String>,
    pub storage: Store,
    pub write: bool,
    pub subcommand: Option<String>,
    pub target: Option<String>,
    pub ewma_alpha: f64,
    pub ewma_limit: f64,
    pub prediction_level: f64,
}

pub enum PerfDataReader {
    Stdin(io::Stdin),
    File(File),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!(
        "Usage: {prog} -t LIBRARY [-f FILE] [--local | --shared] [--record]\n       {prog} history [TEST] [--local | --shared]\n       {prog} setup\n       {prog} clean [--local | --shared]\n       {prog} prune\n\nLIBRARY is name@version, bare name means newest, supported: {libs}",
        prog = program,
        libs = crate::parser::lib_names().join(", ")
    );
    terms::raw(&opts.usage(&brief));
}

pub fn parse_args(config: &Config) -> Result<CliOptions, SlugError> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let subcommand = if args.len() > 1 && !args[1].starts_with('-') {
        let subcommand = args[1].clone();
        match subcommand.as_str() {
            "clean" | "prune" | "history" | "setup" => Some(subcommand),
            _ => return Err(SlugError::cli(format!("Unknown subcommand '{}'", subcommand))),
        }
    } else { None };

    let mut opts = Options::new();
    opts.optopt("f", "file", "set input file name", "FILENAME");
    opts.optopt("t", "type", "set library type", "LIBRARY");
    opts.optflag("l", "local", "use local history (default)");
    opts.optflag("s", "shared", "use shared git history (refs/notes/slug-shared)");
    opts.optflag("", "record", "record this run (default is dry run)");
    opts.optflag("h", "help", "print this help menu");

    let parse_start = if subcommand.is_some() { 2 } else { 1 };
    let matches = opts.parse(&args[parse_start..])?;

    if matches.opt_present("h") {
        print_usage(&program, opts);
        std::process::exit(0);
    }

    let file = matches.opt_str("f");
    let library_type = matches.opt_str("t");
    let local = matches.opt_present("l");
    let shared = matches.opt_present("s");
    let write = matches.opt_present("record");

    // Positional argument, only `history` subcommand takes one
    let target = matches.free.first().cloned();
    if let Some(stray) = &target {
        if subcommand.as_deref() != Some("history") {
            return Err(SlugError::cli(format!("Unexpected argument '{}', input files are passed with -f", stray)));
        }
    }

    let storage = match (local, shared) {
        (true, true) => return Err(SlugError::cli("Use at most one of --local, --shared")),
        (_, true) => Store::Shared,
        _ => Store::Local,
    };

    let ewma_alpha = config.ewma.alpha;
    let ewma_limit = config.ewma.limit;
    let prediction_level = config.prediction_bound.level;

    if library_type.is_none() && subcommand.is_none() {
        print_usage(&program, opts);
        return Err(SlugError::cli("Missing mandatory option -t"));
    };

    Ok(CliOptions {
        file,
        library_type,
        storage,
        write,
        subcommand,
        target,
        ewma_alpha,
        ewma_limit,
        prediction_level,
    })
}


pub fn get_reader(file: &Option<String>) -> Result<PerfDataReader, SlugError> {
    match &file {
        Some(file_name) => {
            // Bare io error does not say which path failed
            let file = File::open(file_name)
                .map_err(|e| SlugError::cli(format!("Cannot open '{}': {}", file_name, e)))?;
            Ok(PerfDataReader::File(file))
        },
        None => {
            // Warn interactive users who probably forgot -f, stay quiet when piped
            if io::stdin().is_terminal() {
                terms::warn("No -f given, reading benchmark output from stdin (Ctrl-D to end)");
            }
            Ok(PerfDataReader::Stdin(io::stdin()))
        }
    }
}
