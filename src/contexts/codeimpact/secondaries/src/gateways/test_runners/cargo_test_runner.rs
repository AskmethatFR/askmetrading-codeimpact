use codeimpact_hexagon::analysis::{AnalysisError, StressTestRun, TestRunnerPort};
use std::time::{Duration, Instant};

const TEST_TIMEOUT: Duration = Duration::from_secs(300);

pub struct CargoTestRunner {
    project_dir: std::path::PathBuf,
}

impl CargoTestRunner {
    pub fn new(project_dir: std::path::PathBuf) -> Self {
        Self { project_dir }
    }

    fn run_cargo_test(
        project_dir: &std::path::Path,
        filter: Option<&str>,
    ) -> Result<StressTestRun, AnalysisError> {
        let start = Instant::now();

        let mut cmd = std::process::Command::new("cargo");
        cmd.arg("test");
        cmd.arg("--lib");
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.current_dir(project_dir);
        cmd.env_clear();
        cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
        cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
        cmd.env("CARGO_HOME", std::env::var("CARGO_HOME").unwrap_or_default());
        cmd.env("RUST_BACKTRACE", std::env::var("RUST_BACKTRACE").unwrap_or_default());
        cmd.env("RUSTUP_HOME", std::env::var("RUSTUP_HOME").unwrap_or_default());
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

        if let Some(f) = filter {
            if !f.is_empty() && f.len() <= 256 {
                cmd.arg("--").arg(f);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| AnalysisError::TestRunnerError(format!("impossible de lancer cargo test: {}", e)))?;

        let _status = loop {
            if start.elapsed() > TEST_TIMEOUT {
                let _ = child.kill();
                return Err(AnalysisError::TestRunnerError(
                    "cargo test a dépassé le timeout de 300s".into(),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    let _ = child.kill();
                    return Err(AnalysisError::TestRunnerError(format!(
                        "cargo test interrompu: {}",
                        e
                    )));
                }
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let output = child
            .wait_with_output()
            .map_err(|e| AnalysisError::TestRunnerError(format!("impossible de lire la sortie: {}", e)))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let cpu_time_ms = Self::parse_cpu_time(&stderr);
        let memory_kb = Self::parse_memory_kb(&stderr);

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

    fn parse_cpu_time(stderr: &str) -> u64 {
        // macOS: "        0.12 real         0.10 user         0.02 sys"
        if let Some(line) = stderr.lines().find(|l| l.trim().contains("user")) {
            if let Some(val) = line.split_whitespace().nth(1) {
                if let Ok(secs) = val.parse::<f64>() {
                    return (secs * 1000.0) as u64;
                }
            }
        }
        // Linux: "User time (seconds): 0.10"
        if let Some(line) = stderr.lines().find(|l| l.trim().starts_with("User time")) {
            if let Some(val) = line.split(':').nth(1) {
                if let Ok(secs) = val.trim().parse::<f64>() {
                    return (secs * 1000.0) as u64;
                }
            }
        }
        0
    }

    fn parse_memory_kb(stderr: &str) -> u64 {
        // macOS: "maximum resident set size (kbytes): 12345"
        if let Some(line) = stderr.lines().find(|l| {
            l.trim()
                .to_lowercase()
                .contains("maximum resident set size")
        }) {
            if let Some(val) = line.split(':').nth(1) {
                if let Ok(kb) = val.trim().parse::<u64>() {
                    return kb;
                }
            }
        }
        // Linux: "Maximum resident set size (kbytes): 12345"
        if let Some(line) = stderr.lines().find(|l| {
            l.trim()
                .to_lowercase()
                .contains("maximum resident set size")
        }) {
            if let Some(val) = line.split(':').nth(1) {
                if let Ok(kb) = val.trim().parse::<u64>() {
                    return kb;
                }
            }
        }
        0
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