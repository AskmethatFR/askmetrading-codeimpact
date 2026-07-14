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
    // D3 (#50 slice S4): an empty file parses to zero functions, so
    // complexity_level() now correctly reads "none" ("nothing to
    // measure"), not a fabricated "low" — even though the file-level base
    // complexity (the "+1") is honestly 1. (stdout still separately
    // contains "low" from the unrelated EconomicImpact::level() line —
    // asserting the exact "Niveau: none" text keeps this test honest about
    // what it actually pins.)
    assert!(
        stdout.contains("Niveau: none"),
        "expected complexity level none for an empty file (nothing measured): {}",
        stdout
    );
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

#[test]
fn e2e_analyze_json_format_outputs_valid_json() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected for --format json. stdout: {}",
        stdout
    );

    // Parse JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");

    // Check schema fields
    assert_eq!(
        json["tool"]["name"], "codeimpact",
        "tool name should be codeimpact"
    );
    assert!(
        json["tool"]["version"].is_string(),
        "version should be present"
    );
    assert!(json["timestamp"].is_string(), "timestamp should be present");
    assert_eq!(json["target_type"], "file", "target_type should be file");
    assert!(
        json["metrics"]["cyclomatic_complexity"].is_number(),
        "cyclomatic_complexity should be present"
    );
    assert!(
        json["metrics"]["transitive_complexity"].is_number(),
        "transitive_complexity should be present"
    );
}

#[test]
fn e2e_analyze_default_format_is_console() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit 0 expected");
    // Default output should be console format (French text)
    assert!(
        stdout.contains("Complexité directe"),
        "default format should be console: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_invalid_format_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--format", "invalid"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "exit non-zero expected for invalid format"
    );
    assert!(
        stderr.contains("invalide") || stderr.contains("erreur"),
        "stderr should contain error about invalid format: {}",
        stderr
    );
}

// US7 T1 — HTML report walking skeleton.
// Test List:
// 1. --format html -o <path> on a project dir writes a self-contained HTML file showing the project view (RED first — behavioral, pins the user-observable outcome)
// 2. --format html on a single-file target errors (T1 scope: project view only)

#[test]
fn e2e_analyze_html_format_writes_self_contained_project_view() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output_path =
        std::env::temp_dir().join(format!("codeimpact_report_{}.html", std::process::id()));
    let _ = std::fs::remove_file(&output_path);

    let output = Command::new(binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--format",
            "html",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected for --format html. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let html =
        std::fs::read_to_string(&output_path).expect("html output file should have been created");
    let _ = std::fs::remove_file(&output_path);

    assert!(
        html.contains("<!DOCTYPE html>"),
        "missing doctype: {}",
        html
    );
    assert_eq!(
        html.matches("<html").count(),
        1,
        "expected a single html root"
    );
    assert!(
        !html.contains("<link "),
        "self-contained report must not reference an external stylesheet: {}",
        html
    );
    assert!(
        !html.contains("<script src="),
        "self-contained report must not reference an external script: {}",
        html
    );
    assert!(
        html.contains("sample.rs"),
        "project view should list the analyzed files: {}",
        html
    );
}

#[test]
fn e2e_analyze_html_format_on_single_file_target_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");

    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--format", "html"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "html format on a single-file target should fail (T1 scope: project view only). stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
    assert!(
        !stderr.contains(fixture.to_str().unwrap()),
        "error message must not leak the absolute path (ADR-0006): {}",
        stderr
    );
}

// QA branch-coverage gap fixes (US7 T1, still T1 — no new behavior):
// 3. --format html without -o defaults the output path to ./report.html (relative to cwd)
// 4. -o pointing at a nonexistent parent directory fails cleanly, no absolute path leak (ADR-0006)

#[test]
fn e2e_analyze_html_format_without_output_flag_defaults_to_report_html_in_cwd() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let isolated_cwd = std::env::temp_dir().join(format!(
        "codeimpact_default_output_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&isolated_cwd).expect("create isolated cwd");
    let expected_path = isolated_cwd.join("report.html");
    let _ = std::fs::remove_file(&expected_path);

    let output = Command::new(&binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--format",
            "html",
        ])
        .current_dir(&isolated_cwd)
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected without -o. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        expected_path.exists(),
        "default output path ./report.html should exist in cwd {:?}",
        isolated_cwd
    );

    let _ = std::fs::remove_file(&expected_path);
    let _ = std::fs::remove_dir(&isolated_cwd);
}

#[test]
fn e2e_analyze_html_format_output_to_nonexistent_dir_errors_without_path_leak() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let bogus_output = "/nonexistent_dir_xyz/report.html";

    let output = Command::new(binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--format",
            "html",
            "-o",
            bogus_output,
        ])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "exit non-zero expected when the output directory does not exist. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
    assert!(
        !stderr.contains("/nonexistent_dir_xyz"),
        "error message must not leak the requested absolute path (ADR-0006): {}",
        stderr
    );
}

