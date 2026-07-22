use clap::{ArgAction, Parser};
use lead_build::{
    Expr, LangContext, Result, Value, add_expr_to_ninjafile,
    lang::{Error, ErrorType, ExprSet, ExprType},
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
    #[arg(short = 'C', value_name = "DIR")]
    cd: Option<PathBuf>,

    /// Add paths to the top level args object, which is accessible as file
    /// arguments to the main pbb file.
    #[arg(
        short = 'P',
        long = "path",
        value_name = "NAME=PATH",
        value_parser = parse_virt_path_mapping,
        action = ArgAction::Append
    )]
    paths: Vec<(String, VirtPath)>,

    /// Print evaluated output instead of generating build.ninja file
    #[arg(short = 'E', long)]
    eval: bool,
}

fn parse_virt_path_mapping(s: &str) -> std::result::Result<(String, VirtPath), String> {
    let (name, path) = s.split_once('=').ok_or("expected NAME=PATH".to_string())?;

    Ok((
        name.to_owned(),
        VirtPath::from_dir(&PathBuf::from(path), name),
    ))
}

fn gen_args_object(args: Vec<(String, VirtPath)>) -> Expr<Value, VirtPath> {
    let mut args_map = ExprSet::new();
    for (name, path) in args {
        args_map.insert(name, ExprType::Value(Value::Path(path)).builtin());
    }
    ExprType::Object(args_map).builtin()
}

fn run(args: Args) -> Result<(), VirtPath> {
    let ctx: LangContext = LangContext::new();

    if let Some(dir) = args.cd {
        set_current_dir(&dir).map_err(|e| {
            Error::new(
                ErrorType::Custom,
                format!("Error changing directory: {}\n\n{}", dir.display(), e),
            )
        })?;
    }

    let input = args.input.unwrap_or_else(|| PathBuf::from("main.pbb"));
    let output = args.output.unwrap_or_else(|| PathBuf::from("build.ninja"));
    let main_file = VirtPath::from_file(&input, "root");
    let args_obj = gen_args_object(args.paths);
    let expr: Expr<Value, VirtPath> = ctx.include(main_file, Some(args_obj))?;

    if args.eval {
        expr.eval()?;
        println!("{}", expr);
        Ok(())
    } else {
        let mut ninja_file = NinjaFile::new();

        add_expr_to_ninjafile(&expr, &mut ninja_file)?;

        let errors = ninja_file.validate();
        if !errors.is_empty() {
            return Err(Error::new(
                ErrorType::Custom,
                format!(
                    "Error generating {}:\n  {}",
                    output.display(),
                    errors.join("\n  ")
                ),
            ));
        }

        let output_file =
            File::create(output).map_err(|e| Error::new(ErrorType::Custom, e.to_string()))?;
        let mut writer = BufWriter::new(&output_file);
        write!(writer, "{}", ninja_file)
            .or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;

        Ok(())
    }
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
