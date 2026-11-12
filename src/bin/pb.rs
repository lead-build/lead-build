use clap::Parser;
use lead_build::{
    Expr, LangContext, Value, add_expr_to_ninjafile_all,
    cli::pb::Args,
    pbbuild::{NinjaFile, VirtPath},
    pbexpr::{Error, ErrorList, ErrorType, ExprSet, ExprType, RenderOptions},
    strkey::StrKey,
};
use std::{
    env::set_current_dir,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process::exit,
};

fn gen_args_object(args: Vec<(String, VirtPath)>) -> Expr<Value, VirtPath> {
    let mut args_map = ExprSet::new();
    for (name, path) in args {
        args_map.insert(
            StrKey::from(&name),
            ExprType::Value(Value::path(path)).builtin(),
        );
    }
    ExprType::Object(args_map).builtin()
}

/// Evaluate the description and write the ninja file, or print the result.
/// Errors from evaluating the parts of the result are collected, so that a
/// run reports every independent mistake; an error before evaluation (a file
/// not found, a syntax error) is reported alone.
fn run(args: Args) -> std::result::Result<(), ErrorList<VirtPath>> {
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
        let errors = expr.eval_all();
        if !errors.is_empty() {
            return Err(errors);
        }
        println!("{}", expr);
        Ok(())
    } else {
        let mut ninja_file = NinjaFile::new();

        let errors = add_expr_to_ninjafile_all(&expr, &mut ninja_file);
        if !errors.is_empty() {
            return Err(errors);
        }

        let errors = ninja_file.validate();
        if !errors.is_empty() {
            return Err(Error::new(
                ErrorType::Custom,
                format!(
                    "Error generating {}:\n  {}",
                    output.display(),
                    errors.join("\n  ")
                ),
            )
            .into());
        }

        let output_file =
            File::create(output).map_err(|e| Error::new(ErrorType::Custom, e.to_string()))?;
        let mut writer = BufWriter::new(&output_file);
        write!(writer, "{}", ninja_file)
            .or_else(|e| Err(Error::new(ErrorType::Custom, e.to_string())))?;
        ninja_file.log_summary();

        Ok(())
    }
}

fn setup_log(verbose: u8) {
    let log_level = match verbose {
        0 => log::LevelFilter::Warn,  // Default
        1 => log::LevelFilter::Info,  // -v
        2 => log::LevelFilter::Debug, // -vv
        _ => log::LevelFilter::Trace, // -vvv or more
    };

    env_logger::Builder::new().filter_level(log_level).init();
}

fn main() {
    let args = Args::parse();
    setup_log(args.verbose);
    let render_options = RenderOptions {
        color: std::io::IsTerminal::is_terminal(&std::io::stderr()),
        full_trace: args.trace,
    };

    match run(args) {
        Ok(_) => {
            exit(0);
        }
        Err(errors) => {
            eprint!("{}", errors.render(&render_options));
            exit(1);
        }
    }
}
