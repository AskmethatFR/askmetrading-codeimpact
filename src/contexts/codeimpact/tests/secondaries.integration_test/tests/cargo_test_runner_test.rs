use std::process::Command;
use std::sync::Mutex;

use codeimpact_hexagon::analysis::TestRunnerPort;
use codeimpact_secondaries::gateways::test_runners::cargo_test_runner::CargoTestRunner;

/// Every test in this file shells out to real `cargo`/`rustc` processes
/// against its own temp crate. Rust's test harness runs tests within one
/// binary in parallel by default, so without serialization these
/// concurrent compiles genuinely contend for CPU/IO on the SAME host at
/// the SAME time — self-inflicted noise (not external host load) that
/// made an earlier version of
/// `cargo_test_runner_excludes_build_time_from_measured_duration` flaky
/// specifically when run as part of the full suite (a measure-phase-only
/// timing spiking to 1350ms under contention), even though it was stable
/// in isolation. Discovered running the mandated `cargo test --workspace`
/// gate (#36 retry, last).
fn lock_cargo_spawn() -> std::sync::MutexGuard<'static, ()> {
    static CARGO_SPAWN_LOCK: Mutex<()> = Mutex::new(());
    CARGO_SPAWN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
    let _guard = lock_cargo_spawn();
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
    let _guard = lock_cargo_spawn();
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
    let _guard = lock_cargo_spawn();
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

/// Injected, wall-clock (not CPU-bound) delay a `build.rs` sleeps for
/// before compilation can proceed. A `std::thread::sleep` floor cannot be
/// shortened by host load — contention can only push it later, never
/// earlier — which is what makes it a safe, deterministic stand-in for
/// "build time", unlike measuring a real `rustc` compile (see history
/// below).
const INJECTED_BUILD_DELAY_MS: u64 = 20_000;

/// How high `duration_ms` may legitimately go for JUST running two trivial
/// assertions. Sized well above the highest measure-phase-only timing
/// observed while calibrating this test (a freshly linked binary's FIRST
/// execution on this host occasionally took ~2-3s — plausibly on-access
/// AV/EDR or Gatekeeper validation of a just-written executable, unrelated
/// to this crate's code), while staying well below
/// `INJECTED_BUILD_DELAY_MS`, which mutated code cannot finish under.
const MEASURED_DURATION_THRESHOLD_MS: u64 = 8_000;

fn create_temp_crate_with_slow_build(name: &str) -> tempfile::TempDir {
    let dir = create_temp_crate(name);
    std::fs::write(
        dir.path().join("build.rs"),
        format!(
            "fn main() {{ std::thread::sleep(std::time::Duration::from_millis({INJECTED_BUILD_DELAY_MS})); }}\n"
        ),
    )
    .expect("write build.rs");
    dir
}

