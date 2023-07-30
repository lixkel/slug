mod cli;
mod parser;

fn main() {
    let options = cli::parse_args();
    let reader = cli::get_reader(&options.file);

    let lib_str = &options.library_type.expect("Missing mandatory option -t");
    let lib = parser::Lib::from_str(lib_str).expect("Library in bad format");

    println!("fuuu {:?}", lib);
    let data = parser::parse(reader, &lib);
    println!("{:?}", data);
    

}