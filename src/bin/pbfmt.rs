use clap::Parser;
use lead_build::{
    cli::pbfmt::Args,
    pblang::{format_tree, parse},
};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    process::exit,
};

fn run_file(file: Option<&PathBuf>, args: &Args) -> Result<(), String> {
    let (source, name) = if args.stdin {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("[io error]: Failed to read standard input: {e}"))?;
        (source, "<stdin>".to_string())
    } else {
        let file = file.expect(
            "[internal error]: Missing `file` argument without `--stdin`",
        );
        let source = fs::read_to_string(file)
            .map_err(|e| format!("[io error]: Failed to read `{}`: {}", file.display(), e))?;
        (source, file.display().to_string())
    };

    let parsed = parse(&source);
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
        return Err(format!(
            "[parse error]: Failed to parse `{name}`:\n{numbered}"
        ));
    }
    let tree = parsed.tree.expect(
        "[internal error]: Missing parse tree without parse error",
    );

    if args.lint {
        print!("{}", tree.text());
    } else {
        let formatted = format_tree(&tree).text().to_string();
        if args.in_place {
            let file = file.expect(
                "[internal error]: Missing `file` argument without `--stdin`",
            );
            fs::write(file, &formatted)
                .map_err(|e| format!("[io error]: Failed to write `{}`: {}", file.display(), e))?;
        } else {
            println!("{}", formatted);
        }
    }

    Ok(())
}

fn main() {
    let args = Args::parse();

    if !args.in_place && args.file.len() > 1 {
        eprintln!("[usage error]: Multiple files only allowed with `--in-place`");
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
