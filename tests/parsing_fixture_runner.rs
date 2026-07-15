use lead_build::LangContext;
use lead_build::lang::ErrorType;
use lead_build::path::VirtPath;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectKind {
    Ok,
    ErrParse,
    ErrEval,
    ErrType,
    ErrCustom,
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
    )
}

#[test]
fn run_parsing_fixture_cases() {
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parsing_cases");
    for case_dir in list_case_dirs(&fixture_root) {
        let case_name = case_dir.file_name().unwrap().to_string_lossy().to_string();
        let expect = parse_expect(&case_dir.join("expect.toml"));
        let main_file = case_dir.join("main.pbb");

        let main_vpath = VirtPath::from_file(&main_file, format!("case-{case_name}"));
        let ctx = LangContext::new();

        let result = (|| {
            let expr = ctx.include(main_vpath, None)?;
            expr.eval()?;
            expr.value()
        })();

        match expect.kind {
            ExpectKind::Ok => {
                let value = result.unwrap_or_else(|e| {
                    panic!("case '{}' expected success, got error:\n{}", case_name, e)
                });
                if let Some(expected) = expect.expect_value.as_ref() {
                    let rendered = format!("{}", value);
                    assert_eq!(
                        rendered, *expected,
                        "case '{}' expected value '{}', got '{}'",
                        case_name, expected, rendered
                    );
                }
            }
            expected_kind => {
                let err =
                    result.expect_err(&format!("case '{}' expected error, got success", case_name));
                assert!(
                    matches_kind(&err.typ, expected_kind),
                    "case '{}' expected error kind {:?}, got {:?} ({})",
                    case_name,
                    expected_kind,
                    err.typ,
                    err
                );
                if let Some(needle) = expect.error_contains.as_ref() {
                    assert!(
                        err.msg.contains(needle),
                        "case '{}' expected error message to contain '{}', got '{}'",
                        case_name,
                        needle,
                        err.msg
                    );
                }
            }
        }
    }
}
