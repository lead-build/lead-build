use lead_build::pbexpr::{ErrorType, ExprSet, ExprType, RenderOptions};
use lead_build::strkey::StrKey;
use lead_build::{LangContext, Value, pbbuild::VirtPath};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

const CASE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectKind {
    Ok,
    ErrParse,
    ErrEval,
    ErrScope,
    ErrNoValue,
    ErrType,
    ErrCustom,
    ErrDebug,
}

#[derive(Debug, Deserialize)]
struct CaseExpect {
    kind: ExpectKind,
    expect_value: Option<String>,
    error_contains: Option<String>,
    /// The whole rendered error must equal `expect_error.txt` in the case
    /// directory. Run with `UPDATE_EXPECT=1` to write that file.
    #[serde(default)]
    golden: bool,
}

fn parse_expect(path: &Path) -> CaseExpect {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read expect file {}: {e}", path.display()));

    toml::from_str::<CaseExpect>(&text).unwrap_or_else(|e| {
        panic!(
            "failed to parse expect file {} as TOML: {e}",
            path.display()
        )
    })
}

fn list_case_dirs(root: &Path) -> Vec<PathBuf> {
    let mut cases = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("failed to read fixture root {}: {e}", root.display()))
        .map(|res| res.unwrap().path())
        .filter(|path| path.is_dir())
        // A directory starting with `_` holds files shared between cases
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('_'))
        })
        .collect::<Vec<_>>();
    cases.sort();
    cases
}

fn matches_kind(actual: &ErrorType, expected: ExpectKind) -> bool {
    matches!(
        (actual, expected),
        (_, ExpectKind::Ok)
            | (ErrorType::Parse, ExpectKind::ErrParse)
            | (ErrorType::Eval, ExpectKind::ErrEval)
            | (ErrorType::Scope, ExpectKind::ErrScope)
            | (ErrorType::NoValue, ExpectKind::ErrNoValue)
            | (ErrorType::Type, ExpectKind::ErrType)
            | (ErrorType::Custom, ExpectKind::ErrCustom)
            | (ErrorType::Debug, ExpectKind::ErrDebug)
    )
}

/// An error rendered on the case thread, since neither values nor errors
/// can be sent between threads.
struct CaseError {
    typ: ErrorType,
    /// `Display` of the error, without source text
    plain: String,
    /// What the command line prints, without color
    rich: String,
}

type CaseResult = Result<String, CaseError>;

/// Evaluate one case on its own thread. A case that does not finish within
/// `CASE_TIMEOUT` fails the test instead of hanging the whole suite; the
/// thread itself is left behind and dies with the test process.
///
/// Every case gets the fixture root as the path argument `fixtures`, to
/// include files shared between cases.
fn run_case_with_timeout(case_name: &str, main_file: &Path, fixture_root: &Path) -> CaseResult {
    let main_file = main_file.to_path_buf();
    let fixture_root = fixture_root.to_path_buf();
    let thread_name = format!("case-{case_name}");
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            let main_vpath = VirtPath::from_file(&main_file, thread_name);
            let mut args = ExprSet::new();
            args.insert(
                StrKey::from("fixtures"),
                ExprType::Value(Value::path(VirtPath::from_dir(&fixture_root, "fixtures")))
                    .builtin(),
            );
            let ctx = LangContext::new();
            let result = (|| {
                let expr = ctx.include(main_vpath, Some(ExprType::Object(args).builtin()))?;
                expr.eval()?;
                expr.value()
            })();
            // The receiver is gone if the case timed out; nothing to report then
            let _ = tx.send(
                result
                    .map(|value| format!("{value}"))
                    .map_err(|err| CaseError {
                        typ: err.typ,
                        plain: format!("{err}"),
                        rich: err.render(&RenderOptions {
                            color: false,
                            full_trace: false,
                        }),
                    }),
            );
        })
        .expect("failed to spawn case thread");
    match rx.recv_timeout(CASE_TIMEOUT) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => {
            panic!("case '{case_name}' did not terminate within {CASE_TIMEOUT:?}")
        }
        Err(RecvTimeoutError::Disconnected) => panic!("case '{case_name}' panicked"),
    }
}

/// Compare the whole rendered error with `expect_error.txt`, or write that
/// file when `UPDATE_EXPECT` is set. The fixture root is replaced by `$DIR`
/// so the file does not depend on where the repository is checked out.
fn check_golden(case_name: &str, case_dir: &Path, fixture_root: &Path, rich: &str) {
    let actual = rich.replace(&fixture_root.display().to_string(), "$DIR");
    let golden_file = case_dir.join("expect_error.txt");
    if std::env::var_os("UPDATE_EXPECT").is_some() {
        fs::write(&golden_file, &actual)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", golden_file.display()));
        return;
    }
    let expected = fs::read_to_string(&golden_file).unwrap_or_else(|e| {
        panic!(
            "case '{case_name}': failed to read {} ({e}); run with UPDATE_EXPECT=1 to create it",
            golden_file.display()
        )
    });
    assert!(
        actual == expected,
        "case '{case_name}': the rendered error differs from expect_error.txt \
         (UPDATE_EXPECT=1 rewrites it)\n--- expected\n{expected}\n--- actual\n{actual}"
    );
}

#[test]
fn run_parsing_fixture_cases() {
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parsing_cases");
    for case_dir in list_case_dirs(&fixture_root) {
        let case_name = case_dir.file_name().unwrap().to_string_lossy().to_string();
        let expect = parse_expect(&case_dir.join("expect.toml"));
        let main_file = case_dir.join("main.pbb");

        let result = run_case_with_timeout(&case_name, &main_file, &fixture_root);

        match expect.kind {
            ExpectKind::Ok => {
                let rendered = result.unwrap_or_else(|e| {
                    panic!(
                        "case '{}' expected success, got error:\n{}",
                        case_name, e.rich
                    )
                });
                if let Some(expected) = expect.expect_value.as_ref() {
                    assert_eq!(
                        rendered, *expected,
                        "case '{}' expected value '{}', got '{}'",
                        case_name, expected, rendered
                    );
                }
            }
            expected_kind => {
                let err = result
                    .err()
                    .unwrap_or_else(|| panic!("case '{}' expected error, got success", case_name));
                let rendered = &err.plain;
                assert!(
                    matches_kind(&err.typ, expected_kind),
                    "case '{}' expected error kind {:?}, got {:?} ({})",
                    case_name,
                    expected_kind,
                    err.typ,
                    rendered
                );
                if expect.golden {
                    check_golden(&case_name, &case_dir, &fixture_root, &err.rich);
                }
                if let Some(needle) = expect.error_contains.as_ref() {
                    // Checked against the full rendered error, not just
                    // `err.msg`, so `error_contains` can also assert on the
                    // backtrace (e.g. a `file:line:col` location).
                    assert!(
                        rendered.contains(needle),
                        "case '{}' expected error output to contain '{}', got '{}'",
                        case_name,
                        needle,
                        rendered
                    );
                }
            }
        }
    }
}
