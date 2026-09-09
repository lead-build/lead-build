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
    /// The .pbb file(s) to format (required unless --stdin is given).
    /// Multiple files are only allowed together with --in-place.
    #[arg(required_unless_present = "stdin", num_args = 1..)]
    file: Vec<PathBuf>,

    /// Read the source from stdin instead of a file
    #[arg(short, long, conflicts_with = "file")]
    stdin: bool,

    /// Only check that the file parses, without reformatting it
    #[arg(short, long)]
    lint: bool,

    /// Format the file in place instead of printing to stdout
    #[arg(short, long, alias = "inplace", conflicts_with_all = ["stdin", "lint"])]
    in_place: bool,
}

fn run_file(file: Option<&PathBuf>, args: &Args) -> Result<(), String> {
    let (source, name) = if args.stdin {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("Error reading stdin: {e}"))?;
        (source, "<stdin>".to_string())
    } else {
        let file = file.expect("clap requires `file` unless --stdin is set");
        let source = fs::read_to_string(file)
            .map_err(|e| format!("Error reading {}: {}", file.display(), e))?;
        (source, file.display().to_string())
    };

    let parsed = pblang::parse(&source);
    if !parsed.errors.is_empty() || parsed.tree.is_err() {
        let mut lines: Vec<String> = parsed.errors.iter().map(|r| r.error.to_string()).collect();
        if let Err(fatal) = &parsed.tree {
            lines.push(fatal.to_string());
        }
        let numbered = lines
            .iter()
            .enumerate()
            .map(|(i, l)| format!("  {}. {}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("Error parsing {name}:\n{numbered}"));
    }
    let tree = parsed.tree.expect("checked above");

    if args.lint {
        println!("{}", tree.text());
    } else {
        let formatted = format_tree(&tree).text().to_string();
        if args.in_place {
            let file = file.expect("clap requires `file` unless --stdin is set");
            fs::write(file, &formatted)
                .map_err(|e| format!("Error writing {}: {}", file.display(), e))?;
        } else {
            println!("{}", formatted);
        }
    }

    Ok(())
}

fn main() {
    let args = Args::parse();

    if !args.in_place && args.file.len() > 1 {
        eprintln!("Multiple files are only allowed together with --in-place");
        exit(1);
    }

    let mut had_error = false;

    if args.stdin {
        if let Err(err) = run_file(None, &args) {
            eprintln!("{}", err);
            had_error = true;
        }
    } else {
        for file in &args.file {
            if let Err(err) = run_file(Some(file), &args) {
                eprintln!("{}", err);
                had_error = true;
            }
        }
    }

    if had_error {
        exit(1);
    }
}
