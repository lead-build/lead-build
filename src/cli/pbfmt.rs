use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "pbfmt",
    version,
    about = "Format and lint build descriptions",
    long_about = None
)]
pub struct Args {
    /// The .pbb file(s) to format (required unless --stdin is given).
    /// Multiple files are only allowed together with --in-place.
    #[arg(required_unless_present = "stdin", num_args = 1..)]
    pub file: Vec<PathBuf>,

    /// Read the source from stdin instead of a file
    #[arg(short, long, conflicts_with = "file")]
    pub stdin: bool,

    /// Only check that the file parses, without reformatting it
    #[arg(short, long)]
    pub lint: bool,

    /// Format the file in place instead of printing to stdout
    #[arg(short, long, alias = "inplace", conflicts_with_all = ["stdin", "lint"])]
    pub in_place: bool,
}
