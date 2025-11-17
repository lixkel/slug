extern crate getopts;

use std::io;
use std::fs::File;
use getopts::Options;
use std::env;
use crate::errors::SlugError;

pub struct CliOptions {
    pub file: Option<String>,
    pub library_type: Option<String>,
    pub amend: bool,
    pub subcommand: Option<String>,
}

pub enum PerfDataReader {
    Stdin(io::Stdin),
    File(File),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} -t LIBRARY -f FILE", program);
    print!("{}", opts.usage(&brief));
}

pub fn parse_args() -> Result<CliOptions, SlugError> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let subcommand = if args.len() > 1 {
        let subcommand = args[1].clone();
        match subcommand.as_str() {
            "setup" => Some(subcommand),
            _ => None,
        }
    } else { None };

    let mut opts = Options::new();
    opts.optopt("f", "file", "set input file name", "FILENAME");
    opts.optopt("t", "type", "set library type", "LIBRARY");
    opts.optflag("a", "amend", "store records in current branch using commit amend");
    opts.optflag("h", "help", "print this help menu");

    let matches = opts.parse(&args[1..])?;

    if matches.opt_present("h") {
        print_usage(&program, opts);
        std::process::exit(0);
    }

    let file = matches.opt_str("f");
    let library_type = matches.opt_str("t");
    let amend = matches.opt_present("a");

    if library_type.is_none() && subcommand.is_none() {
        print_usage(&program, opts);
        return Err(SlugError::Cli("Missing mandatory option -t".to_string()));
    };

    Ok(CliOptions {
        file,
        library_type,
        amend,
        subcommand,
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
