use clap::{ArgAction, Parser};
use std::path::PathBuf;

use crate::pbbuild::VirtPath;

#[derive(Parser, Debug)]
#[command(
    name = "pb",
    version,
    about = "Generate Ninja build files from a build description",
    long_about = None
)]
pub struct Args {
    /// Root description file (default: main.pbb)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Output Ninja file (default: build.ninja)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Change directory before invoking command
    #[arg(short = 'C', value_name = "DIR")]
    pub cd: Option<PathBuf>,

    /// Add paths to the top level args object, which is accessible as file
    /// arguments to the main pbb file.
    #[arg(
        short = 'P',
        long = "path",
        value_name = "NAME=PATH",
        value_parser = parse_virt_path_mapping,
        action = ArgAction::Append
    )]
    pub paths: Vec<(String, VirtPath)>,

    /// Print evaluated output instead of generating build.ninja file
    #[arg(short = 'E', long)]
    pub eval: bool,

    /// Show the whole trace of an error, however long
    #[arg(long)]
    pub trace: bool,

    /// Set verbosity level. Pass many times for more log output (up to three)
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,
}

fn parse_virt_path_mapping(s: &str) -> std::result::Result<(String, VirtPath), String> {
    let (name, path) = s.split_once('=').ok_or("expected NAME=PATH".to_string())?;

    Ok((
        name.to_owned(),
        VirtPath::from_dir(&PathBuf::from(path), name),
    ))
}
