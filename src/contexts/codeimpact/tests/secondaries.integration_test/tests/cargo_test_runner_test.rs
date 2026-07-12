use std::process::Command;

use codeimpact_hexagon::analysis::TestRunnerPort;
use codeimpact_secondaries::gateways::test_runners::cargo_test_runner::CargoTestRunner;

fn create_temp_crate(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir(&src).expect("create src dir");

    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
"#,
        name
    );
    std::fs::write(dir.path().join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    let lib_rs = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[test]
fn test_add() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn test_add_negative() {
    assert_eq!(add(-1, 1), 0);
}
"#;
    std::fs::write(src.join("lib.rs"), lib_rs).expect("write lib.rs");

    dir
}

#[test]
fn cargo_test_runner_runs_tests_and_returns_metrics() {
    let crate_dir = create_temp_crate("test_crate");

    // Build the crate first so the test run is fast
    let build = Command::new("cargo")
        .args(["test", "--no-run"])
        .current_dir(crate_dir.path())
        .output()
        .expect("build test crate");
    assert!(
        build.status.success(),
        "build failed: {:?}",
        String::from_utf8_lossy(&build.stderr)
    );

    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());
    let result = runner.run_tests(None).expect("run_tests should succeed");

    assert_eq!(result.tests_total(), 2);
    assert_eq!(result.tests_passed(), 2);
    assert!(result.duration_ms() > 0);
}

#[test]
fn cargo_test_runner_with_filter() {
    let crate_dir = create_temp_crate("test_crate_filter");

    let build = Command::new("cargo")
        .args(["test", "--no-run"])
        .current_dir(crate_dir.path())
        .output()
        .expect("build test crate");
    assert!(build.status.success(), "build failed");

    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());
    let result = runner
        .run_tests(Some("test_add_negative"))
        .expect("run_tests with filter should succeed");

    assert_eq!(result.tests_total(), 1);
    assert_eq!(result.tests_passed(), 1);
    assert_eq!(result.filter(), Some("test_add_negative".to_string()));
}

#[test]
fn cargo_test_runner_on_empty_crate_returns_zero() {
    let crate_dir = create_temp_crate("empty_crate");
    std::fs::write(crate_dir.path().join("src/lib.rs"), "").expect("write empty lib.rs");

    let build = Command::new("cargo")
        .args(["test", "--no-run"])
        .current_dir(crate_dir.path())
        .output()
        .expect("build test crate");
    assert!(build.status.success(), "build failed");

    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());
    let result = runner.run_tests(None).expect("run_tests should succeed");

    assert_eq!(result.tests_total(), 0);
    assert_eq!(result.tests_passed(), 0);
}

// #36 bug 2 — the acceptance criterion for "build is excluded from the
// measurement": deliberately do NOT pre-build this crate. `run_tests` must
// still build it internally (unmeasured) before measuring, so the reported
// `duration_ms` reflects only running two trivial assertions — not the
// rustc compile, which alone takes far longer than this bound. A flaky
// relative-timing comparison (build twice, compare) is avoided on purpose;
// this is a single generous absolute bound.
#[test]
fn cargo_test_runner_excludes_build_time_from_measured_duration() {
    let crate_dir = create_temp_crate("unbuilt_crate");
    // Intentionally no pre-build here: `run_tests` must do it internally.

    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());
    let result = runner.run_tests(None).expect("run_tests should succeed");

    assert_eq!(result.tests_total(), 2);
    assert_eq!(result.tests_passed(), 2);
    assert!(
        result.duration_ms() < 2000,
        "duration_ms should reflect only the measured run, not the build; got {} ms",
        result.duration_ms()
    );
}
