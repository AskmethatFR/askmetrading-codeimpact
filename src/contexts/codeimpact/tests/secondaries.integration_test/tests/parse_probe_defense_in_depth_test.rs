use std::io::Write;
use std::process::Command;
use std::process::Stdio;

use codeimpact_secondaries_integration_test::support::ensure_bin_built;

/// The exit code `parse_probe.rs` uses for "abnormal termination that is
/// NOT a proven syntax error" — stdin unreadable, the defense-in-depth
/// size guard tripping, or a parse-thread panic. Mirrors
/// `EXIT_NOT_PROVEN_SAFE` in `src/bin/parse_probe.rs`; kept as a literal
/// here (not imported) since a `[[bin]]`'s internal constants are not
/// part of the crate's public API — this test asserts the OBSERVABLE
/// contract (the exit code the parent's `verdict_from` actually sees).
const EXIT_NOT_PROVEN_SAFE: i32 = 101;

fn probe_path() -> std::path::PathBuf {
    ensure_bin_built("codeimpact_secondaries", "codeimpact-parse-probe")
}

fn run_probe_with_stdin(bytes: &[u8]) -> i32 {
    let mut child = Command::new(probe_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the probe directly");

    child
        .stdin
        .take()
        .expect("probe should have a stdin pipe")
        .write_all(bytes)
        .expect("failed to write to probe stdin");

    let status = child.wait().expect("failed to wait on probe");
    status
        .code()
        .expect("probe should exit with a status code, not a signal")
}

/// QA finding (retry 1): `parse_probe.rs`'s `read_to_string(...).is_err()`
/// guard had no test — QA deleted it and the suite stayed green. Invalid
/// UTF-8 on stdin is the only realistic way to make `read_to_string` fail
/// (a real pipe never gets an I/O error mid-stream in this scenario), and
/// must be refused (never silently treated as admissible or as a syntax
/// error the parent would trust enough to re-parse).
#[test]
fn invalid_utf8_stdin_is_refused_not_proven_safe() {
    let invalid_utf8: &[u8] = &[0x66, 0x6e, 0x20, 0xff, 0xfe, 0x00, 0x28, 0x29];

    let code = run_probe_with_stdin(invalid_utf8);

    assert_eq!(
        code, EXIT_NOT_PROVEN_SAFE,
        "invalid UTF-8 on stdin must exit EXIT_NOT_PROVEN_SAFE, not be silently accepted"
    );
}

/// QA finding (retry 1): the probe's own defense-in-depth
/// `check_admissible` guard (in case it is ever invoked directly, bypassing
/// the parent's own size check) had no test — the only tested path blocks
/// the source at the PARENT before the probe is ever spawned. Feeding an
/// oversized source directly to the real binary exercises the probe's own
/// copy of the guard.
#[test]
fn oversized_stdin_is_refused_not_proven_safe() {
    let oversized = "a".repeat(1024 * 1024 + 1);

    let code = run_probe_with_stdin(oversized.as_bytes());

    assert_eq!(
        code, EXIT_NOT_PROVEN_SAFE,
        "an oversized source fed directly to the probe must exit EXIT_NOT_PROVEN_SAFE"
    );
}
