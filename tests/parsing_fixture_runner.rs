use lead_build::pbexpr::ErrorType;
use lead_build::{LangContext, pbbuild::VirtPath};
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
    ErrType,
    ErrCustom,
    ErrDebug,
}

#[derive(Debug, Deserialize)]
struct CaseExpect {
    kind: ExpectKind,
    expect_value: Option<String>,
    error_contains: Option<String>,
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
            | (ErrorType::Type, ExpectKind::ErrType)
            | (ErrorType::Custom, ExpectKind::ErrCustom)
            | (ErrorType::Debug, ExpectKind::ErrDebug)
    )
}

/// The outcome of a case, rendered on the case thread since neither values
/// nor errors can be sent between threads.
type CaseResult = Result<String, (ErrorType, String)>;

/// Evaluate one case on its own thread. A case that does not finish within
/// `CASE_TIMEOUT` fails the test instead of hanging the whole suite; the
/// thread itself is left behind and dies with the test process.
fn run_case_with_timeout(case_name: &str, main_file: &Path) -> CaseResult {
    let main_file = main_file.to_path_buf();
    let thread_name = format!("case-{case_name}");
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            let main_vpath = VirtPath::from_file(&main_file, thread_name);
            let ctx = LangContext::new();
            let result = (|| {
                let expr = ctx.include(main_vpath, None)?;
                expr.eval()?;
                expr.value()
            })();
            // The receiver is gone if the case timed out; nothing to report then
            let _ = tx.send(
                result
                    .map(|value| format!("{value}"))
                    .map_err(|err| (err.typ, format!("{err}"))),
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

#[test]
fn run_parsing_fixture_cases() {
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parsing_cases");
    for case_dir in list_case_dirs(&fixture_root) {
        let case_name = case_dir.file_name().unwrap().to_string_lossy().to_string();
        let expect = parse_expect(&case_dir.join("expect.toml"));
        let main_file = case_dir.join("main.pbb");

        let result = run_case_with_timeout(&case_name, &main_file);

        match expect.kind {
            ExpectKind::Ok => {
                let rendered = result.unwrap_or_else(|(_, e)| {
                    panic!("case '{}' expected success, got error:\n{}", case_name, e)
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
                let (err_typ, rendered) =
                    result.expect_err(&format!("case '{}' expected error, got success", case_name));
                assert!(
                    matches_kind(&err_typ, expected_kind),
                    "case '{}' expected error kind {:?}, got {:?} ({})",
                    case_name,
                    expected_kind,
                    err_typ,
                    rendered
                );
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
