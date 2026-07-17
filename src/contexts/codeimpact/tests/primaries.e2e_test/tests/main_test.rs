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
    // The CLI now shells out to codeimpact-parse-probe (#63) for every
    // parse — it must sit next to the CLI binary (sibling discovery, D2)
    // whenever an e2e test invokes `codeimpact analyze`.
    ensure_probe_built();
    bin
}

fn ensure_probe_built() {
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

/// Writes `content` to an isolated temp file and returns its path. Used to
/// materialize pathologically-nested fixtures (#63) without checking huge
/// generated files into the repo.
fn write_temp_fixture(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("codeimpact_e2e_{}_{}.rs", name, std::process::id()));
    std::fs::write(&path, content).expect("failed to write temp fixture");
    path
}

/// `mod m0 { mod m1 { ... fn f() {} ... } }` nested `depth` levels deep —
/// the #63 pre-scan bypass class (~1800 nested `mod` is enough to overflow
/// `syn::parse_file`'s recursive-descent stack).
fn nested_mods_source(depth: usize) -> String {
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&format!("mod m{} {{\n", i));
    }
    src.push_str("fn f() {}\n");
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src
}

/// `fn f(x: Vec<Vec<...i32...>>) {}` nested `depth` levels deep — cycle-1
/// bypass vector #1 (nested generic types survive a byte-level brace scan).
fn nested_vec_type_source(depth: usize) -> String {
    let mut ty = String::from("i32");
    for _ in 0..depth {
        ty = format!("Vec<{}>", ty);
    }
    format!("fn f(x: {}) {{}}\n", ty)
}

/// `let _ = !!!!...x;` with `depth` leading `!` — cycle-1 bypass vector #2
/// (chained unary negation survives the same byte-level scan).
fn nested_not_chain_source(depth: usize) -> String {
    let mut expr = String::from("x");
    for _ in 0..depth {
        expr = format!("!{}", expr);
    }
    format!("fn f() {{ let x = true; let _ = {}; }}\n", expr)
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

// ── #63 — pathologically nested source must never crash the process ──
//
// Test List (one behavior, three fixtures — same reason expected, AC2):
//   1. Nested `mod` (the ticket's own repro).
//   2. `Vec<Vec<...>>` nesting (cycle-1 bypass vector #1).
//   3. `!!!!...x` chain (cycle-1 bypass vector #2).
// Collapsed into one parameterized cycle (three rows, same behavior):
// exit 1, no crash, stderr names the "trop complexe" reason (AC1). Depths
// bumped with margin (retry 2, Security informational): this e2e test
// always exercises the DEBUG `codeimpact` binary regardless of this test
// harness's own build profile (binary_path() hardcodes target/debug), so
// it was never itself profile-fragile — bumped anyway for consistency
// with parse_probe_pathological_test.rs's fixtures, which ARE.
#[test]
fn e2e_analyze_pathological_source_is_unmeasured_not_crashed() {
    let binary = binary_path();
    let cases: [(&str, String); 3] = [
        ("nested_mods", nested_mods_source(30000)),
        ("nested_vec", nested_vec_type_source(30000)),
        ("nested_not", nested_not_chain_source(60000)),
    ];

    for (name, source) in &cases {
        let fixture = write_temp_fixture(name, source);
        let output = Command::new(&binary)
            .args(["analyze", fixture.to_str().unwrap()])
            .output()
            .expect("failed to execute binary");
        let _ = std::fs::remove_file(&fixture);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "case {}: expected a clean exit 1 (never a crash/signal). stderr: {}",
            name,
            stderr
        );
        assert!(
            stderr.contains("trop complexe"),
            "case {}: expected the SourceTooComplex reason in stderr, got: {}",
            name,
            stderr
        );
    }
}

