mod cli;
mod git;
mod parser;
mod statistics;
mod dbm_csv;
mod dbm_git;
mod dbm_amend;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let options = cli::parse_args();


    let reader = cli::get_reader(&options.file);

    let lib_str = &options.library_type.expect("Missing mandatory option -t");
    let lib = parser::Lib::from_str(lib_str).expect("Library in bad format");

    // TODO: look at error handling
    let mut data = match parser::parse(reader, &lib) {
        Ok(v) => v,
        Err(e) => panic!("Parsing error: {:?}", e),
    };

    /*
        match statistics::calculate_stats(&mut data) {
        Ok(_) => {},
        Err(e) => panic!("Error occurred while parsing: {}", e),
    };
    */

    //println!("{:#?}", data);

    let ldata;
    let mut slug_git = git::SlugGit::new()?;

    if options.amend {
        dbm_amend::insert(&slug_git, &data)?;
        ldata = dbm_amend::get_latest_n(&data[0].name, 3)?;
    } else {
        dbm_git::insert(&slug_git, &data)?;
        ldata = dbm_git::get_latest_n(&slug_git, &data[0].name, 3)?;
    }
    //println!("{:#?}", ldata);

    statistics::ewma(&ldata, 0.2);

    Ok(())
}