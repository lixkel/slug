mod cli;
mod git;
mod parser;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod dbm_amend;
mod setup;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let options = cli::parse_args();

    match options.subcommand.as_deref() {
        Some("setup") => {
            setup::setup()?;
            return Ok(())
        }
        _ => {}
    }

    let reader = cli::get_reader(&options.file);

    let lib_str = &options.library_type.expect("Missing mandatory option -t");
    let lib = parser::Lib::from_str(lib_str).expect("Library in bad format");

    let mut data = match parser::parse(reader, &lib) {
        Ok(v) => v,
        Err(e) => panic!("Parsing error: {:?}", e),
    };

    match statistics::calculate_stats(&mut data) {
        Ok(_) => {},
        Err(e) => panic!("Error occurred while parsing: {}", e),
    };

    println!("{:#?}", data);

    let ldata;

    if options.amend {
        dbm_amend::insert(&data)?;
        ldata = dbm_amend::get_latest_n(&data.name, 3)?;
    } else {
        dbm_git::insert(&data)?;
        ldata = dbm_git::get_latest_n(&data.name, 3)?;
    }
    println!("{:#?}", ldata[0]);

    statistics::ewma(&ldata, 0.2);

    Ok(())
}