// #47 retry 2 — the parser was blind to non-I/O calls nested in a loop
// (`calls_in_loops` only ever held I/O calls, despite its name), so
// QuadraticLoop could never fire on the actual bug it targets: a loop
// calling another loop-having function. Both checks below must hold, or
// the fix is only half done (retry 1 satisfied the first and silently
// broke the second — the false positive was closed by making the
// detector blind to true positives too).
// Test List:
// 1. process_items/validate fixture (regular, non-I/O nested call) -> 1
//    CRITICAL QuadraticLoop, function process_items.
// 2. the real view_model.rs source (aggregate is a self-recursive tree
//    descent, build_tree's calls to sort_children/aggregate are
//    sequential, not nested in its own loop) -> 0 QuadraticLoop warnings.

#[test]
fn e2e_analyze_quadratic_loop_fixture_reports_one_critical_quadratic_loop() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("quadratic_loop.rs");
    let output = Command::new(&binary)
        .args(["analyze", fixture.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    let warnings = json["metrics"]["warnings"]
        .as_array()
        .expect("warnings should be an array");
    let quadratic: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["pattern"] == "QuadraticLoop")
        .collect();

    assert_eq!(
        quadratic.len(),
        1,
        "expected exactly 1 QuadraticLoop warning, got: {:#?}",
        warnings
    );
    assert_eq!(quadratic[0]["severity"], "Critical");
    assert_eq!(quadratic[0]["function"], "process_items");
}

#[test]
fn e2e_analyze_view_model_reports_no_quadratic_loop_warnings() {
    let binary = binary_path();
    let view_model = workspace_root()
        .join("src")
        .join("contexts")
        .join("codeimpact")
        .join("secondaries")
        .join("src")
        .join("gateways")
        .join("report_writers")
        .join("html")
        .join("view_model.rs");
    assert!(
        view_model.exists(),
        "expected view_model.rs to exist at {:?}",
        view_model
    );

    let output = Command::new(&binary)
        .args(["analyze", view_model.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    let warnings = json["metrics"]["warnings"]
        .as_array()
        .expect("warnings should be an array");
    let quadratic: Vec<&serde_json::Value> = warnings
        .iter()
        .filter(|w| w["pattern"] == "QuadraticLoop")
        .collect();

    assert!(
        quadratic.is_empty(),
        "view_model.rs's aggregate (self-recursive tree descent) and \
         build_tree (sequential, non-nested calls to sort_children/aggregate) \
         must not trigger QuadraticLoop: {:#?}",
        quadratic
    );
}

// Issue #48 — `--format json -o <file>` silently ignored `-o`: JSON was always
// printed to stdout and the requested file was never created, no warning, no
// error. HTML already honors `-o` (see the tests above); JSON must too, and
// `-o` must behave consistently across every format.
// Test List:
// 1. --format json -o <path> writes the file; content is valid JSON and matches
//    the expected shape (real RED first — reproduces the reported bug).
// 2. --format json -o <path> does NOT also dump the JSON to stdout (consistent
//    with the HTML branch, which prints a confirmation line, not the document).
// 3. --format json -o <nonexistent-parent> fails cleanly, non-zero exit, no
//    absolute path leak (ADR-0006) — same contract as the HTML branch.
// 4. --format console -o <path> must not silently ignore -o either: it errors
//    explicitly (console writes straight to stdout as a stream; wiring it to
//    a file is a separate, larger port change — out of scope here per the
//    architecture note. The consistent, non-silent choice is a clear refusal).

#[test]
fn e2e_analyze_json_format_with_output_writes_file() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output_path =
        std::env::temp_dir().join(format!("codeimpact_report_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&output_path);

    let output = Command::new(binary)
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--format",
            "json",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "exit 0 expected for --format json -o. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("\"tool\""),
        "when -o is given, the JSON document should not also be dumped to stdout: {}",
        stdout
    );

    let content =
        std::fs::read_to_string(&output_path).expect("json output file should have been created");
    let _ = std::fs::remove_file(&output_path);

    let json: serde_json::Value =
        serde_json::from_str(&content).expect("file content should be valid JSON");
    assert_eq!(json["tool"]["name"], "codeimpact");
    assert_eq!(json["target_type"], "file");
}

#[test]
fn e2e_analyze_json_format_output_to_nonexistent_dir_errors_without_path_leak() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let bogus_output = "/nonexistent_dir_xyz/report.json";

    let output = Command::new(binary)
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--format",
            "json",
            "-o",
            bogus_output,
        ])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "exit non-zero expected when the output directory does not exist. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
    assert!(
        !stderr.contains("/nonexistent_dir_xyz"),
        "error message must not leak the requested absolute path (ADR-0006): {}",
        stderr
    );
}

#[test]
fn e2e_analyze_console_format_with_output_flag_errors_instead_of_silently_ignoring() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output_path = std::env::temp_dir().join(format!(
        "codeimpact_console_output_test_{}.txt",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output_path);

    let output = Command::new(binary)
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--format",
            "console",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "console format with -o should fail explicitly rather than silently ignore -o. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
    assert!(
        !output_path.exists(),
        "-o must not be silently ignored: no file should have been created for console format"
    );
}
