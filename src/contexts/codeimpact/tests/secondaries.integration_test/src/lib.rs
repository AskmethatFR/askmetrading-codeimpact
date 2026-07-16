/// Shared test-only helpers for this crate's `tests/*.rs` files (#63): each
/// integration test file is compiled as its own binary, so this is the one
/// place the "ensure the probe binary exists" plumbing is written once and
/// imported everywhere it is needed, instead of duplicated per file.
pub mod support {
    use std::path::PathBuf;
    use std::process::Command;

    pub fn workspace_root() -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for _ in 0..5 {
            path.pop();
        }
        path
    }

    /// Builds `codeimpact-parse-probe` into `target/debug` if it is not
    /// already there. A test binary's own `current_exe()` lives one level
    /// deeper (`target/debug/deps/`), so once built here it is discovered
    /// automatically via `SynCodeParser`'s "grandparent of current_exe"
    /// fallback — no `CODEIMPACT_PARSE_PROBE` override needed.
    pub fn ensure_probe_built() {
        let probe = workspace_root().join("target").join("debug").join(format!(
            "codeimpact-parse-probe{}",
            std::env::consts::EXE_SUFFIX
        ));
        if !probe.exists() {
            let status = Command::new("cargo")
                .args([
                    "build",
                    "-p",
                    "codeimpact_secondaries",
                    "--bin",
                    "codeimpact-parse-probe",
                ])
                .current_dir(workspace_root())
                .status()
                .expect("failed to build probe binary");
            assert!(status.success(), "probe binary build failed");
        }
    }
}