// #36 bug 2 / retry (last) — the acceptance criterion for "build is
// excluded from the measurement".
//
// History: an earlier version asserted an absolute bound (`duration_ms <
// 2000`), which QA reproduced as flaky under machine load. That was
// replaced with a cold/warm RATIO (run the same crate unbuilt, then
// already-built, compare `duration_ms` of the two). QA then mutated
// `run_cargo_test` to fold build time back into `duration_ms` — reintroducing
// bug 2 verbatim — and the ratio test stayed GREEN across 4 runs. Root
// cause: the "warm" call still shells out to `cargo test --no-run` to
// check freshness, so its `duration_ms` was dominated by cargo/rustc
// process-spawn overhead — the SAME fixed cost that dominates a cold
// compile of this trivial, zero-dependency crate. A THIRD attempt compared
// `duration_ms` against an independently-measured "ground truth" (a direct,
// test-only call to the measure phase, bypassing `run_cargo_test`
// entirely) — a real improvement, but it still compared two REAL,
// host-dependent timings, and under genuine contention (this file's other
// cargo-spawning tests running concurrently) the correct-code measure
// phase itself was observed spiking to 1350ms, well past any ratio bound
// that also has to reject a merely-600ms-ish mutated reading. Two
// real-world timings of the same order of magnitude can always be pushed
// into each other's range by load — no ratio or multiplier fixes that.
//
// Fix: stop measuring real build time altogether. Give the temp crate a
// `build.rs` that deliberately sleeps for a KNOWN, fixed
// `INJECTED_BUILD_DELAY_MS` before compilation proceeds. This turns "how
// long did the build take" from a noisy, host-dependent quantity into a
// constant we control:
//   - Correct code: `duration_ms` is just the measure phase (running 2
//     trivial assertions) — a build script only runs during compilation,
//     never during the compiled binary's execution, so it never touches
//     this number. Calibration on this host showed at most ~3.2s for that
//     phase, entirely due to first-execution latency of a just-linked
//     binary (see `MEASURED_DURATION_THRESHOLD_MS`), never anywhere near
//     `INJECTED_BUILD_DELAY_MS`.
//   - Mutated code (build folded into `duration_ms`, #36 bug 2
//     reintroduced): `duration_ms` includes the `cargo test --no-run`
//     call that runs `build.rs`, so it is AT LEAST
//     `INJECTED_BUILD_DELAY_MS` — a `sleep()` floor that contention cannot
//     lower.
// `MEASURED_DURATION_THRESHOLD_MS` sits in a gap neither side can cross:
// correct code has no path to spending seconds on measure-phase-only work,
// and mutated code cannot finish in under `INJECTED_BUILD_DELAY_MS`
// because the sleep alone blocks that long — the two are separated by a
// difference in KIND of work (a fixed sleep vs. running two assertions),
// not by a margin contention could close.
#[test]
fn cargo_test_runner_excludes_build_time_from_measured_duration() {
    let _guard = lock_cargo_spawn();
    let crate_dir = create_temp_crate_with_slow_build("slow_build_crate");
    let runner = CargoTestRunner::new(crate_dir.path().to_path_buf());

    let result = runner.run_tests(None).expect("run_tests should succeed");

    assert_eq!(result.tests_total(), 2);
    assert_eq!(result.tests_passed(), 2);
    assert!(
        result.duration_ms() < MEASURED_DURATION_THRESHOLD_MS,
        "duration_ms ({}) should be far below the {INJECTED_BUILD_DELAY_MS}ms \
         build.rs deliberately sleeps for — a value this high means build \
         time leaked into the measured duration",
        result.duration_ms()
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

    let _guard = lock_cargo_spawn();
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

fn write_package(dir: &std::path::Path, name: &str, lib_rs: &str) {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
"#,
            name
        ),
    )
    .expect("write Cargo.toml");
    std::fs::write(src.join("lib.rs"), lib_rs).expect("write lib.rs");
}

// #36 retry N3 (P2, QA blocking) — `build_test_binary`'s `if
// !output.status.success()` check is genuinely new logic: the pre-#36 code
// never inspected the build's exit status at all. It ships realistic
// coverage here: a workspace with ONE package that compiles fine (and
// therefore DOES emit a valid `compiler-artifact` line with a real
// `executable` path for its test binary) alongside a SIBLING package that
// fails to compile. Cargo's overall exit status is still failure (101), but
// stdout legitimately contains a usable test-binary path for the healthy
// package. Without the exit-status check, `parse_test_binary_path` would
// happily pick that path and `run_tests` would return `Ok(..)` with a
// passing measurement for `good` — a bogus success hiding the real compile
// error in `bad`. This is exactly the "silent success" / "bogus
// measurement" scenario the check exists to prevent.
#[test]
fn cargo_test_runner_returns_error_when_a_workspace_member_fails_to_compile() {
    let _guard = lock_cargo_spawn();
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["good", "bad"]
resolver = "2"
"#,
    )
    .expect("write workspace Cargo.toml");

    write_package(
        &dir.path().join("good"),
        "good",
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n#[test]\nfn test_add() { assert_eq!(add(2, 2), 4); }\n",
    );
    // Deliberately invalid Rust: unclosed delimiter.
    write_package(
        &dir.path().join("bad"),
        "bad",
        "pub fn broken(a: i32, b: i32 -> i32 { a + b }\n",
    );

    let runner = CargoTestRunner::new(dir.path().to_path_buf());
    let result = runner.run_tests(None);

    assert!(
        result.is_err(),
        "expected run_tests to fail when a workspace member has a compile \
         error, even though a sibling member built a valid test binary; got {:?}",
        result
    );
}
