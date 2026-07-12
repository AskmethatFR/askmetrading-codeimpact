use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use codeimpact_hexagon::analysis::{
    AnalysisError, Measurement, StressTestRun, TestRunnerPort, UnmeasurableReason,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Bound on a child's buffered stdout/stderr. This repo's own
/// `--workspace --message-format=json` build output is ~70KB — 64MB
/// leaves three orders of magnitude of headroom for a legitimate test
/// binary's output while still protecting the host against a runaway
/// print loop or `--nocapture` (#39 follow-up, Security MEDIUM).
const MAX_CHILD_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

pub struct CargoTestRunner {
    project_dir: std::path::PathBuf,
}

impl CargoTestRunner {
    pub fn new(project_dir: std::path::PathBuf) -> Self {
        Self { project_dir }
    }

    fn time_wrapper_available() -> bool {
        Path::new("/usr/bin/time").exists()
    }

    fn time_flag() -> &'static str {
        if cfg!(target_os = "macos") {
            "-l"
        } else {
            "-v"
        }
    }

    fn apply_sanitized_env(cmd: &mut Command, project_dir: &Path) {
        cmd.current_dir(project_dir);
        cmd.env_clear();
        cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
        cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
        cmd.env(
            "CARGO_HOME",
            std::env::var("CARGO_HOME").unwrap_or_default(),
        );
        cmd.env(
            "RUST_BACKTRACE",
            std::env::var("RUST_BACKTRACE").unwrap_or_default(),
        );
        cmd.env(
            "RUSTUP_HOME",
            std::env::var("RUSTUP_HOME").unwrap_or_default(),
        );
        cmd.env("TMPDIR", std::env::var("TMPDIR").unwrap_or_default());
        cmd.env("USER", std::env::var("USER").unwrap_or_default());
        cmd.env("SHELL", std::env::var("SHELL").unwrap_or_default());
        cmd.env("PWD", std::env::var("PWD").unwrap_or_default());
        if let Ok(ld_path) = std::env::var("LD_LIBRARY_PATH") {
            cmd.env("LD_LIBRARY_PATH", ld_path);
        }
        if let Ok(dyld_path) = std::env::var("DYLD_LIBRARY_PATH") {
            cmd.env("DYLD_LIBRARY_PATH", dyld_path);
        }
        if let Ok(temp) = std::env::var("TEMP") {
            cmd.env("TEMP", temp);
        }
        if let Ok(tmp) = std::env::var("TMP") {
            cmd.env("TMP", tmp);
        }
    }

    fn valid_filter(filter: Option<&str>) -> Option<&str> {
        filter.filter(|f| !f.is_empty() && f.len() <= 256)
    }

    /// Builds the test binary — UNMEASURED. This step is dominated by
    /// `rustc` compilation, not by the code under test, so its cost must
    /// never be attributed to the measured run (#36 bug 2).
    fn build_cmd(project_dir: &Path) -> Command {
        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .arg("--no-run")
            .arg("--workspace")
            .arg("--message-format=json");
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        Self::apply_sanitized_env(&mut cmd, project_dir);
        cmd
    }

    /// Runs the already-compiled test binary directly — this, and only
    /// this, is what gets measured (#36 bug 2).
    fn measure_cmd(
        project_dir: &Path,
        binary: &Path,
        filter: Option<&str>,
        use_time: bool,
    ) -> Command {
        let mut cmd = if use_time {
            let mut c = Command::new("/usr/bin/time");
            c.arg(Self::time_flag());
            c.arg(binary);
            c
        } else {
            Command::new(binary)
        };
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        Self::apply_sanitized_env(&mut cmd, project_dir);

        if let Some(f) = Self::valid_filter(filter) {
            cmd.arg(f);
        }

        cmd
    }

    fn run_with_timeout(cmd: Command) -> Result<(Duration, Output), AnalysisError> {
        Self::run_with_timeout_with_budget(cmd, TEST_TIMEOUT)
    }

    /// Polls for the timeout budget while a child runs, WITHOUT ever
    /// blocking the child on a full OS pipe buffer (~64KB). Draining
    /// stdout/stderr is handed to two dedicated reader threads that run
    /// concurrently with the poll loop — a child that writes past the
    /// buffer threshold (a `--workspace` build's JSON, a chatty test
    /// binary, `--nocapture`) would otherwise block on `write()` forever,
    /// since `try_wait()` never returns while the child is stuck (#39).
    fn run_with_timeout_with_budget(
        mut cmd: Command,
        budget: Duration,
    ) -> Result<(Duration, Output), AnalysisError> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let start = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| AnalysisError::TestRunnerError(format!("impossible de lancer: {}", e)))?;

        let stdout_reader = child
            .stdout
            .take()
            .map(|pipe| Self::spawn_drain_thread(pipe, MAX_CHILD_OUTPUT_BYTES));
        let stderr_reader = child
            .stderr
            .take()
            .map(|pipe| Self::spawn_drain_thread(pipe, MAX_CHILD_OUTPUT_BYTES));

        // Poll for the timeout budget, but ALWAYS fall through to join the
        // reader threads below — on every path, success or failure — so
        // no exit path leaves a reader thread (and its pipe FD) leaked
        // (#39 follow-up, Security MEDIUM/LOW).
        let status_result = loop {
            if start.elapsed() > budget {
                Self::kill_child_and_group(&mut child);
                break Err(AnalysisError::TestRunnerError(
                    "le processus a dépassé le timeout de 300s".into(),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    Self::kill_child_and_group(&mut child);
                    break Err(AnalysisError::TestRunnerError(format!(
                        "processus interrompu: {}",
                        e
                    )));
                }
            }
        };

        let elapsed = start.elapsed();
        let stdout = Self::join_drain_thread(stdout_reader);
        let stderr = Self::join_drain_thread(stderr_reader);

        let status = status_result?;
        Ok((
            elapsed,
            Output {
                status,
                stdout: stdout?,
                stderr: stderr?,
            },
        ))
    }

    /// Kills the child AND (best effort) its whole process group, then
    /// ALWAYS reaps it (`wait()`) so it never lingers as a zombie. Used
    /// identically on the timeout branch and the `try_wait()`-error
    /// branch, so both get the exact same discipline (#39 follow-up,
    /// Security LOW).
    fn kill_child_and_group(child: &mut std::process::Child) {
        #[cfg(unix)]
        Self::kill_process_group(child.id());
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Best-effort defense against a grandchild process inheriting the
    /// pipe's write end and holding it open after the direct child is
    /// killed — that would otherwise block the reader thread's read
    /// forever, waiting for an EOF that never comes (#39 follow-up,
    /// Security MEDIUM). `run_with_timeout_with_budget` places the child
    /// in its own process group (`process_group(0)`); signalling the
    /// NEGATIVE pid here reaches every process in that group, not just
    /// the direct child. This is defense in depth, not a hard guarantee:
    /// a grandchild that calls `setsid()` to leave the group can still
    /// evade it — closing that gap fully needs OS-level cgroup/job-object
    /// confinement, out of scope here. `join_drain_thread`'s bounded wait
    /// is what keeps THAT residual case from hanging the caller forever.
    ///
    /// Uses the `libc` crate rather than a hand-rolled `extern "C"`
    /// binding: `libc::kill`/`libc::pid_t`/`libc::SIGKILL` are generated
    /// and tested against the real platform headers, so a wrong arg
    /// order, wrong signal constant, or ABI drift on a new target is
    /// caught upstream instead of only by human review of a project-
    /// maintained FFI block (#39 follow-up, Security MEDIUM). `libc` adds
    /// no transitive dependencies; the zero-dep rule (ADR-0001/ADR-0005)
    /// binds the hexagon, not `secondaries`, which already depends on
    /// `serde_json`/`tempfile`.
    #[cfg(unix)]
    fn kill_process_group(pid: u32) {
        // Guard the cast: a `-0 == 0` target would signal the CALLER's
        // own process group (i.e. kill `codeimpact` itself). `Child::id()`
        // never actually returns 0 on unix, and no real pid reaches
        // `i32::MAX`, but the guard is cheap insurance against either
        // (#39 follow-up, Security LOW).
        if pid == 0 {
            return;
        }
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return;
        };
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }

    /// Spawns a thread that reads a child's pipe to completion — bounded
    /// by `cap` — off the poll loop's critical path.
    fn spawn_drain_thread(
        pipe: impl std::io::Read + Send + 'static,
        cap: usize,
    ) -> std::thread::JoinHandle<Result<Vec<u8>, AnalysisError>> {
        std::thread::spawn(move || Self::drain_with_cap(pipe, cap))
    }

    /// Joins a reader thread, surfacing BOTH kinds of failure honestly
    /// instead of silently defaulting to an empty buffer: the reader's
    /// own `Err` (cap exceeded, io error) and the thread having panicked.
    /// A read error must not silently become "the child printed nothing"
    /// (#39 follow-up, Security LOW/MEDIUM).
    fn join_drain_thread(
        reader: Option<std::thread::JoinHandle<Result<Vec<u8>, AnalysisError>>>,
    ) -> Result<Vec<u8>, AnalysisError> {
        match reader {
            Some(handle) => handle.join().unwrap_or_else(|_| {
                Err(AnalysisError::TestRunnerError(
                    "le thread de lecture de la sortie du processus a paniqué".into(),
                ))
            }),
            None => Ok(Vec::new()),
        }
    }

    /// Reads `pipe` to completion, honestly bounded: past `cap` bytes,
    /// returns Err rather than silently truncating. A silent truncation
    /// on the BUILD stream would feed `parse_test_binary_paths` a cut-off
    /// JSON stream — fewer binaries measured, no error — exactly the
    /// class of quietly-incomplete result this ticket exists to kill
    /// (#39 follow-up, Security MEDIUM).
    fn drain_with_cap(mut pipe: impl std::io::Read, cap: usize) -> Result<Vec<u8>, AnalysisError> {
        let mut buf = Vec::new();
        (&mut pipe)
            .take(cap as u64 + 1)
            .read_to_end(&mut buf)
            .map_err(|e| {
                AnalysisError::TestRunnerError(format!(
                    "lecture de la sortie du processus impossible: {}",
                    e
                ))
            })?;

        if buf.len() > cap {
            return Err(AnalysisError::TestRunnerError(
                "le processus a produit plus de sortie que la limite autorisée".into(),
            ));
        }

        Ok(buf)
    }

    /// Builds every test binary in the workspace, unmeasured, and returns
    /// their confined paths (#39: a `--workspace` build produces one test
    /// target per member, not one).
    fn build_test_binaries(project_dir: &Path) -> Result<Vec<PathBuf>, AnalysisError> {
        let cmd = Self::build_cmd(project_dir);
        let (_elapsed, output) = Self::run_with_timeout(cmd)?;

        if !output.status.success() {
            return Err(AnalysisError::TestRunnerError(
                "la compilation des tests a échoué".into(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let candidates = Self::parse_test_binary_paths(&stdout);
        if candidates.is_empty() {
            return Err(AnalysisError::TestRunnerError(
                "impossible de localiser le binaire de test compilé".into(),
            ));
        }

        Self::confine_all(project_dir, &candidates)
    }

    /// Parses `cargo ... --message-format=json` output to find every
    /// executable produced for the test profile, in emission order. A
    /// `--workspace` build emits one `compiler-artifact` per test target
    /// across every member — all of them must be collected, not just the
    /// last one (#39: `--lib` + "keep the last" together caused a
    /// multi-crate workspace to measure a single, arbitrary binary).
    fn parse_test_binary_paths(stdout: &str) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for line in stdout.lines() {
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
                continue;
            }
            let is_test_profile = msg
                .get("profile")
                .and_then(|p| p.get("test"))
                .and_then(|t| t.as_bool())
                .unwrap_or(false);
            if !is_test_profile {
                continue;
            }
            if let Some(exe) = msg.get("executable").and_then(|e| e.as_str()) {
                found.push(PathBuf::from(exe));
            }
        }
        found
    }

    /// Confines a candidate binary path to `project_dir/target` before it is
    /// ever executed (#36 retry B3). `cargo ... --message-format=json`
    /// reports an `executable` path that this process later re-executes in a
    /// separate `Command::new(binary)` call — a TOCTOU window a hostile
    /// `.cargo/config.toml` (`[build] target-dir = <outside path>`, pure repo
    /// content, no code execution needed) can steer outside the project.
    /// Mirrors the canonicalize-then-confine discipline `FileSystemCodeReader`
    /// already applies to mere reads (ADR-0006) — executing is strictly more
    /// dangerous than reading, so it gets the same discipline.
    fn confine_to_target_dir(
        project_dir: &Path,
        candidate: &Path,
    ) -> Result<PathBuf, AnalysisError> {
        let locate_err = || {
            AnalysisError::TestRunnerError(
                "impossible de localiser le binaire de test compilé".into(),
            )
        };

        let canonical_target =
            std::fs::canonicalize(project_dir.join("target")).map_err(|_| locate_err())?;
        let canonical_candidate = std::fs::canonicalize(candidate).map_err(|_| locate_err())?;

        if !canonical_candidate.starts_with(&canonical_target) {
            return Err(AnalysisError::TestRunnerError(
                "binaire de test hors du répertoire de build".into(),
            ));
        }

        Ok(canonical_candidate)
    }

    /// Confines every candidate binary from a `--workspace` build (#39).
    /// Fails closed on the first rejection: a hostile candidate does not
    /// get silently dropped while the good ones proceed — the whole run
    /// is rejected, mirroring the single-binary discipline above applied
    /// to a batch.
    fn confine_all(
        project_dir: &Path,
        candidates: &[PathBuf],
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        candidates
            .iter()
            .map(|candidate| Self::confine_to_target_dir(project_dir, candidate))
            .collect()
    }

    /// Thin wrapper: probes for `/usr/bin/time` on the real filesystem and
    /// delegates to the testable inner function. Kept separate so tests can
    /// drive the `use_time = false` (no-sampler) path deterministically,
    /// without depending on the host having (or lacking) `/usr/bin/time`
    /// (#36 retry B2 — mirrors the `measure_cmd(..., use_time: bool)` seam).
    fn measure_test_binary(
        project_dir: &Path,
        binary: &Path,
        filter: Option<&str>,
    ) -> Result<StressTestRun, AnalysisError> {
        let use_time = Self::time_wrapper_available();
        Self::measure_test_binary_with_sampler(project_dir, binary, filter, use_time)
    }

    fn measure_test_binary_with_sampler(
        project_dir: &Path,
        binary: &Path,
        filter: Option<&str>,
        use_time: bool,
    ) -> Result<StressTestRun, AnalysisError> {
        let cmd = Self::measure_cmd(project_dir, binary, filter, use_time);
        let (elapsed, output) = Self::run_with_timeout(cmd)?;

        let duration_ms = elapsed.as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !Self::has_test_summary_line(&stdout) {
            return Err(AnalysisError::TestRunnerError(
                "le binaire de test ne s'est pas terminé normalement".into(),
            ));
        }

        let (cpu_time_ms, memory_kb) = if use_time {
            (
                Self::parse_cpu_time(&stderr)
                    .map(Measurement::Available)
                    .unwrap_or(Measurement::Unmeasurable(UnmeasurableReason::NoSampler)),
                Self::parse_memory_kb(&stderr)
                    .map(Measurement::Available)
                    .unwrap_or(Measurement::Unmeasurable(UnmeasurableReason::NoSampler)),
            )
        } else {
            (
                Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
                Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
            )
        };

        let (tests_passed, tests_total) = Self::parse_test_results(&stdout);

        Ok(StressTestRun::new(
            duration_ms,
            cpu_time_ms,
            memory_kb,
            tests_passed,
            tests_total,
            filter.map(String::from),
        ))
    }

    fn run_cargo_test(
        project_dir: &Path,
        filter: Option<&str>,
    ) -> Result<StressTestRun, AnalysisError> {
        let binaries = Self::build_test_binaries(project_dir)?;
        let runs: Vec<StressTestRun> = binaries
            .iter()
            .map(|binary| Self::measure_test_binary(project_dir, binary, filter))
            .collect::<Result<_, _>>()?;
        StressTestRun::aggregate(&runs)
    }

    /// Sums `user` + `sys` CPU time — kernel time (I/O syscalls, ...) was
    /// previously invisible, in a tool whose selling point includes I/O
    /// detection (#36 bug 2). Returns `None` when no reading could be
    /// parsed, never `0` (#36 bug 1).
    fn parse_cpu_time(stderr: &str) -> Option<u64> {
        let mut total_ms = 0.0_f64;
        let mut found = false;

        // macOS `/usr/bin/time -l`: "0.06 real         0.01 user         0.02 sys"
        for line in stderr.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            for (i, token) in tokens.iter().enumerate() {
                if (*token == "user" || *token == "sys") && i > 0 {
                    if let Ok(secs) = tokens[i - 1].parse::<f64>() {
                        total_ms += secs * 1000.0;
                        found = true;
                    }
                }
            }
        }
        if found {
            return Some(total_ms as u64);
        }

        // Linux `/usr/bin/time -v`: "User time (seconds): 0.10" / "System time (seconds): 0.02"
        for line in stderr.lines() {
            let trimmed = line.trim();
            let is_user = trimmed.starts_with("User time");
            let is_system = trimmed.starts_with("System time");
            if !is_user && !is_system {
                continue;
            }
            if let Some(val) = trimmed.split(':').nth(1) {
                if let Ok(secs) = val.trim().parse::<f64>() {
                    total_ms += secs * 1000.0;
                    found = true;
                }
            }
        }

        found.then_some(total_ms as u64)
    }

    fn parse_memory_kb(stderr: &str) -> Option<u64> {
        for line in stderr.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();
            if lower.contains("maximum resident set size") {
                let val_str = if let Some(val) = trimmed.split(':').nth(1) {
                    // Linux: "Maximum resident set size (kbytes): 12345"
                    val.trim()
                } else {
                    // macOS: "32555008  maximum resident set size"
                    trimmed.split_whitespace().next().unwrap_or("")
                };
                if let Ok(kb) = val_str.parse::<u64>() {
                    // macOS reports bytes, Linux reports KB
                    return Some(if lower.contains("(kbytes)") {
                        kb
                    } else {
                        kb / 1024
                    });
                }
            }
        }
        None
    }

    /// A libtest binary that ran to completion — pass, fail, or zero
    /// tests — always prints a `test result: ...` summary line. A binary
    /// that crashed mid-harness (SIGSEGV, `abort()`, a panic that kills
    /// the process before the summary) prints none. This, not the exit
    /// status, is the discriminator: a binary with FAILING tests exits
    /// non-zero on its ordinary, nominal path and must stay measurable
    /// (#39 follow-up — Dev B).
    fn has_test_summary_line(stdout: &str) -> bool {
        stdout
            .lines()
            .any(|line| line.trim().starts_with("test result"))
    }

    fn parse_test_results(stdout: &str) -> (u32, u32) {
        let mut tests_passed: u32 = 0;
        let mut tests_total: u32 = 0;

        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("test result") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("test ") {
                if rest.contains("FAILED") {
                    tests_total += 1;
                } else if rest.contains("ok") {
                    tests_passed += 1;
                    tests_total += 1;
                }
            }
        }

        (tests_passed, tests_total)
    }
}

