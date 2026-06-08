extern crate getopts;

use std::io;
use std::fs::File;
use getopts::Options;
use std::env;
use crate::errors::SlugError;

// Where to save the data from current run / where to look for history
pub enum Mode {
    DryRun,
    Local,
    Shared,
}

pub struct CliOptions {
    pub file: Option<String>,
    pub library_type: Option<String>,
    pub mode: Mode,
    pub subcommand: Option<String>,
    pub target: Option<String>,
    pub zscore_threshold: f64,
    pub ewma_alpha: f64,
}

pub enum PerfDataReader {
    Stdin(io::Stdin),
    File(File),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!(
        "Usage: {prog} -t LIBRARY [-f FILE] [--local | --shared] [--zscore THRESHOLD] [--ewma ALPHA]\n       {prog} history [TEST] [--local]\n       {prog} clean",
        prog = program
    );
    print!("{}", opts.usage(&brief));
}

pub fn parse_args() -> Result<CliOptions, SlugError> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let subcommand = if args.len() > 1 && !args[1].starts_with('-') {
        let subcommand = args[1].clone();
        match subcommand.as_str() {
            "clean" | "history" => Some(subcommand),
            _ => None,
        }
    } else { None };

    let mut opts = Options::new();
    opts.optopt("f", "file", "set input file name", "FILENAME");
    opts.optopt("t", "type", "set library type", "LIBRARY");
    opts.optflag("l", "local", "store records locally");
    opts.optflag("s", "shared", "store records to shared git history (slug branch)");
    opts.optopt("", "zscore", "set z-score anomaly threshold (default: 3.0)", "FLOAT");
    opts.optopt("", "ewma", "set ewma smoothing factor alpha (default: 0.2)", "FLOAT");
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

    // Positional argument (test name in `slug history fib32`)
    let target = matches.free.first().cloned();

    let mode = match (local, shared) {
        (false, false) => Mode::DryRun,
        (true, false) => Mode::Local,
        (false, true) => Mode::Shared,
        (true, true) => return Err(SlugError::Cli("Use at most one of --local, --shared".to_string())),
    };

    let zscore_threshold = match matches.opt_str("zscore") {
        Some(val) => val.parse::<f64>().map_err(|_| SlugError::Cli("Invalid float for zscore".to_string()))?,
        None => 3.0,
    };

    let ewma_alpha = match matches.opt_str("ewma") {
        Some(val) => val.parse::<f64>().map_err(|_| SlugError::Cli("Invalid float for ewma".to_string()))?,
        None => 0.2,
    };

    if library_type.is_none() && subcommand.is_none() {
        print_usage(&program, opts);
        return Err(SlugError::Cli("Missing mandatory option -t".to_string()));
    };

    Ok(CliOptions {
        file,
        library_type,
        mode,
        subcommand,
        target,
        zscore_threshold,
        ewma_alpha,
    })
}


pub fn get_reader(file: &Option<String>) -> Result<PerfDataReader, SlugError> {
    match &file {
        Some(file_name) => {
            let file = File::open(file_name)?;
            Ok(PerfDataReader::File(file))
        },
        None => {
            Ok(PerfDataReader::Stdin(io::stdin()))
        }
    }
}
