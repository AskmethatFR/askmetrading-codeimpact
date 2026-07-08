use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ensure_binary() -> PathBuf {
    let binary = workspace_root()
        .join("target")
        .join("debug")
        .join("codeimpact");
    if !binary.exists() {
        // Build the binary first
        let status = Command::new("cargo")
            .args(["build", "-p", "codeimpact-primaries"])
            .current_dir(workspace_root())
            .status()
            .expect("Failed to build binary");
        assert!(status.success(), "Binary build failed");
    }
    binary
}

fn fixtures_dir() -> PathBuf {
    workspace_root().join("tests").join("fixtures")
}

#[test]
fn e2e_analyze_valid_file_returns_zero_exit() {
    let binary = ensure_binary();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Expected exit 0, got {}. stdout: {} stderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Complexité"),
        "Output should contain 'Complexité': {}",
        stdout
    );
    assert!(
        stdout.contains("low")
            || stdout.contains("moderate")
            || stdout.contains("high")
            || stdout.contains("critical"),
        "Output should contain a complexity level: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_nonexistent_file_returns_exit_1() {
    let binary = ensure_binary();
    let output = Command::new(binary)
        .args(["analyze", "/tmp/nonexistent_file_12345.rs"])
        .output()
        .expect("Failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "Expected exit != 0");
    assert!(
        stderr.contains("erreur")
            || stderr.contains("error")
            || stderr.contains("introuvable")
            || stderr.contains("trouvé"),
        "stderr should contain error message: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_empty_file_returns_complexity_1_low() {
    let binary = ensure_binary();
    let empty_fixture = fixtures_dir().join("empty.rs");
    // Write a temporary empty file
    std::fs::write(&empty_fixture, "").expect("Failed to write empty fixture");

    let output = Command::new(binary)
        .args(["analyze", empty_fixture.to_str().unwrap()])
        .output()
        .expect("Failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Clean up
    let _ = std::fs::remove_file(&empty_fixture);

    assert!(
        output.status.success(),
        "Expected exit 0, got {}",
        output.status
    );
    assert!(
        stdout.contains("Complexité: 1"),
        "Expected complexity 1 for empty file: {}",
        stdout
    );
    assert!(
        stdout.contains("low"),
        "Expected level 'low' for empty file: {}",
        stdout
    );
}
