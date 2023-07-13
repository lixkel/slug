mod cli;

use cli::parse_args;

fn main() {
    let args = parse_args();

    match args.file {
        Some(string) => {
            println!("The value is: {}", string);
        },
        None => {
            println!("The value is None.");
        }
    }
}