use clap::Parser;
use lead_build::{
    Expr, LangContext, Result, Value,
    lang::{Error, ErrorType},
    ninjawriter::NinjaFile,
    path::VirtPath,
};
use std::{
    env::set_current_dir,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process::exit,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Root description file
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Root description file
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Change directory before invoking command
    #[arg(short = 'C', id = "PATH")]
    cd: Option<PathBuf>,
}

fn run(args: Args) -> Result<(), VirtPath> {
    let ctx: LangContext = LangContext::new();

    if let Some(dir) = args.cd {
        set_current_dir(&dir).or_else(|e| {
            Err(Error::new(
                ErrorType::Custom,
                format!(
                    "Error changing directory: {}\n\n{}",
                    dir.display(),
                    e.to_string()
                ),
            ))
        })?;
    }

    let input = args.input.unwrap_or_else(|| PathBuf::from("main.pbb"));
    let output = args.output.unwrap_or_else(|| PathBuf::from("build.ninja"));
    let main_file = VirtPath::virtualize(&input, "root");
    let expr: Expr<Value, VirtPath> = ctx.include(main_file)?;

    if let Value::Build(build) = expr.value()? {
        let mut ninja_file = NinjaFile::new();
        build.populate_ninja_file(&mut ninja_file);

        let output_file =
            File::create(output).or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;
        let mut writer = BufWriter::new(&output_file);

        write!(&mut writer, "{}", ninja_file)
            .or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;
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
            eprintln!("{}", err);
            exit(1);
        }
    }
}
