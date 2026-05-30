pub mod error;
mod grammar;
pub mod ast;

use clap::Parser;
use error::Result;
use std::{path::PathBuf, process::exit};
use grammar::DnjParser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Root description file
    #[arg(short, long)]
    input: PathBuf,
}

fn run(args: Args) -> Result<()> {
    let ast = DnjParser::parse_file(args.input)?;
    println!("Done: {:#?}", ast);
    Ok(())
}

fn main() {
    match run(Args::parse()) {
        Ok(_) => {
            exit(0);
        }
        Err(err) => {
            println!("{}", err);
            println!("{:#?}", err);
            exit(1);
        }
    }
}
