use clap::Parser;
use lead_build::pblang::{self, fmt::format_tree};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    process::exit,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The .pbb file to format (required unless --stdin is given)
    #[arg(required_unless_present = "stdin")]
    file: Option<PathBuf>,

    /// Read the source from stdin instead of a file
    #[arg(short, long, conflicts_with = "file")]
    stdin: bool,

    /// Only check that the file parses, without reformatting it
    #[arg(short, long)]
    lint: bool,
}

fn run(args: Args) -> Result<(), String> {
    let (source, name) = if args.stdin {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("Error reading stdin: {e}"))?;
        (source, "<stdin>".to_string())
    } else {
        let file = args
            .file
            .as_ref()
            .expect("clap requires `file` unless --stdin is set");
        let source = fs::read_to_string(file)
            .map_err(|e| format!("Error reading {}: {}", file.display(), e))?;
        (source, file.display().to_string())
    };

    let tree = pblang::parse(&source).map_err(|e| format!("Error parsing {name}: {e}"))?;

    if args.lint {
        println!("{}", tree.text());
    } else {
        println!("{}", format_tree(&tree).text());
    }

    Ok(())
}

fn main() {
    let args = Args::parse();

    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
