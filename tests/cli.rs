use assert_cmd::Command;

#[test]
fn phpvm_help_succeeds() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn phpvm_versions_without_runtimes() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("versions")
        .assert()
        .success();
}
