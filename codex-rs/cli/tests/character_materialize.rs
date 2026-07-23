use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run_resolve(home: &TempDir, name: &str, materialize: bool) -> assert_cmd::assert::Assert {
    let mut command =
        Command::new(codex_utils_cargo_bin::cargo_bin("clanker").expect("clanker binary"));
    command
        .env("CODEX_HOME", home.path())
        .args(["character", "resolve", name, "--json"]);
    if materialize {
        command.arg("--materialize-builtin");
    }
    command.assert()
}

fn json_output(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("schema JSON output")
}

fn entry_names(path: &std::path::Path) -> Vec<String> {
    let mut names = fs::read_dir(path)
        .expect("directory exists")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn resolve_without_materialization_is_read_only() {
    let home = TempDir::new().expect("CODEX_HOME");
    let assertion = run_resolve(&home, "chloe", false).failure();
    let output = json_output(&assertion);

    assert_eq!(output["schemaVersion"], 1);
    assert_eq!(output["ok"], false);
    assert_eq!(output["errors"][0]["code"], "not_found");
    assert!(!home.path().join("characters").exists());
}

#[test]
fn materialize_builtin_resolves_chloe_on_first_use() {
    let home = TempDir::new().expect("CODEX_HOME");
    let assertion = run_resolve(&home, "cleo", true).success();
    let output = json_output(&assertion);

    assert_eq!(output["schemaVersion"], 1);
    assert_eq!(output["ok"], true);
    assert_eq!(output["id"], "chloe");
    assert!(
        home.path()
            .join("characters/chloe/character.json")
            .is_file()
    );
    assert!(!home.path().join("characters/clanker").exists());
}

#[test]
fn materialize_rusty_alias_creates_only_clanker() {
    let home = TempDir::new().expect("CODEX_HOME");
    let assertion = run_resolve(&home, "rusty", true).success();
    let output = json_output(&assertion);

    assert_eq!(output["id"], "clanker");
    assert!(
        home.path()
            .join("characters/clanker/character.json")
            .is_file()
    );
    assert!(!home.path().join("characters/chloe").exists());
}

#[test]
fn materialize_custom_name_does_not_create_builtins() {
    let home = TempDir::new().expect("CODEX_HOME");
    let assertion = run_resolve(&home, "custom", true).failure();
    let output = json_output(&assertion);

    assert_eq!(output["schemaVersion"], 1);
    assert_eq!(output["ok"], false);
    assert_eq!(output["errors"][0]["code"], "not_found");
    assert!(!home.path().join("characters").exists());
}

#[test]
fn materialize_ignores_unrelated_partial_builtin_directory() {
    let home = TempDir::new().expect("CODEX_HOME");
    let partial = home.path().join("characters/clanker");
    fs::create_dir_all(&partial).expect("partial directory");
    fs::write(partial.join("sentinel"), b"untouched").expect("sentinel");
    let before_entries = entry_names(&partial);
    let assertion = run_resolve(&home, "chloe", true).success();
    let output = json_output(&assertion);

    assert_eq!(output["id"], "chloe");
    assert!(
        home.path()
            .join("characters/chloe/character.json")
            .is_file()
    );
    assert!(
        !home
            .path()
            .join("characters/clanker/character.json")
            .exists()
    );
    assert_eq!(entry_names(&partial), before_entries);
    assert_eq!(fs::read(partial.join("sentinel")).unwrap(), b"untouched");
}

#[test]
fn materialize_selected_partial_builtin_returns_json_failure() {
    let home = TempDir::new().expect("CODEX_HOME");
    let partial = home.path().join("characters/chloe");
    fs::create_dir_all(&partial).expect("partial directory");
    fs::write(partial.join("sentinel"), b"untouched").expect("sentinel");
    let before_entries = entry_names(&partial);
    let assertion = run_resolve(&home, "chloe", true).failure();
    let output = json_output(&assertion);

    assert_eq!(output["schemaVersion"], 1);
    assert_eq!(output["ok"], false);
    assert_eq!(output["errors"][0]["code"], "invalid_manifest");
    assert!(!home.path().join("characters/chloe/character.json").exists());
    assert_eq!(entry_names(&partial), before_entries);
    assert_eq!(fs::read(partial.join("sentinel")).unwrap(), b"untouched");
}
