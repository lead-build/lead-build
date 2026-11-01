//! Generate man pages for the binaries, from their command line arguments.
//!
//! Usage: cargo run --example mangen -- <output dir>

use clap::CommandFactory;
use clap_mangen::Man;
use lead_build::cli;
use std::{env, fs, io, path::PathBuf, process::exit};

fn main() -> io::Result<()> {
    let Some(out_dir) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: mangen <output dir>");
        exit(2);
    };
    fs::create_dir_all(&out_dir)?;

    for cmd in [
        cli::pb::Args::command(),
        cli::pbfmt::Args::command(),
        cli::pbls::Args::command(),
    ] {
        let path = out_dir.join(format!("{}.1", cmd.get_name()));
        let mut out = fs::File::create(&path)?;
        Man::new(cmd).render(&mut out)?;
        println!("{}", path.display());
    }
    Ok(())
}
