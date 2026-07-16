#![cfg(unix)]

use std::io::Write;
use std::process::Command;
use std::process::Stdio;

use codeimpact_secondaries_integration_test::support::ensure_bin_built;

/// #63 T3 (iii) — the probe's own `RLIMIT_AS` (2 GiB, self-applied at
/// startup) must not break an ordinary, healthy parse: this spawns the
/// real `codeimpact-parse-probe` binary directly (not through
/// `SynCodeParser`) and checks it still exits 0 under its own limit.
#[test]
fn healthy_source_parses_successfully_under_the_probes_own_rlimit() {
    let probe = ensure_bin_built("codeimpact_secondaries", "codeimpact-parse-probe");

    let mut child = Command::new(probe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the probe directly");

    child
        .stdin
        .take()
        .expect("probe should have a stdin pipe")
        .write_all(b"fn f() { if true {} }")
        .expect("failed to write to probe stdin");

    let status = child.wait().expect("failed to wait on probe");

    assert_eq!(
        status.code(),
        Some(0),
        "a healthy source must still parse cleanly under the probe's own RLIMIT_AS"
    );
}
