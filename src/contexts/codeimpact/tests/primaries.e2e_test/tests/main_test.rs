use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.pop();
    path.pop();
    path.pop();
    path
}

fn binary_path() -> PathBuf {
    let bin = workspace_root()
        .join("target")
        .join("debug")
        .join("codeimpact");
    if !bin.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "codeimpact_primaries"])
            .current_dir(workspace_root())
            .status()
            .expect("failed to build binary");
        assert!(status.success(), "binary build failed");
    }
    bin
}

fn fixtures_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path
}

#[test]
fn e2e_analyze_valid_file_exits_0() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("Complexité directe"),
        "missing complexity: {}",
        stdout
    );
    assert!(
        stdout.contains("low")
            || stdout.contains("moderate")
            || stdout.contains("high")
            || stdout.contains("critical"),
        "missing level: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_nonexistent_file_exits_1() {
    let binary = binary_path();
    let output = Command::new(binary)
        .args(["analyze", "/tmp/nonexistent_file_12345.rs"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "exit non-zero expected");
    assert!(
        stderr.contains("introuvable") || stderr.contains("erreur"),
        "stderr should contain error: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_directory_exits_0() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args(["analyze", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected for directory. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn e2e_analyze_with_path_option_exits_0() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", "--path", fixture.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected with --path. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("Complexité directe"),
        "missing complexity: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_empty_file_returns_complexity_1() {
    let binary = binary_path();
    let empty = fixtures_dir().join("empty.rs");
    std::fs::write(&empty, "").expect("write empty fixture");

    let output = Command::new(binary)
        .args(["analyze", empty.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let _ = std::fs::remove_file(&empty);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "exit 0 expected");
    assert!(
        stdout.contains("Complexité directe: 1"),
        "expected complexity 1: {}",
        stdout
    );
    assert!(stdout.contains("low"), "expected level low: {}", stdout);
}

#[test]
fn e2e_help_shows_stress_test_subcommand() {
    let binary = binary_path();
    let output = Command::new(binary)
        .args(["--help"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit 0 expected");
    assert!(
        stdout.contains("stress-test"),
        "help should list stress-test: {}",
        stdout
    );
}

#[test]
fn e2e_stress_test_help_shows_filter_option() {
    let binary = binary_path();
    let output = Command::new(binary)
        .args(["stress-test", "--help"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit 0 expected");
    assert!(
        stdout.contains("--filter"),
        "stress-test help should show --filter: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_sample_contains_io_in_loop() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("I/O dans boucle"),
        "expected I/O dans boucle in output: {}",
        stdout
    );
}
