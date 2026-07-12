use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use codeimpact_hexagon::analysis::{
    AnalysisError, Measurement, StressTestRun, TestRunnerPort, UnmeasurableReason,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(300);

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
            .arg("--lib")
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

    fn run_with_timeout(mut cmd: Command) -> Result<(Duration, Output), AnalysisError> {
        let start = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| AnalysisError::TestRunnerError(format!("impossible de lancer: {}", e)))?;

        loop {
            if start.elapsed() > TEST_TIMEOUT {
                let _ = child.kill();
                return Err(AnalysisError::TestRunnerError(
                    "le processus a dépassé le timeout de 300s".into(),
                ));
            }
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    let _ = child.kill();
                    return Err(AnalysisError::TestRunnerError(format!(
                        "processus interrompu: {}",
                        e
                    )));
                }
            }
        }

        let elapsed = start.elapsed();
        let output = child.wait_with_output().map_err(|e| {
            AnalysisError::TestRunnerError(format!("impossible de lire la sortie: {}", e))
        })?;
        Ok((elapsed, output))
    }

    /// Builds the test binary, unmeasured, and returns its path.
    fn build_test_binary(project_dir: &Path) -> Result<PathBuf, AnalysisError> {
        let cmd = Self::build_cmd(project_dir);
        let (_elapsed, output) = Self::run_with_timeout(cmd)?;

        if !output.status.success() {
            return Err(AnalysisError::TestRunnerError(
                "la compilation des tests a échoué".into(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_test_binary_path(&stdout).ok_or_else(|| {
            AnalysisError::TestRunnerError(
                "impossible de localiser le binaire de test compilé".into(),
            )
        })
    }

    /// Parses `cargo ... --message-format=json` output to find the
    /// executable produced for the test profile. Takes the last match:
    /// dependencies may also emit compiler-artifact messages, but the
    /// crate's own test binary is emitted last.
    fn parse_test_binary_path(stdout: &str) -> Option<PathBuf> {
        let mut found = None;
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
                found = Some(PathBuf::from(exe));
            }
        }
        found
    }

    fn measure_test_binary(
        project_dir: &Path,
        binary: &Path,
        filter: Option<&str>,
    ) -> Result<StressTestRun, AnalysisError> {
        let use_time = Self::time_wrapper_available();
        let cmd = Self::measure_cmd(project_dir, binary, filter, use_time);
        let (elapsed, output) = Self::run_with_timeout(cmd)?;

        let duration_ms = elapsed.as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

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
        let binary = Self::build_test_binary(project_dir)?;
        Self::measure_test_binary(project_dir, &binary, filter)
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

    // Test List (parse_test_binary_path):
    // 1. finds the executable of the compiler-artifact with profile.test == true
    // 2. ignores non-test compiler-artifact messages (e.g. build-script, deps)
    // 3. no matching artifact -> None

    #[test]
    fn parse_test_binary_path_finds_test_executable() {
        let stdout = r#"{"reason":"compiler-artifact","profile":{"test":false},"executable":null}
{"reason":"compiler-artifact","profile":{"test":true},"executable":"/tmp/target/debug/deps/mycrate-abc123"}
{"reason":"build-finished","success":true}"#;
        assert_eq!(
            CargoTestRunner::parse_test_binary_path(stdout),
            Some(PathBuf::from("/tmp/target/debug/deps/mycrate-abc123"))
        );
    }

    #[test]
    fn parse_test_binary_path_ignores_non_test_artifacts() {
        let stdout = r#"{"reason":"compiler-artifact","profile":{"test":false},"executable":"/tmp/target/debug/deps/dep-xyz"}"#;
        assert_eq!(CargoTestRunner::parse_test_binary_path(stdout), None);
    }

    #[test]
    fn parse_test_binary_path_none_when_no_artifact() {
        let stdout = r#"{"reason":"build-finished","success":true}"#;
        assert_eq!(CargoTestRunner::parse_test_binary_path(stdout), None);
    }
}
