pub mod error;
pub mod expr;
pub mod immap;
pub mod parser;
pub mod value;

use clap::Parser;
use error::Result;
use std::{path::PathBuf, process::exit};
use value::Value;

use crate::expr::{ExprRef, ExprSet, ExprType};

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Root description file
    #[arg(short, long)]
    input: PathBuf,
}

fn run(args: Args) -> Result<()> {
    let p = parser::Parser::new();
    let expr: ExprRef<Value> =
        ExprType::BoundExpr(ExprSet::new(), p.parse_file(args.input)?).into();
    println!("input: {:#}", expr);
    expr.eval()?;
    println!("output: {:#}", expr);
    Ok(())
}

fn main() {
    match run(Args::parse()) {
        Ok(_) => {
            exit(0);
        }
        Err(err) => {
            println!("{}", err);
            exit(1);
        }
    }
}
