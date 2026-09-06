use clap::Parser;
use lead_build::pblang::{self, fmt::format_source};
use std::{fs, path::PathBuf, process::exit};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The .pbb file to format
    file: PathBuf,
}

fn run(args: Args) -> Result<(), String> {
    let source = fs::read_to_string(&args.file)
        .map_err(|e| format!("Error reading {}: {}", args.file.display(), e))?;

    let tree = pblang::parse(&source)
        .map_err(|e| format!("Error parsing {}: {}", args.file.display(), e))?;

    print!("{}", format_source(&tree));

    Ok(())
}

fn main() {
    let args = Args::parse();

    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
