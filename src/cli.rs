extern crate getopts;

use std::io;
use std::fs::File;
use getopts::Options;
use std::env;

pub struct CliOptions {
    pub file: Option<String>,
    pub library_type: Option<String>,
}

pub enum PerfDataReader {
    Stdin(io::Stdin),
    File(File),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} -t LIBRARY -f FILE", program);
    print!("{}", opts.usage(&brief));
}

pub fn parse_args() -> CliOptions {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("f", "file", "set input file name", "FILENAME");
    opts.optopt("t", "type", "set library type", "LIBRARY");
    opts.optflag("h", "help", "print this help menu");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!("{}", f.to_string()) }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        std::process::exit(0);
    }

    let file = matches.opt_str("f");
    let library_type = matches.opt_str("t");

    if library_type.is_none() {
        println!("Missing mandatory option -t");
        print_usage(&program, opts);
        std::process::exit(1);
    };

    CliOptions { file, library_type }
}


pub fn get_reader(options: CliOptions) -> PerfDataReader {
    match options.file {
        Some(file_name) => {
            PerfDataReader::File(File::open(file_name).expect("Failed to open the file"))
        },
        None => {
            PerfDataReader::Stdin(io::stdin())
        }
    }
}
