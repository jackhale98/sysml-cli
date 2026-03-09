/// End-to-end CLI integration tests.
///
/// These test the actual binary with real SysML fixture files.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("sysml-cli").unwrap()
}

fn fixture(name: &str) -> String {
    format!("../../test/fixtures/{}", name)
}

// ========================================================================
// lint
// ========================================================================

#[test]
fn lint_valid_file() {
    cmd()
        .args(["lint", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stderr(predicate::str::contains("Found"));
}

#[test]
fn lint_missing_file() {
    cmd()
        .args(["lint", "nonexistent.sysml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn lint_json_format() {
    cmd()
        .args(["lint", "-f", "json", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"code\""));
}

#[test]
fn lint_disable_check() {
    cmd()
        .args(["lint", "-d", "unused", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

// ========================================================================
// list
// ========================================================================

#[test]
fn list_all_elements() {
    cmd()
        .args(["list", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Vehicle"))
        .stdout(predicate::str::contains("Engine"));
}

#[test]
fn list_filter_by_kind() {
    cmd()
        .args(["list", "--kind", "parts", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("part def"));
}

#[test]
fn list_filter_by_name() {
    cmd()
        .args(["list", "--name", "Vehicle", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Vehicle"));
}

#[test]
fn list_json_output() {
    cmd()
        .args(["list", "-f", "json", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"kind\""));
}

// ========================================================================
// show
// ========================================================================

#[test]
fn show_element() {
    cmd()
        .args(["show", &fixture("simple-vehicle.sysml"), "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Vehicle"));
}

#[test]
fn show_missing_element() {
    cmd()
        .args(["show", &fixture("simple-vehicle.sysml"), "NonExistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ========================================================================
// diagram
// ========================================================================

#[test]
fn diagram_bdd_mermaid() {
    cmd()
        .args(["diagram", "-t", "bdd", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("classDiagram"));
}

#[test]
fn diagram_bdd_plantuml() {
    cmd()
        .args([
            "diagram", "-t", "bdd", "-o", "plantuml",
            &fixture("simple-vehicle.sysml"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@startuml"));
}

#[test]
fn diagram_bdd_dot() {
    cmd()
        .args([
            "diagram", "-t", "bdd", "-o", "dot",
            &fixture("simple-vehicle.sysml"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("digraph"));
}

#[test]
fn diagram_bdd_d2() {
    cmd()
        .args([
            "diagram", "-t", "bdd", "-o", "d2",
            &fixture("simple-vehicle.sysml"),
        ])
        .assert()
        .success();
}

#[test]
fn diagram_req() {
    cmd()
        .args(["diagram", "-t", "req", &fixture("RequirementTest.sysml")])
        .assert()
        .success();
}

#[test]
fn diagram_stm() {
    cmd()
        .args(["diagram", "-t", "stm", &fixture("flashlight.sysml")])
        .assert()
        .success();
}

#[test]
fn diagram_invalid_type() {
    cmd()
        .args(["diagram", "-t", "xyz", &fixture("simple-vehicle.sysml")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown diagram type"));
}

// ========================================================================
// simulate
// ========================================================================

#[test]
fn simulate_list() {
    cmd()
        .args(["simulate", "list", &fixture("flashlight.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("state"));
}

#[test]
fn simulate_eval() {
    cmd()
        .args([
            "simulate", "eval",
            &fixture("simulation.sysml"),
        ])
        .assert()
        .success();
}

#[test]
fn simulate_state_machine_with_events() {
    // After consuming both events the machine deadlocks (no more events),
    // so exit code is 1 — but the trace is still produced correctly.
    cmd()
        .args([
            "simulate", "state-machine",
            &fixture("flashlight.sysml"),
            "-n", "FlashlightStates",
            "-e", "switchOn,switchOff",
        ])
        .assert()
        .stdout(predicate::str::contains("Step 0"))
        .stdout(predicate::str::contains("Step 1"))
        .stdout(predicate::str::contains("off"));
}

// ========================================================================
// trace
// ========================================================================

#[test]
fn trace_requirements() {
    cmd()
        .args(["trace", &fixture("RequirementTest.sysml")])
        .assert()
        .success();
}

// ========================================================================
// new
// ========================================================================

#[test]
fn new_part_def_to_stdout() {
    cmd()
        .args(["new", "part-def", "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("part def Vehicle;"));
}

#[test]
fn new_with_members() {
    cmd()
        .args([
            "new", "part-def", "Vehicle",
            "-m", "part engine:Engine",
            "--doc", "A vehicle",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("part engine : Engine;"))
        .stdout(predicate::str::contains("doc /* A vehicle */"));
}

#[test]
fn new_view_def_with_expose() {
    cmd()
        .args([
            "new", "view-def", "PartsView",
            "--expose", "Vehicle::*",
            "--filter", "part",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("expose Vehicle::*;"))
        .stdout(predicate::str::contains("filter @type istype part;"));
}

#[test]
fn new_unknown_kind() {
    cmd()
        .args(["new", "bogus-thing", "Foo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown element kind"));
}

// ========================================================================
// edit
// ========================================================================

#[test]
fn edit_rename_dry_run() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.sysml");
    fs::write(&file, "part def Vehicle;\npart def Engine;\n").unwrap();

    cmd()
        .args([
            "edit", "rename",
            file.to_str().unwrap(),
            "Engine", "Motor",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("-part def Engine;"))
        .stdout(predicate::str::contains("+part def Motor;"));
}

#[test]
fn edit_remove_dry_run() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.sysml");
    fs::write(&file, "part def Vehicle;\npart def Engine;\n").unwrap();

    cmd()
        .args([
            "edit", "remove",
            file.to_str().unwrap(),
            "Engine",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("-part def Engine;"));
}

// ========================================================================
// fmt
// ========================================================================

#[test]
fn fmt_check_formatted() {
    cmd()
        .args(["fmt", "--check", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

#[test]
fn fmt_diff_unformatted() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bad.sysml");
    fs::write(&file, "part def Vehicle {\npart engine : Engine;\n}\n").unwrap();

    cmd()
        .args(["fmt", "--diff", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("+    part engine : Engine;"));
}

// ========================================================================
// export
// ========================================================================

#[test]
fn export_list() {
    cmd()
        .args(["export", "list", &fixture("fmi-vehicle.sysml")])
        .assert()
        .success();
}

// ========================================================================
// general
// ========================================================================

#[test]
fn help_flag() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("SysML v2"))
        .stdout(predicate::str::contains("GETTING STARTED"));
}

#[test]
fn version_flag() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("sysml-cli"));
}
