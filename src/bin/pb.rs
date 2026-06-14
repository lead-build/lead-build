use clap::Parser;
use lead_build::{Expr, LangContext, Result, Value, ninjawriter::NinjaFile};
use std::{path::PathBuf, process::exit};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Root description file
    #[arg(short, long)]
    input: PathBuf,
}

fn run(args: Args) -> Result<()> {
    let mut ctx: LangContext = LangContext::new();
    let main_file = ctx.virtualize_path("root", &args.input)?;
    let expr: Expr<Value> = ctx.include(main_file)?;
    if let Value::Build(build) = expr.value()? {
        let mut ninja_file = NinjaFile::new();
        build.populate_ninja_file(&mut ninja_file);
        print!("{}", ninja_file);
    } else {
        println!("expceted top level to be a build, got {}", expr);
    }
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
