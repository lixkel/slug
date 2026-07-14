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
    pub zscore_threshold: f64,
    pub ewma_alpha: f64,
    pub ewma_threshold: f64,
    pub confidence_level: f64,
}

pub enum PerfDataReader {
    Stdin(io::Stdin),
    File(File),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!(
        "Usage: {prog} -t LIBRARY [-f FILE] [--local | --shared] [--record] [--zscore THRESHOLD] [--ewma ALPHA] [--confidence LEVEL]\n       {prog} history [TEST] [--local | --shared]\n       {prog} setup\n       {prog} clean [--local | --shared]\n       {prog} prune\n\nLIBRARY is name@version, supported: {libs}",
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
    opts.optopt("", "zscore", "set z-score anomaly threshold (default: 3.0)", "FLOAT");
    opts.optopt("", "ewma", "set ewma smoothing factor alpha (default: 0.2)", "FLOAT");
    opts.optopt("", "confidence", "flag a new measurement above this share of other measurements (default: 0.95)", "FLOAT");
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

    // CLI flag value beats config value
    let zscore_threshold = match matches.opt_str("zscore") {
        Some(val) => val.parse::<f64>().map_err(|_| SlugError::cli("Invalid float for zscore"))?,
        None => config.zscore.threshold,
    };
    if !(zscore_threshold > 0.0) {
        return Err(SlugError::cli("Z-score threshold must be positive"));
    }

    let ewma_alpha = match matches.opt_str("ewma") {
        Some(val) => val.parse::<f64>().map_err(|_| SlugError::cli("Invalid float for ewma"))?,
        None => config.ewma.alpha,
    };
    if !(ewma_alpha > 0.0 && ewma_alpha <= 1.0) {
        return Err(SlugError::cli("EWMA alpha must be between 0 (exclusive) and 1"));
    }

    // no CLI flag for the ewma threshold; it comes from config (like the windows)
    let ewma_threshold = config.ewma.threshold;

    let confidence_level = match matches.opt_str("confidence") {
        Some(val) => val.parse::<f64>().map_err(|_| SlugError::cli("Invalid float for confidence"))?,
        None => config.confidence.level,
    };
    if !(confidence_level > 0.0 && confidence_level < 1.0) {
        return Err(SlugError::cli("Confidence level must be between 0 and 1 (exclusive)"));
    }

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
        zscore_threshold,
        ewma_alpha,
        ewma_threshold,
        confidence_level,
    })
}


pub fn get_reader(file: &Option<String>) -> Result<PerfDataReader, SlugError> {
    match &file {
        Some(file_name) => {
            let file = File::open(file_name)?;
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
