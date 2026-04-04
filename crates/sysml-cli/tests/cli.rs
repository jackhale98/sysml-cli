/// End-to-end CLI integration tests.
///
/// These test the actual binary with real SysML fixture files.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("sysml").unwrap()
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

// show --raw

#[test]
fn show_raw_prints_source() {
    cmd()
        .args(["show", "--raw", &fixture("simple-vehicle.sysml"), "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("part def Vehicle"))
        .stdout(predicate::str::contains("{"));
}

#[test]
fn show_raw_usage() {
    cmd()
        .args(["show", "--raw", &fixture("simple-vehicle.sysml"), "engine"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine"));
}

#[test]
fn show_raw_missing_element() {
    cmd()
        .args(["show", "--raw", &fixture("simple-vehicle.sysml"), "NotThere"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ========================================================================
// verify run (non-interactive — should fail without TTY)
// ========================================================================

#[cfg(feature = "verify")]
#[test]
fn verify_run_no_tty() {
    // verify run requires an interactive terminal; should fail gracefully in CI
    cmd()
        .args(["verify", "run", &fixture("simple-vehicle.sysml")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

// ========================================================================
// mfg start-lot (non-interactive — should fail without TTY)
// ========================================================================

#[cfg(feature = "mfg")]
#[test]
fn mfg_start_lot_no_tty() {
    cmd()
        .args(["mfg", "start-lot", &fixture("simple-vehicle.sysml")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

#[cfg(feature = "mfg")]
#[test]
fn mfg_step_missing_lot() {
    cmd()
        .args(["mfg", "step", "NONEXISTENT-LOT-ID"])
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
        .stderr(predicate::str::contains("invalid value"));
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
// add (replaces new + edit add)
// ========================================================================

#[test]
fn add_stdout_part_def() {
    cmd()
        .args(["add", "--stdout", "part-def", "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("part def Vehicle;"));
}

#[test]
fn add_stdout_with_members() {
    cmd()
        .args([
            "add", "--stdout", "part-def", "Vehicle",
            "-m", "part engine:Engine",
            "--doc", "A vehicle",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("part engine : Engine;"))
        .stdout(predicate::str::contains("doc /* A vehicle */"));
}

#[test]
fn add_stdout_view_def_with_expose() {
    cmd()
        .args([
            "add", "--stdout", "view-def", "PartsView",
            "--expose", "Vehicle::*",
            "--filter", "part",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("expose Vehicle::*;"))
        .stdout(predicate::str::contains("filter @type istype part;"));
}

#[test]
fn add_stdout_unknown_usage() {
    // Unknown kinds are treated as usage-level and produce "kind name;" output
    cmd()
        .args(["add", "--stdout", "bogus", "Foo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bogus Foo;"));
}

#[test]
fn add_stdout_unknown_def_kind() {
    // A kind with "def" suffix but not recognized should error
    cmd()
        .args(["add", "--stdout", "bogus-def", "Foo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown"));
}

#[test]
fn add_insert_into_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.sysml");
    fs::write(&file, "part def Vehicle;\n").unwrap();

    cmd()
        .args([
            "add",
            file.to_str().unwrap(),
            "part-def", "Engine",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("+part def Engine;"));
}

// ========================================================================
// remove (replaces edit remove)
// ========================================================================

#[test]
fn remove_dry_run() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.sysml");
    fs::write(&file, "part def Vehicle;\npart def Engine;\n").unwrap();

    cmd()
        .args([
            "remove",
            file.to_str().unwrap(),
            "Engine",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("-part def Engine;"));
}

// ========================================================================
// rename (replaces edit rename)
// ========================================================================

#[test]
fn rename_dry_run() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.sysml");
    fs::write(&file, "part def Vehicle;\npart def Engine;\n").unwrap();

    cmd()
        .args([
            "rename",
            file.to_str().unwrap(),
            "Engine", "Motor",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("-part def Engine;"))
        .stdout(predicate::str::contains("+part def Motor;"));
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
// stats
// ========================================================================

#[test]
fn stats_basic() {
    cmd()
        .args(["stats", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Definitions:"))
        .stdout(predicate::str::contains("Usages:"));
}

#[test]
fn stats_json() {
    cmd()
        .args(["stats", "-f", "json", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"total_definitions\""));
}

// ========================================================================
// deps
// ========================================================================

#[test]
fn deps_basic() {
    cmd()
        .args(["deps", &fixture("simple-vehicle.sysml"), "Engine"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Referenced by"));
}

#[test]
fn deps_missing_target() {
    cmd()
        .args(["deps", &fixture("simple-vehicle.sysml"), "NonExistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ========================================================================
// diff
// ========================================================================

#[test]
fn diff_identical_files() {
    cmd()
        .args(["diff", &fixture("simple-vehicle.sysml"), &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("No semantic differences"));
}

#[test]
fn diff_different_files() {
    let dir = tempfile::tempdir().unwrap();
    let file_a = dir.path().join("a.sysml");
    let file_b = dir.path().join("b.sysml");
    fs::write(&file_a, "part def Vehicle;\npart def Engine;\n").unwrap();
    fs::write(&file_b, "part def Vehicle;\npart def Motor;\n").unwrap();

    cmd()
        .args(["diff", file_a.to_str().unwrap(), file_b.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("+ Motor"))
        .stdout(predicate::str::contains("- Engine"));
}

// ========================================================================
// allocation
// ========================================================================

#[test]
fn allocation_basic() {
    cmd()
        .args(["allocation", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

// ========================================================================
// coverage
// ========================================================================

#[test]
fn coverage_basic() {
    cmd()
        .args(["coverage", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Overall score:"));
}

#[test]
fn coverage_json() {
    cmd()
        .args(["coverage", "-f", "json", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"overall_score\""));
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
        .stdout(predicate::str::contains("sysml"));
}

#[test]
fn completions_bash() {
    cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_sysml"));
}

#[test]
fn completions_zsh() {
    cmd()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sysml"));
}

// ========================================================================
// lint suggestions ("did you mean")
// ========================================================================

#[test]
fn lint_suggests_closest_match() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("typo.sysml");
    fs::write(&file, "part def Vehicle;\npart car : Vehicel;\n").unwrap();

    cmd()
        .args(["lint", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("did you mean `Vehicle`?"));
}

// ========================================================================
// quality
// ========================================================================

#[cfg(feature = "capa")]
#[test]
fn quality_list() {
    cmd()
        .args(["quality", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("NCR"))
        .stdout(predicate::str::contains("CAPA"))
        .stdout(predicate::str::contains("Process Deviation"));
}

#[cfg(feature = "capa")]
#[test]
fn quality_trend_no_files() {
    cmd()
        .args(["quality", "trend"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Trend Analysis"));
}

#[cfg(feature = "capa")]
#[test]
fn quality_create_requires_terminal() {
    // Non-interactive invocation should fail gracefully
    cmd()
        .args(["quality", "create", "--type", "ncr"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

#[cfg(feature = "capa")]
#[test]
fn quality_rca_requires_terminal() {
    cmd()
        .args(["quality", "rca", "--source", "NCR-001", "--method", "five-why"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

#[cfg(feature = "capa")]
#[test]
fn quality_action_requires_terminal() {
    cmd()
        .args(["quality", "action", "--capa", "CAPA-001"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

// ========================================================================
// pipeline
// ========================================================================

#[test]
fn pipeline_list_no_project() {
    // Running without .sysml/config.toml should fail
    cmd()
        .args(["pipeline", "list"])
        .current_dir(std::env::temp_dir())
        .assert()
        .failure()
        .stderr(predicate::str::contains("config.toml"));
}

#[test]
fn pipeline_run_no_project() {
    cmd()
        .args(["pipeline", "run", "ci"])
        .current_dir(std::env::temp_dir())
        .assert()
        .failure()
        .stderr(predicate::str::contains("config.toml"));
}

#[test]
fn pipeline_list_with_config() {
    let tmp = std::env::temp_dir().join("sysml_pipeline_test_list");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".sysml")).unwrap();
    std::fs::write(
        tmp.join(".sysml/config.toml"),
        r#"
[project]
name = "PipeTest"

[[pipeline]]
name = "ci"
steps = ["lint *.sysml", "fmt --check *.sysml"]
"#,
    )
    .unwrap();

    cmd()
        .args(["pipeline", "list"])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("ci"))
        .stdout(predicate::str::contains("2 steps"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn pipeline_run_dry_run() {
    let tmp = std::env::temp_dir().join("sysml_pipeline_test_dry");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".sysml")).unwrap();
    std::fs::write(
        tmp.join(".sysml/config.toml"),
        r#"
[project]
name = "DryTest"

[[pipeline]]
name = "check"
steps = ["lint model.sysml"]
"#,
    )
    .unwrap();

    cmd()
        .args(["pipeline", "run", "check", "--dry-run"])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pipeline: check"))
        .stdout(predicate::str::contains("dry run"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn pipeline_run_unknown_name() {
    let tmp = std::env::temp_dir().join("sysml_pipeline_test_unknown");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".sysml")).unwrap();
    std::fs::write(
        tmp.join(".sysml/config.toml"),
        "[project]\nname = \"Test\"\n",
    )
    .unwrap();

    cmd()
        .args(["pipeline", "run", "nonexistent"])
        .current_dir(&tmp)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no pipeline named"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn stdlib_path_flag_accepted() {
    // Just verify the --stdlib-path flag is accepted without error
    cmd()
        .args(["--stdlib-path", "/nonexistent/stdlib", "lint", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

#[test]
fn pipeline_create_adds_to_config() {
    let tmp = std::env::temp_dir().join("sysml_pipeline_test_create");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".sysml")).unwrap();
    std::fs::write(
        tmp.join(".sysml/config.toml"),
        "[project]\nname = \"CreateTest\"\n",
    )
    .unwrap();

    cmd()
        .args(["pipeline", "create", "deploy"])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created pipeline"));

    // Verify the config was updated
    let config_content = std::fs::read_to_string(tmp.join(".sysml/config.toml")).unwrap();
    assert!(config_content.contains("[[pipeline]]"));
    assert!(config_content.contains("deploy"));

    let _ = std::fs::remove_dir_all(&tmp);
}

// ========================================================================
// rollup
// ========================================================================

#[test]
fn rollup_compute_mass() {
    cmd()
        .args(["rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass"])
        .assert()
        .success()
        .stdout(predicate::str::contains("900"));
}

#[test]
fn rollup_compute_cost() {
    cmd()
        .args(["rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "cost"])
        .assert()
        .success()
        .stdout(predicate::str::contains("17300"));
}

#[test]
fn rollup_compute_json() {
    cmd()
        .args(["-f", "json", "rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"total\": 900"));
}

#[test]
fn rollup_compute_rss() {
    cmd()
        .args(["rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass", "--method", "rss"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rss"));
}

#[test]
fn rollup_budget_pass() {
    cmd()
        .args(["rollup", "budget", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass", "--limit", "1000"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn rollup_budget_fail() {
    cmd()
        .args(["rollup", "budget", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass", "--limit", "500"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn rollup_sensitivity() {
    cmd()
        .args(["rollup", "sensitivity", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass"])
        .assert()
        .success()
        .stdout(predicate::str::contains("body"))
        .stdout(predicate::str::contains("44.4%"));
}

#[test]
fn rollup_query() {
    cmd()
        .args(["rollup", "query", &fixture("rollup-vehicle.sysml"),
               "--attr", "mass"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine"))
        .stdout(predicate::str::contains("Wheel"))
        .stdout(predicate::str::contains("Vehicle"));
}

#[test]
fn rollup_unknown_root() {
    cmd()
        .args(["rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "NonExistent", "--attr", "mass"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn rollup_unknown_method() {
    cmd()
        .args(["rollup", "compute", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass", "--method", "bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown aggregation"));
}

// ========================================================================
// analyze
// ========================================================================

#[test]
fn analyze_list() {
    cmd()
        .args(["analyze", "list", &fixture("analysis-trade.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("FuelAnalysis"))
        .stdout(predicate::str::contains("EngineTradeOff"));
}

#[test]
fn analyze_list_json() {
    cmd()
        .args(["-f", "json", "analyze", "list", &fixture("analysis-trade.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"FuelAnalysis\""));
}

#[test]
fn analyze_run() {
    cmd()
        .args(["analyze", "run", &fixture("analysis-trade.sysml"),
               "-n", "FuelAnalysis"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Subject: vehicle"))
        .stdout(predicate::str::contains("Return: fuelEconomy"));
}

#[test]
fn analyze_trade() {
    cmd()
        .args(["analyze", "trade", &fixture("analysis-trade.sysml"),
               "-n", "EngineTradeOff"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Trade Study"))
        .stdout(predicate::str::contains("Maximize"))
        .stdout(predicate::str::contains("engine4cyl"))
        .stdout(predicate::str::contains("engine6cyl"));
}

#[test]
fn analyze_trade_no_alternatives() {
    cmd()
        .args(["analyze", "trade", &fixture("analysis-trade.sysml"),
               "-n", "FuelAnalysis"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no alternatives"));
}

#[test]
fn analyze_unknown_name() {
    cmd()
        .args(["analyze", "run", &fixture("analysis-trade.sysml"),
               "-n", "NonExistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ========================================================================
// find
// ========================================================================

#[test]
fn find_by_name() {
    cmd()
        .args(["find", &fixture("rollup-vehicle.sysml"), "--pattern", "Engine"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine"));
}

#[test]
fn find_by_attribute() {
    cmd()
        .args(["find", &fixture("rollup-vehicle.sysml"), "--pattern", "mass"])
        .assert()
        .success()
        .stdout(predicate::str::contains("5 match"));
}

#[test]
fn find_defs_only() {
    cmd()
        .args(["find", &fixture("rollup-vehicle.sysml"), "--pattern", "Engine", "--kind", "defs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 match"));
}

#[test]
fn find_no_matches() {
    cmd()
        .args(["find", &fixture("rollup-vehicle.sysml"), "--pattern", "nonexistent"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No matches"));
}

#[test]
fn find_json() {
    cmd()
        .args(["-f", "json", "find", &fixture("rollup-vehicle.sysml"), "--pattern", "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"Vehicle\""));
}

// ========================================================================
// rollup sweep and what-if
// ========================================================================

#[test]
fn rollup_sweep() {
    cmd()
        .args(["rollup", "sweep", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass",
               "--param", "engine", "--from", "100", "--to", "300", "--steps", "3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sweep"))
        .stdout(predicate::str::contains("Sensitivity"));
}

#[test]
fn rollup_sweep_json() {
    cmd()
        .args(["-f", "json", "rollup", "sweep", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass",
               "--param", "engine", "--from", "100", "--to", "200", "--steps", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"sensitivity\""));
}

#[test]
fn rollup_what_if() {
    cmd()
        .args(["rollup", "what-if", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass",
               "-s", "light:engine=100", "-s", "heavy:engine=300"])
        .assert()
        .success()
        .stdout(predicate::str::contains("What-if"))
        .stdout(predicate::str::contains("light"))
        .stdout(predicate::str::contains("heavy"));
}

#[test]
fn rollup_what_if_json() {
    cmd()
        .args(["-f", "json", "rollup", "what-if", &fixture("rollup-vehicle.sysml"),
               "--root", "Vehicle", "--attr", "mass",
               "-s", "test:engine=150"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"baseline\""));
}

// ========================================================================
// doc
// ========================================================================

#[test]
fn doc_generates_markdown() {
    cmd()
        .args(["doc", &fixture("rollup-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Model Documentation"))
        .stdout(predicate::str::contains("Vehicle"));
}

#[test]
fn doc_with_root() {
    cmd()
        .args(["doc", &fixture("rollup-vehicle.sysml"), "--root", "Vehicle"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Vehicle"));
}

#[test]
fn doc_json() {
    cmd()
        .args(["-f", "json", "doc", &fixture("rollup-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"markdown\""));
}

#[test]
fn doc_includes_definitions() {
    cmd()
        .args(["doc", &fixture("rollup-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine"))
        .stdout(predicate::str::contains("Vehicle"));
}

// ========================================================================
// Standard view diagram types (new names)
// ========================================================================

#[test]
fn diagram_gv_mermaid() {
    cmd()
        .args(["diagram", "-t", "gv", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("classDiagram"));
}

#[test]
fn diagram_stv_alias() {
    cmd()
        .args(["diagram", "-t", "stv", &fixture("simulation.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("stateDiagram"));
}

#[test]
fn diagram_afv_alias() {
    cmd()
        .args(["diagram", "-t", "afv", &fixture("simulation.sysml")])
        .assert()
        .success();
}

#[test]
fn diagram_bv_alias() {
    cmd()
        .args(["diagram", "-t", "bv", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("classDiagram"));
}

#[test]
fn diagram_sv_sequence() {
    cmd()
        .args(["diagram", "-t", "sv", &fixture("annex-a-simple-vehicle-model.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("sequenceDiagram"));
}

#[test]
fn diagram_legacy_bdd_still_works() {
    cmd()
        .args(["diagram", "-t", "bdd", &fixture("simple-vehicle.sysml")])
        .assert()
        .success()
        .stdout(predicate::str::contains("classDiagram"));
}

// ========================================================================
// check command (direct, not via lint alias)
// ========================================================================

#[test]
fn check_direct() {
    cmd()
        .args(["check", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

#[test]
fn check_severity_error() {
    cmd()
        .args(["check", "--severity", "error", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

// ========================================================================
// interfaces command
// ========================================================================

#[test]
fn interfaces_basic() {
    cmd()
        .args(["interfaces", &fixture("simple-vehicle.sysml")])
        .assert()
        .success();
}

// ========================================================================
// rename --project
// ========================================================================

#[test]
fn rename_project_dry_run() {
    cmd()
        .args(["rename", &fixture("rollup-vehicle.sysml"),
               "Engine", "Motor", "--dry-run", "--project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine").or(predicate::str::contains("Motor")));
}

// ========================================================================
// deps --transitive
// ========================================================================

#[test]
fn deps_transitive() {
    cmd()
        .args(["deps", &fixture("simple-vehicle.sysml"), "Engine", "--transitive"])
        .assert()
        .success();
}
