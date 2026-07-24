/// Shared test-only helpers for this crate's `tests/*.rs` files (#63): each
/// integration test file is compiled as its own binary, so this is the one
/// place the "ensure the probe binary exists" plumbing is written once and
/// imported everywhere it is needed, instead of duplicated per file.
pub mod support {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub fn workspace_root() -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for _ in 0..5 {
            path.pop();
        }
        path
    }

    /// Builds `codeimpact-parse-probe` fresh into `target/debug`. A test
    /// binary's own `current_exe()` lives one level deeper
    /// (`target/debug/deps/`), so once built here it is discovered
    /// automatically via `SynCodeParser`'s "grandparent of current_exe"
    /// fallback — no `CODEIMPACT_PARSE_PROBE` override needed.
    pub fn ensure_probe_built() {
        ensure_bin_built("codeimpact_secondaries", "codeimpact-parse-probe");
    }

    /// Builds `bin_name` (a `[[bin]]` target of `package`) into the SAME
    /// profile directory (`target/debug` or `target/release`) this test
    /// binary itself was compiled under, and returns its path.
    /// Generalizes `ensure_probe_built` for this crate's own fake-probe
    /// `[[bin]]`s (T3: a sleeping probe for the timeout path, an
    /// unknown-exit-code probe — portable Rust binaries rather than shell
    /// scripts).
    ///
    /// Profile-matched, not hardcoded to `debug` (retry 2, found while
    /// verifying `cargo test -p codeimpact_secondaries_integration_test
    /// --release` for a DIFFERENT finding): `SynCodeParser::discover_probe_path`
    /// finds a probe via `current_exe()`'s own directory — under
    /// `--release` that is `target/release/deps/`, so its "grandparent"
    /// fallback lands in `target/release`, NOT `target/debug`. A helper
    /// that always builds into `target/debug` would leave a `--release`
    /// run silently picking up whatever stale binary happens to already
    /// sit in `target/release` (e.g. from an earlier manual `cargo build
    /// --release`) with NO rebuild guarantee at all — reopening exactly
    /// the staleness bug this function was already fixed for, just for
    /// the other profile.
    ///
    /// UNCONDITIONALLY runs `cargo build` regardless of profile — it does
    /// NOT short-circuit on "the file already exists at this path"
    /// (Dev-B finding, retry 2). An exists-check let every "verified via
    /// the real binary" test silently validate a STALE binary: a
    /// mutation test that forgot to `rm` first would pass against
    /// yesterday's build, and CI's own `Swatinem/rust-cache` persists
    /// `target/` across runs, so a cache restored from before a fix would
    /// do the exact same thing while reporting green. Cargo's own
    /// incremental build is the cache that matters here — a no-op
    /// rebuild when nothing changed is cheap; an exists-check that can
    /// validate stale code is not a cache, it is a correctness bug in the
    /// harness itself.
    pub fn ensure_bin_built(package: &str, bin_name: &str) -> PathBuf {
        let is_release = !cfg!(debug_assertions);
        let profile_dir = if is_release { "release" } else { "debug" };
        let bin_path = workspace_root()
            .join("target")
            .join(profile_dir)
            .join(format!("{}{}", bin_name, std::env::consts::EXE_SUFFIX));

        let mut args = vec!["build", "-p", package, "--bin", bin_name];
        if is_release {
            args.push("--release");
        }
        let status = Command::new("cargo")
            .args(&args)
            .current_dir(workspace_root())
            .status()
            .unwrap_or_else(|_| panic!("failed to build {}", bin_name));
        assert!(status.success(), "{} build failed", bin_name);
        bin_path
    }

    /// Reads the SAME path signal `SynCodeParser::discover_probe_path`
    /// itself relies on (`target/<profile>/deps/<binary>`) to decide whether
    /// `exe_path` was built under `--release`, instead of
    /// `cfg!(debug_assertions)` — which #51's own
    /// `[profile.release] debug-assertions = true` makes `true` in BOTH
    /// profiles, so it can no longer distinguish them (QA retry-1 finding).
    pub fn is_release_exe_path(exe_path: &Path) -> bool {
        unimplemented!("scaffold — RED before GREEN, ticket #51 retry 1")
    }
}
