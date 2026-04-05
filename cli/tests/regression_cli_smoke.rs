//! CLI 烟测：与本包 `stormclaw` 二进制同进程构建，Cargo 会设置 `CARGO_BIN_EXE_stormclaw`。

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn stormclaw_help_exits_zero() {
    Command::new(cargo_bin("stormclaw"))
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("stormclaw").or(predicate::str::contains("Stormclaw")));
}

#[test]
fn stormclaw_version_or_help_shows_usage() {
    let out = Command::new(cargo_bin("stormclaw"))
        .arg("--help")
        .unwrap();
    assert!(out.status.success());
}
