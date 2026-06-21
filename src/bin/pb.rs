use clap::Parser;
use lead_build::{
    Expr, LangContext, Result, Value,
    lang::{Error, ErrorType, ExprStorage, ExprType},
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

fn add_expr_to_ninjafile(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
) -> Result<(), VirtPath> {
    expr.resolve()?;
    match &*expr.inner_ref() {
        ExprStorage {
            tok: ExprType::Value(Value::Build(build)),
            ..
        } => {
            build.populate_ninja_file(ninja_file);
            Ok(())
        }
        ExprStorage {
            tok: ExprType::List(list),
            ..
        } => {
            for item in list.iter() {
                add_expr_to_ninjafile(item, ninja_file)?;
            }
            Ok(())
        }
        ExprStorage { tok: _, loc } => {
            Err(Error::new(ErrorType::Custom, "Not a valid build definition").reref(loc))
        }
    }
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

    let mut ninja_file = NinjaFile::new();

    add_expr_to_ninjafile(&expr, &mut ninja_file)?;

    let output_file =
        File::create(output).or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;
    let mut writer = BufWriter::new(&output_file);
    write!(writer, "{}", ninja_file)
        .or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;

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
