// Ticket #51 retry 1 — QA reproduced a regression: `ensure_bin_built` detected
// the build profile via `!cfg!(debug_assertions)`, but #51's own
// `[profile.release] debug-assertions = true` makes that cfg `true` in BOTH
// profiles, so `is_release` was permanently `false` under `--release` and the
// probe was built into the wrong `target/` directory (never found by
// `SynCodeParser::discover_probe_path`, which walks `current_exe()`'s own
// `target/<profile>/deps/` ancestry instead).
//
// `is_release_exe_path` fixes this by reading the SAME path signal
// `discover_probe_path` already relies on, so both agree on what "release"
// means without touching any Cargo profile flag.
//
// Test List:
// 1. release_exe_path_under_target_release_deps_is_release — the shape a
//    real `--release` test binary reports via `current_exe()`.
// 2. debug_exe_path_under_target_debug_deps_is_not_release — the shape a
//    plain `cargo test` binary reports.
// 3. path_with_no_grandparent_is_not_release — a malformed/too-short path
//    (no `target/<profile>/deps` ancestry) must fall back to `false`
//    (build into `debug`) rather than panicking.

use codeimpact_secondaries_integration_test::support::is_release_exe_path;
use std::path::Path;

#[test]
fn release_exe_path_under_target_release_deps_is_release() {
    let exe = Path::new("/workspace/target/release/deps/some_test-abcd1234");

    assert!(is_release_exe_path(exe));
}

#[test]
fn debug_exe_path_under_target_debug_deps_is_not_release() {
    let exe = Path::new("/workspace/target/debug/deps/some_test-abcd1234");

    assert!(!is_release_exe_path(exe));
}

#[test]
fn path_with_no_grandparent_is_not_release() {
    let exe = Path::new("some_test-abcd1234");

    assert!(!is_release_exe_path(exe));
}
