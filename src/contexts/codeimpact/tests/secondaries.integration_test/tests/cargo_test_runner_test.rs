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
// measurement". An earlier version of this test asserted an absolute bound
// (`duration_ms < 2000`) which QA reproduced as flaky under machine load
// (14674ms, 8067ms on a contended runner). Replaced with a relative,
// load-tolerant comparison: run the SAME crate cold (unbuilt — `run_tests`
// must compile it internally, unmeasured) and then warm (already built by
// the cold call). If build time leaked into the measured window, the cold
// duration would be dominated by the rustc compile and dwarf the warm
// duration by an order of magnitude or more. If build time is correctly
// excluded, both calls measure only the same two trivial assertions and
// stay within the same order of magnitude — regardless of how loaded the
// machine is, because contention inflates both runs together (they run
// back-to-back on the same host under the same conditions), not just one.
#[test]
fn cargo_test_runner_excludes_build_time_from_measured_duration() {
    let crate_dir = create_temp_crate("cold_warm_crate");
    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());

    // Cold: crate_dir is intentionally NOT pre-built. `run_tests` must
    // compile it internally before measuring.
    let cold = runner
        .run_tests(None)
        .expect("cold run_tests should succeed");
    assert_eq!(cold.tests_total(), 2);
    assert_eq!(cold.tests_passed(), 2);

    // Warm: same crate, now already built by the cold call above.
    let warm = runner
        .run_tests(None)
        .expect("warm run_tests should succeed");
    assert_eq!(warm.tests_total(), 2);
    assert_eq!(warm.tests_passed(), 2);

    let cold_ms = cold.duration_ms().max(1) as f64;
    let warm_ms = warm.duration_ms().max(1) as f64;
    let ratio = cold_ms / warm_ms;

    // 30x is deliberately generous: rustc startup overhead alone puts a
    // genuine (excluded) compile at several hundred ms to ~1-2s, while the
    // measured run of two trivial assertions is a handful of ms — so a real
    // regression (build time leaking into the window) produces a ratio far
    // above this, typically 50-200x+. 30x still comfortably absorbs a noisy
    // CI host (observed up to ~13x from transient contention during manual
    // validation) without masking the actual regression this test exists to
    // catch.
    assert!(
        ratio < 30.0,
        "cold run ({cold_ms} ms) should stay within the same order of \
         magnitude as warm run ({warm_ms} ms) — a {ratio:.1}x ratio suggests \
         build time leaked into the measured duration"
    );
}

// #36 retry N2 — the mirror image of the Unmeasurable acceptance criteria:
// when a sampler genuinely IS available, cpu_time_ms()/memory_kb() must come
// back Available(_), not Unmeasurable. Guarded (not skipped/failed) on hosts
// without /usr/bin/time — CI runs on Linux where it exists, but a bare
// container must not see a red suite for a missing optional tool.
#[test]
fn cargo_test_runner_with_sampler_available_reports_measured_cpu_and_memory() {
    if !std::path::Path::new("/usr/bin/time").exists() {
        eprintln!("skipping: /usr/bin/time not available on this host");
        return;
    }

    let crate_dir = create_temp_crate("sampled_crate");

    let build = Command::new("cargo")
        .args(["test", "--no-run"])
        .current_dir(crate_dir.path())
        .output()
        .expect("build test crate");
    assert!(build.status.success(), "build failed");

    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());
    let result = runner.run_tests(None).expect("run_tests should succeed");

    assert!(
        result.cpu_time_ms().available().is_some(),
        "expected cpu_time_ms to be Available when /usr/bin/time is present, got {:?}",
        result.cpu_time_ms()
    );
    assert!(
        result.memory_kb().available().is_some(),
        "expected memory_kb to be Available when /usr/bin/time is present, got {:?}",
        result.memory_kb()
    );
}