impl TestRunnerPort for CargoTestRunner {
    fn run_tests(&self, filter: Option<&str>) -> Result<StressTestRun, AnalysisError> {
        Self::run_cargo_test(&self.project_dir, filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List (build_cmd / measure_cmd — the build/measure seam, #36 bug 2):
    // 1. build_cmd never wraps in /usr/bin/time (unmeasured)
    // 2. build_cmd asks for --no-run (never executes the tests)
    // 3. measure_cmd wraps the binary in /usr/bin/time when a sampler is available
    // 4. measure_cmd runs the binary directly (not `cargo`) when no sampler is available
    // 5. measure_cmd never re-invokes cargo / never rebuilds

    #[test]
    fn build_cmd_is_never_wrapped_in_time() {
        let cmd = CargoTestRunner::build_cmd(Path::new("."));
        assert_eq!(cmd.get_program(), "cargo");
    }

    #[test]
    fn build_cmd_only_compiles_never_runs() {
        let cmd = CargoTestRunner::build_cmd(Path::new("."));
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--no-run".to_string()));
    }

    // #39 — root cause: `--lib` builds only lib test targets, structurally
    // excluding every integration test in `tests/*.rs` (where every real
    // test in this repo lives). `--workspace` must replace it so every
    // member's test targets are built.
    #[test]
    fn build_cmd_builds_every_workspace_member() {
        let cmd = CargoTestRunner::build_cmd(Path::new("."));
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--workspace".to_string()));
        assert!(!args.contains(&"--lib".to_string()));
    }

    #[test]
    fn measure_cmd_wraps_binary_in_time_when_sampler_available() {
        let binary = Path::new("/tmp/fake-test-binary");
        let cmd = CargoTestRunner::measure_cmd(Path::new("."), binary, None, true);
        assert_eq!(cmd.get_program(), "/usr/bin/time");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a.contains("fake-test-binary")));
    }

    #[test]
    fn measure_cmd_runs_binary_directly_when_no_sampler() {
        let binary = Path::new("/tmp/fake-test-binary");
        let cmd = CargoTestRunner::measure_cmd(Path::new("."), binary, None, false);
        assert_eq!(cmd.get_program(), binary);
    }

    #[test]
    fn measure_cmd_never_reinvokes_cargo() {
        let binary = Path::new("/tmp/fake-test-binary");
        let cmd = CargoTestRunner::measure_cmd(Path::new("."), binary, None, true);
        assert_ne!(cmd.get_program(), "cargo");
    }

    // Test List (run_with_timeout — drain stdout/stderr concurrently while
    // polling, #39 deadlock fix): the OS pipe buffer is ~64 KB. Polling
    // try_wait() without ever reading the piped stdout/stderr means a
    // child that writes more than that blocks on write() forever — it can
    // never reach exit, so try_wait() returns None until the budget is
    // exhausted. `--workspace` (this ticket) pushes `cargo test --no-run
    // --message-format=json`'s stdout past that threshold on this repo's
    // own 21 test binaries, so the "fix" for #39 would otherwise hang the
    // tool on its own dogfood run — strictly worse than the bug it set
    // out to fix.
    // 1. a child writing well over 64 KB to stdout must not deadlock the
    //    timeout loop
    // 2. same for stderr — two independent pipes, two independent buffers

    fn big_output_script(dir: &Path, name: &str, stream_redirect: &str) -> PathBuf {
        write_executable_script(
            dir,
            name,
            &format!(
                "#!/bin/sh\nyes x | head -c 200000 {}\nexit 0\n",
                stream_redirect
            ),
        )
    }

    // Test List (drain_with_cap — bounded read, #39 follow-up / Security
    // MEDIUM finding "unbounded in-memory buffering of child output"):
    // 1. output within the cap -> Ok with the full bytes, unchanged
    // 2. output over the cap -> Err, never a silently truncated Ok

    #[test]
    fn drain_with_cap_returns_full_output_within_the_cap() {
        let data = vec![b'x'; 100];
        let cursor = std::io::Cursor::new(data.clone());

        let result = CargoTestRunner::drain_with_cap(cursor, 200);

        assert_eq!(result.expect("100 bytes is within a 200-byte cap"), data);
    }

    #[test]
    fn drain_with_cap_errors_when_output_exceeds_the_cap() {
        let data = vec![b'x'; 300];
        let cursor = std::io::Cursor::new(data);

        let result = CargoTestRunner::drain_with_cap(cursor, 200);

        assert!(
            result.is_err(),
            "300 bytes over a 200-byte cap must be Err, never a silently truncated Ok"
        );
    }

    // Test List (spawn_drain_thread / join_drain_thread — wiring the cap
    // and surfacing failures honestly instead of swallowing them into an
    // empty buffer, #39 follow-up / Security MEDIUM+LOW):
    // 1. spawn_drain_thread enforces the cap it is given (not just the
    //    pure drain_with_cap function in isolation — the actual thread
    //    wiring production code uses)
    // 2. join_drain_thread propagates a reader's Err instead of turning
    //    it into a silent empty Vec
    // 3. join_drain_thread propagates a reader thread PANIC as an Err too
    //    — not just an io/cap error inside the thread

    #[test]
    fn spawn_drain_thread_propagates_the_cap_error() {
        let data = vec![b'x'; 300];
        let cursor = std::io::Cursor::new(data);

        let handle = CargoTestRunner::spawn_drain_thread(cursor, 200);
        let result = CargoTestRunner::join_drain_thread(Some(handle));

        assert!(
            result.is_err(),
            "the thread wiring must enforce the cap it was given, not just the pure function"
        );
    }

    #[test]
    fn join_drain_thread_propagates_a_reader_error() {
        let handle = std::thread::spawn(|| Err(AnalysisError::TestRunnerError("boom".into())));

        let result = CargoTestRunner::join_drain_thread(Some(handle));

        assert!(
            result.is_err(),
            "a reader thread's Err must not be silently swallowed into an empty buffer"
        );
    }

    #[test]
    fn join_drain_thread_propagates_a_reader_panic() {
        let handle = std::thread::spawn(|| -> Result<Vec<u8>, AnalysisError> {
            panic!("simulated reader thread panic");
        });

        let result = CargoTestRunner::join_drain_thread(Some(handle));

        assert!(
            result.is_err(),
            "a reader thread PANIC must not be silently swallowed into an empty buffer"
        );
    }

    #[test]
    fn run_with_timeout_does_not_deadlock_on_large_stdout() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let script = big_output_script(dir.path(), "big_stdout.sh", "");

        let mut cmd = Command::new(&script);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let result = CargoTestRunner::run_with_timeout_with_budget(cmd, Duration::from_secs(10));

        let (_elapsed, output) =
            result.expect("a child writing >64KB to stdout must not deadlock the timeout loop");
        assert_eq!(output.stdout.len(), 200_000);
    }

