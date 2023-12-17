mod cli;
mod parser;
mod statistics;

fn main() {
    let options = cli::parse_args();
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
}