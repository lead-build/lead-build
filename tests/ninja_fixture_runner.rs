use lead_build::{LangContext, add_expr_to_ninjafile};
use lead_build::ninjawriter::NinjaFile;
use lead_build::path::VirtPath;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct NinjaExpect {
    #[serde(default, rename = "ninja_contains")]
    contains: Vec<String>,
    #[serde(default, rename = "ninja_not_contains")]
    not_contains: Vec<String>,
}

fn parse_expect(path: &Path) -> NinjaExpect {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read expect file {}: {e}", path.display()));

    toml::from_str::<NinjaExpect>(&text).unwrap_or_else(|e| {
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

#[test]
fn run_ninja_fixture_cases() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ninja_cases");

    for case_dir in list_case_dirs(&fixture_root) {
        let case_name = case_dir.file_name().unwrap().to_string_lossy().to_string();
        let expect = parse_expect(&case_dir.join("expect.toml"));
        let main_file = case_dir.join("main.pbb");

        let main_vpath = VirtPath::virtualize(&main_file, format!("ninja-case-{case_name}"));
        let ctx = LangContext::new();

        let expr = ctx.include(main_vpath).unwrap_or_else(|e| {
            panic!("ninja case '{}' include failed:\n{}", case_name, e)
        });

        let mut ninja_file = NinjaFile::new();
        add_expr_to_ninjafile(&expr, &mut ninja_file).unwrap_or_else(|e| {
            panic!("ninja case '{}' conversion failed:\n{}", case_name, e)
        });

        let errors = ninja_file.validate();
        assert!(
            errors.is_empty(),
            "ninja case '{}' generated invalid ninja file:\n{}",
            case_name,
            errors.join("\n")
        );

        let rendered = format!("{}", ninja_file);
        let case_dir_token = case_dir.display().to_string();

        for needle in expect.contains.iter() {
            let needle = needle.replace("{{CASE_DIR}}", case_dir_token.as_str());
            assert!(
                rendered.contains(&needle),
                "ninja case '{}' expected generated ninja to contain '{}', got:\n{}",
                case_name,
                needle,
                rendered
            );
        }

        for needle in expect.not_contains.iter() {
            let needle = needle.replace("{{CASE_DIR}}", case_dir_token.as_str());
            assert!(
                !rendered.contains(&needle),
                "ninja case '{}' expected generated ninja to not contain '{}', got:\n{}",
                case_name,
                needle,
                rendered
            );
        }
    }
}