    #[test]
    fn run_with_timeout_does_not_deadlock_on_large_stderr() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let script = big_output_script(dir.path(), "big_stderr.sh", "1>&2");

        let mut cmd = Command::new(&script);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let result = CargoTestRunner::run_with_timeout_with_budget(cmd, Duration::from_secs(10));

        let (_elapsed, output) =
            result.expect("a child writing >64KB to stderr must not deadlock the timeout loop");
        assert_eq!(output.stderr.len(), 200_000);
    }

    // Test List (run_with_timeout_with_budget — a grandchild holding the
    // pipe open must not hang the join, #39 follow-up / Security MEDIUM):
    // killing only the direct child on timeout is not enough if a
    // grandchild inherited the same stdout/stderr pipe and is still
    // alive — its write end stays open, so the reader thread's read
    // blocks forever waiting for an EOF that never comes. Since every
    // exit path now unconditionally joins the reader threads, THIS
    // exact scenario would hang the whole function forever without a
    // process-group-wide kill. Bounded via a channel + recv_timeout so
    // the test itself can never hang the suite even if the fix regresses.
    // 1. a child that backgrounds a grandchild sharing its pipe, then
    //    itself outlives the budget -> the call still returns (as an
    //    Err, since the budget was exceeded) within a sane wall-clock
    //    bound, not "never"

    #[test]
    fn run_with_timeout_with_budget_does_not_hang_when_a_grandchild_holds_the_pipe_open() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let script = write_executable_script(
            dir.path(),
            "grandchild_holds_pipe.sh",
            "#!/bin/sh\n(sleep 30 &)\nsleep 30\n",
        );

        let mut cmd = Command::new(&script);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = CargoTestRunner::run_with_timeout_with_budget(cmd, Duration::from_secs(1));
            let _ = tx.send(result);
        });

        let result = rx.recv_timeout(Duration::from_secs(15)).expect(
            "run_with_timeout_with_budget did not return within 15s — it is blocked joining \
             a reader thread on a pipe a grandchild process still holds open",
        );

        assert!(
            result.is_err(),
            "expected the 1s budget to be exceeded and reported as an error"
        );
    }

    // Test List (parse_cpu_time — sum user + sys, never default to 0):
    // 1. macOS format: user + sys both present -> summed
    // 2. Linux format: user + sys both present -> summed
    // 3. no recognizable line -> None (never 0)

    #[test]
    fn parse_cpu_time_sums_user_and_sys_macos() {
        let stderr = "        0.06 real         0.01 user         0.02 sys";
        // 0.01 + 0.02 = 0.03s = 30ms
        assert_eq!(CargoTestRunner::parse_cpu_time(stderr), Some(30));
    }

    #[test]
    fn parse_cpu_time_sums_user_and_sys_linux() {
        let stderr = "\tUser time (seconds): 0.10\n\tSystem time (seconds): 0.05";
        // 0.10 + 0.05 = 0.15s = 150ms
        assert_eq!(CargoTestRunner::parse_cpu_time(stderr), Some(150));
    }

    #[test]
    fn parse_cpu_time_unparsable_output_is_none_not_zero() {
        let stderr = "some unrelated tool output";
        assert_eq!(CargoTestRunner::parse_cpu_time(stderr), None);
    }

    // Test List (parse_test_binary_paths — #39, collect EVERY test
    // executable a `--workspace` build produces, in emission order):
    // 1. 3 profile.test == true artifacts -> Vec of all 3, in order
    // 2. profile.test == false / executable: null -> both excluded
    // 3. no matching artifact -> [] (not a phantom entry)

    #[test]
    fn parse_test_binary_paths_collects_every_test_executable() {
        let stdout = r#"{"reason":"compiler-artifact","profile":{"test":true},"executable":"/tmp/target/debug/deps/alpha-abc123"}
{"reason":"compiler-artifact","profile":{"test":true},"executable":"/tmp/target/debug/deps/beta-def456"}
{"reason":"compiler-artifact","profile":{"test":true},"executable":"/tmp/target/debug/deps/gamma-ghi789"}
{"reason":"build-finished","success":true}"#;
        assert_eq!(
            CargoTestRunner::parse_test_binary_paths(stdout),
            vec![
                PathBuf::from("/tmp/target/debug/deps/alpha-abc123"),
                PathBuf::from("/tmp/target/debug/deps/beta-def456"),
                PathBuf::from("/tmp/target/debug/deps/gamma-ghi789"),
            ]
        );
    }

    #[test]
    fn parse_test_binary_paths_ignores_non_test_artifacts() {
        let stdout = r#"{"reason":"compiler-artifact","profile":{"test":false},"executable":"/tmp/target/debug/deps/dep-xyz"}
{"reason":"compiler-artifact","profile":{"test":true},"executable":null}"#;
        assert_eq!(
            CargoTestRunner::parse_test_binary_paths(stdout),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn parse_test_binary_paths_is_empty_when_no_test_artifact() {
        let stdout = r#"{"reason":"build-finished","success":true}"#;
        assert_eq!(
            CargoTestRunner::parse_test_binary_paths(stdout),
            Vec::<PathBuf>::new()
        );
    }

    // Test List (parse_memory_kb — bytes on macOS vs kbytes on Linux, never
    // default to 0, #36 retry B1):
    // 1. macOS format: bytes -> converted to KB
    // 2. Linux format: "(kbytes)" -> used directly, no conversion
    // 3. no recognizable line -> None (never 0)

    #[test]
    fn parse_memory_kb_converts_bytes_to_kb_macos() {
        let stderr = "  2097152  maximum resident set size";
        // 2097152 bytes / 1024 = 2048 KB
        assert_eq!(CargoTestRunner::parse_memory_kb(stderr), Some(2048));
    }

    #[test]
    fn parse_memory_kb_uses_kbytes_directly_linux() {
        let stderr = "\tMaximum resident set size (kbytes): 12345";
        assert_eq!(CargoTestRunner::parse_memory_kb(stderr), Some(12345));
    }

    #[test]
    fn parse_memory_kb_unparsable_output_is_none_not_zero() {
        let stderr = "some unrelated tool output";
        assert_eq!(CargoTestRunner::parse_memory_kb(stderr), None);
    }

    // Test List (measure_test_binary_with_sampler — the real no-sampler
    // path, #36 retry B2): the acceptance criterion QA's rejected mutation
    // proved missing.
    // 1. use_time = false -> cpu_time_ms() and memory_kb() are both
    //    Unmeasurable(NoSampler), never Available(0)

    fn write_executable_script(dir: &Path, name: &str, contents: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join(name);
        std::fs::write(&script, contents).expect("write fake test binary");
        let mut perms = std::fs::metadata(&script)
            .expect("read fake test binary metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("make fake test binary executable");
        script
    }

    // Test List (confine_to_target_dir — reject an executable path outside
    // project_dir/target, #36 retry B3 / TOCTOU hardening):
    // 1. candidate inside project_dir/target -> accepted
    // 2. candidate outside project_dir/target -> rejected
    // 3. the rejection error message leaks no path (ADR-0006)
    // 4. project_dir/target does not exist -> rejected (fail-closed; this is
    //    exactly the state a hostile `.cargo/config.toml` target-dir redirect
    //    produces, #36 retry P3)
    // 5. candidate does not exist on disk -> rejected (#36 retry P3)

    #[test]
    fn confine_to_target_dir_accepts_path_inside_target() {
        let project = tempfile::tempdir().expect("create temp dir");
        let target_dir = project.path().join("target/debug/deps");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let binary = target_dir.join("mycrate-abc123");
        std::fs::write(&binary, b"").expect("write fake binary");

        let result = CargoTestRunner::confine_to_target_dir(project.path(), &binary);

        assert!(result.is_ok());
    }

    #[test]
    fn confine_to_target_dir_rejects_path_outside_target() {
        let project = tempfile::tempdir().expect("create temp dir");
        std::fs::create_dir_all(project.path().join("target")).expect("create target dir");
        let outside = tempfile::tempdir().expect("create temp dir");
        let outside_binary = outside.path().join("evil-binary");
        std::fs::write(&outside_binary, b"").expect("write fake binary");

        let result = CargoTestRunner::confine_to_target_dir(project.path(), &outside_binary);

        assert!(result.is_err());
    }

    #[test]
    fn confine_to_target_dir_rejection_message_leaks_no_path() {
        let project = tempfile::tempdir().expect("create temp dir");
        std::fs::create_dir_all(project.path().join("target")).expect("create target dir");
        let outside = tempfile::tempdir().expect("create temp dir");
        let outside_binary = outside.path().join("evil-binary");
        std::fs::write(&outside_binary, b"").expect("write fake binary");

        let err = CargoTestRunner::confine_to_target_dir(project.path(), &outside_binary)
            .expect_err("should be rejected");

        let message = err.to_string();
        assert!(!message.contains(&outside_binary.to_string_lossy().to_string()));
        assert!(!message.contains(&outside.path().to_string_lossy().to_string()));
    }

    #[test]
    fn confine_to_target_dir_rejects_when_target_dir_does_not_exist() {
        let project = tempfile::tempdir().expect("create temp dir");
        // Deliberately do NOT create project/target: this is exactly the
        // state a hostile `.cargo/config.toml` (`[build] target-dir =
        // <outside path>`) redirect produces — the real binary was built
        // elsewhere, so `project_dir/target` never exists. The fail-closed
        // path must reject, not silently proceed with the raw candidate.
        let candidate = project.path().join("some-binary");
        std::fs::write(&candidate, b"").expect("write fake binary");

        let err = CargoTestRunner::confine_to_target_dir(project.path(), &candidate)
            .expect_err("missing target dir must be rejected");

        let message = err.to_string();
        assert!(!message.contains(&candidate.to_string_lossy().to_string()));
    }

    #[test]
    fn confine_to_target_dir_rejects_when_candidate_does_not_exist() {
        let project = tempfile::tempdir().expect("create temp dir");
        std::fs::create_dir_all(project.path().join("target")).expect("create target dir");
        let missing_candidate = project.path().join("target/debug/deps/does-not-exist");

        let result = CargoTestRunner::confine_to_target_dir(project.path(), &missing_candidate);

        assert!(result.is_err());
    }

    // Test List (confine_all — batch confinement across N binaries, #39):
    // 16. every candidate inside target -> Ok with N canonical paths
    // 17. one candidate outside target -> Err, the whole run rejected
    // 18. the rejection error message leaks no path (ADR-0006)

    #[test]
    fn confine_all_accepts_every_candidate_inside_target() {
        let project = tempfile::tempdir().expect("create temp dir");
        let target_dir = project.path().join("target/debug/deps");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let binary_a = target_dir.join("alpha-abc123");
        let binary_b = target_dir.join("beta-def456");
        std::fs::write(&binary_a, b"").expect("write fake binary");
        std::fs::write(&binary_b, b"").expect("write fake binary");

        let result =
            CargoTestRunner::confine_all(project.path(), &[binary_a.clone(), binary_b.clone()]);

        let confined = result.expect("both candidates are inside target");
        assert_eq!(confined.len(), 2);
    }

    #[test]
    fn confine_all_rejects_the_whole_run_when_any_candidate_is_outside_target() {
        let project = tempfile::tempdir().expect("create temp dir");
        let target_dir = project.path().join("target/debug/deps");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let inside = target_dir.join("alpha-abc123");
        std::fs::write(&inside, b"").expect("write fake binary");
        let outside_dir = tempfile::tempdir().expect("create temp dir");
        let outside = outside_dir.path().join("evil-binary");
        std::fs::write(&outside, b"").expect("write fake binary");

        let result = CargoTestRunner::confine_all(project.path(), &[inside, outside]);

        assert!(
            result.is_err(),
            "expected the whole run to be rejected when any candidate is outside target"
        );
    }

    #[test]
    fn confine_all_rejection_message_leaks_no_path() {
        let project = tempfile::tempdir().expect("create temp dir");
        let target_dir = project.path().join("target/debug/deps");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let inside = target_dir.join("alpha-abc123");
        std::fs::write(&inside, b"").expect("write fake binary");
        let outside_dir = tempfile::tempdir().expect("create temp dir");
        let outside = outside_dir.path().join("evil-binary");
        std::fs::write(&outside, b"").expect("write fake binary");

        let err = CargoTestRunner::confine_all(project.path(), &[inside, outside.clone()])
            .expect_err("should be rejected");

        let message = err.to_string();
        assert!(!message.contains(&outside.to_string_lossy().to_string()));
        assert!(!message.contains(&outside_dir.path().to_string_lossy().to_string()));
    }

    #[test]
    fn measure_test_binary_with_sampler_no_sampler_yields_unmeasurable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let binary = write_executable_script(
            dir.path(),
            "fake_test_binary.sh",
            "#!/bin/sh\necho 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\nexit 0\n",
        );

        let result =
            CargoTestRunner::measure_test_binary_with_sampler(dir.path(), &binary, None, false)
                .expect("measure should succeed");

        assert_eq!(result.cpu_time_ms().available(), None);
        assert_eq!(result.memory_kb().available(), None);
    }

    // Test List (measure_test_binary_with_sampler — crash detection, #39
    // follow-up / Dev B blocking finding): a crashed test binary (SIGSEGV,
    // abort(), a panic that kills the harness before the summary line)
    // prints some "test <name> ... ok" lines and then NOTHING — no "test
    // result:" summary. Without a completeness check, that is
    // indistinguishable from "this binary legitimately has 0 remaining
    // tests" and silently dilutes into a healthy-looking aggregate
    // (exactly #39 regenerated). The discriminator is the summary line,
    // NOT the exit status — a binary with FAILING tests exits non-zero on
    // the nominal path and must stay measurable.
    // 1. no "test result:" line in stdout -> Err (the binary never
    //    finished, whatever its exit code)
    // 2. a "test result: FAILED. ..." line present, exit code non-zero
    //    (the ordinary failing-tests path) -> still Ok, with the parsed
    //    counts — this is what stops a naive `status.success()` "fix"

    #[test]
    fn measure_test_binary_with_sampler_errors_when_binary_crashes_without_summary_line() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let binary = write_executable_script(
            dir.path(),
            "crashes_mid_run.sh",
            "#!/bin/sh\necho 'test foo ... ok'\necho 'test bar ... ok'\nexit 134\n",
        );

        let result =
            CargoTestRunner::measure_test_binary_with_sampler(dir.path(), &binary, None, false);

        assert!(
            result.is_err(),
            "a binary with no 'test result:' summary line must not be trusted, \
             even though it printed some 'test ... ok' lines before dying"
        );
    }

    #[test]
    fn measure_test_binary_with_sampler_still_succeeds_when_tests_fail_but_complete() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let binary = write_executable_script(
            dir.path(),
            "fails_but_completes.sh",
            "#!/bin/sh\necho 'test foo ... ok'\necho 'test bar ... FAILED'\necho 'test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'\nexit 1\n",
        );

        let result =
            CargoTestRunner::measure_test_binary_with_sampler(dir.path(), &binary, None, false)
                .expect(
                    "a binary that completes with failing tests (non-zero exit, summary \
                     line present) must still be measurable",
                );

        assert_eq!(result.tests_passed(), 1);
        assert_eq!(result.tests_total(), 2);
    }
}