// ── #63 T2 (AC3) — a project scan tolerates ONE pathological file ──
//
// `analyze --path <dir>` over a mix of healthy and pathological files must
// finish, report the pathological file unmeasured with its reason, and
// still measure the healthy ones.
#[test]
fn e2e_analyze_path_with_one_pathological_file_still_completes() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_project_scan_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated scan dir");
    std::fs::write(dir.join("good.rs"), "fn good() { if true {} }").expect("write healthy fixture");
    std::fs::write(dir.join("pathological.rs"), nested_mods_source(30000))
        .expect("write pathological fixture");

    let output = Command::new(&binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute binary");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "the scan must finish (exit 0) even with one pathological file. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");

    let unmeasurable = json["metrics"]["unmeasurable_files"]
        .as_array()
        .expect("unmeasurable_files should be an array");
    assert_eq!(
        unmeasurable.len(),
        1,
        "exactly the pathological file should be unmeasurable, got: {:#?}",
        unmeasurable
    );
    assert!(
        unmeasurable[0]["path"]
            .as_str()
            .unwrap()
            .contains("pathological.rs"),
        "got: {:#?}",
        unmeasurable[0]
    );
    // The JSON writer serializes `UnmeasurableReason` via `{:?}` (Debug),
    // consistent with every other reason it already reports — not the
    // French `Display` sentence used for stderr/console output.
    assert_eq!(
        unmeasurable[0]["reason"].as_str(),
        Some("SourceTooComplex"),
        "got: {:#?}",
        unmeasurable[0]
    );

    // Project-level JSON never populates `function_details` (ADR-0012 —
    // no per-file location at aggregate scope), so the healthy file's
    // measurement shows up in the aggregate instead: `unmeasurable_files`
    // above already proves it is exactly 1 (the pathological file alone),
    // and its `if true {}` contributes a real decision point.
    assert_eq!(
        json["metrics"]["unmeasurable_files_count"].as_u64(),
        Some(1),
        "only the pathological file should count as unmeasurable, got: {:#?}",
        json["metrics"]
    );
    assert!(
        json["metrics"]["cyclomatic_complexity"].as_u64().unwrap() >= 1,
        "good.rs's decision point must still be counted, got: {:#?}",
        json["metrics"]
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

// #56 T1 — `is_io_call` only ever matched `Expr::Call` names against
// `IO_PREFIXES` ("std::fs::", ...); a method call records the BARE method
// identifier (`read_to_string`), which can never start with a qualified
// prefix, so `file.read_to_string(&mut buf)` inside a loop was silently
// unflagged. This fixture (`method_io_in_loop.rs`) pins the user-observable
// outcome: a method call on a receiver whose declared type is a known I/O
// type (`File`, bound via `File::open(..).unwrap()`) now surfaces the same
// end-to-end console warning as the free-function form.
#[test]
fn e2e_analyze_method_call_io_in_loop_is_detected() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("method_io_in_loop.rs");
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
        stdout.contains("I/O dans boucle: read_to_string"),
        "expected the method-call form read_to_string to be reported as I/O in loop: {}",
        stdout
    );
}

// #56 T2 — abstention (ADR-0010). `ctx.conn.connect()` is a field-access
// receiver (never resolved) whose method name ("connect") is on the
// human-approved suspicious-name list — the correct verdict is Unknown, not
// a fabricated Io warning and not a silent NotIo either. The user-observable
// outcome: the console shows an aggregate "non classifiables" counter ≥ 1,
// while the genuine `file.read_to_string(..)` Io call in the SAME loop still
// warns normally. Abstention is a NUMBER (ADR-0010/ADR-0014 §4), never a
// per-line pseudo-warning — so "connect" must NOT appear as an
// "I/O dans boucle" entry.
#[test]
fn e2e_analyze_unclassifiable_io_in_loop_is_counted_not_warned() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("unclassifiable_io_in_loop.rs");
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
        stdout.contains("I/O dans boucle: read_to_string"),
        "the genuine Io call must still warn: {}",
        stdout
    );
    assert!(
        !stdout.contains("I/O dans boucle: connect"),
        "an Unknown call must never surface as a per-line I/O warning: {}",
        stdout
    );
    assert!(
        stdout.contains("Appels en boucle non classifiables: 1"),
        "expected the aggregate unclassifiable counter to show 1: {}",
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

// #50 slice S4, test case 23 — the full chain CLI → parser → hexagon →
// JSON for a file whose only functions live inside an `impl` block (S1/S2
// of #50: qualified `Type::method` names). Proves D1/D2/D3 all hold
// end-to-end, not just at the unit level: impl methods are parsed,
// function_details is non-empty, and complexity_level therefore reflects a
// real threshold, never "none".
#[test]
fn e2e_analyze_impl_only_fixture_reports_measured_functions() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("impl_only.rs");
    let output = Command::new(binary)
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

    let function_details = json["metrics"]["function_details"]
        .as_array()
        .expect("function_details should be an array");
    assert!(
        !function_details.is_empty(),
        "impl methods must be parsed into function_details, got: {:#?}",
        json["metrics"]
    );
    let names: Vec<&str> = function_details
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"Widget::render") && names.contains(&"Widget::is_visible"),
        "expected qualified Type::method names, got: {:?}",
        names
    );
    assert_ne!(
        json["metrics"]["complexity_level"], "none",
        "a file with measured functions must never report \"none\": {:#?}",
        json["metrics"]
    );
}

// US8 slice 1 (issue #8) — AC1/AC3/AC6: `analyze --path <dir> --max-cpu
// <µ$>` compares the project's aggregate CPU cost against the threshold and
// prints a warning on a breach, without changing the exit code (--strict is
// T2 scope).
//
// Test List:
// 1. a maximally strict --max-cpu 0 breaches any real project -> warning
//    printed, exit 0 (AC1, AC3)
// 2. no --max-cpu/--max-co2 flags at all -> no warning, unchanged behavior
//    (AC6)

#[test]
fn e2e_analyze_path_with_breached_max_cpu_warns_and_still_exits_0() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args(["analyze", "--path", dir.to_str().unwrap(), "--max-cpu", "0"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected even on a breach (non-strict, AC3). stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("SEUIL") && stdout.contains("CPU"),
        "expected a threshold breach warning naming CPU, got: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_path_without_threshold_flags_shows_no_warning() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit 0 expected");
    assert!(
        !stdout.contains("SEUIL"),
        "no threshold was configured (AC6): output must be unchanged, got: {}",
        stdout
    );
}
