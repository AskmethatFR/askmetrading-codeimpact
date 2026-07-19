use std::path::Path;
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

// ── US16 T2 (step A/F) — C# support via tree-sitter, second CodeParser
// adapter (ADR-0018). Test List:
//   1. e2e_analyze_csharp_file_exits_0 — the slice's own behavioral test
//      (RED until the registry wiring landed): a C# file that previously
//      produced nothing now reports functions + nonzero complexity.
//   2. e2e_analyze_path_with_mixed_rust_and_csharp_files_measures_both —
//      dispatch per file in a real project scan, not stubs.

#[test]
fn e2e_analyze_csharp_file_exits_0() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.cs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {} stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Complexité directe"),
        "missing complexity: {}",
        stdout
    );
    assert!(
        stdout.contains("Compute"),
        "missing function name: {}",
        stdout
    );
    assert!(
        !stdout.contains("Complexité directe: 0"),
        "expected a nonzero complexity for a file with an if+for: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_path_with_mixed_rust_and_csharp_files_measures_both() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_mixed_languages_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated scan dir");
    std::fs::write(dir.join("a.rs"), "fn a() { if true {} }").expect("write rust fixture");
    std::fs::write(dir.join("b.cs"), "class C { void M() { if (true) { } } }")
        .expect("write csharp fixture");

    let output = Command::new(binary)
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
        "exit 0 expected for a mixed-language project. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(
        json["metrics"]["unmeasurable_files_count"], 0,
        "both languages must be measured, none unmeasurable: {}",
        stdout
    );
    assert_eq!(
        json["metrics"]["cyclomatic_complexity"], 4,
        "1 (base) + 1 (if) for each of the two files: {}",
        stdout
    );
}

// ── US16 T2 (step H, Q2 security) — a pathologically deep C# file must
// never crash the process, and must never abort a project scan for its
// healthy siblings. Mirrors #63's own e2e test for the Rust/syn adapter.
#[test]
fn e2e_analyze_path_with_one_pathological_csharp_file_still_completes() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_csharp_pathological_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated scan dir");
    std::fs::write(
        dir.join("good.cs"),
        "class C { void M() { if (true) { } } }",
    )
    .expect("write healthy fixture");

    // Depth chosen empirically to exercise the PARSE_QUERY_BUDGET timeout
    // path (Q2) specifically, not source_guard's separate 1 MB size cap:
    // 80_000 * "if(x){\n" + "}\n" stays under ~720 KB (well inside 1 MB)
    // while still exceeding the 5s wall-clock budget the query stage runs
    // against.
    let mut pathological = String::from("class P { void M() { bool x = true;\n");
    for _ in 0..80_000 {
        pathological.push_str("if(x){\n");
    }
    pathological.push_str("int z = 1;\n");
    for _ in 0..80_000 {
        pathological.push_str("}\n");
    }
    pathological.push_str("} }\n");
    std::fs::write(dir.join("pathological.cs"), &pathological).expect("write pathological fixture");

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
        "the scan must finish (exit 0, never a crash) even with one pathological C# file. stdout: {}, stderr: {}",
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
            .contains("pathological.cs"),
        "got: {:#?}",
        unmeasurable[0]
    );
    assert_eq!(
        unmeasurable[0]["reason"], "SourceTooComplex",
        "must be the Q2 parse/query budget, not the separate 1 MB size cap: {:#?}",
        unmeasurable[0]
    );
}

// ── US16 T2 retry #1 (Security HIGH) — a FLAT-sibling pathological C# file
// (tens of thousands of `if(x){}` one after another in a single method,
// NOT nested) must ALSO become Unmeasurable(SourceTooComplex) within
// budget. The nested variant above trips the shared parse/query deadline
// (PARSE_QUERY_BUDGET) quickly; a flat structure keeps parse+query fast
// and instead blows up in the O(n) assign_captures_to_functions
// post-processing pass, which the parse/query deadline alone does not
// cover — reproduced by Security as a 45.9s measured (non-Unmeasurable)
// run before the fix.
#[test]
fn e2e_analyze_path_with_one_flat_sibling_pathological_csharp_file_still_completes() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_csharp_flat_pathological_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated scan dir");
    std::fs::write(
        dir.join("good.cs"),
        "class C { void M() { if (true) { } } }",
    )
    .expect("write healthy fixture");

    // 80_000 SIBLING (not nested) `if(x){}` statements, one after another,
    // in a single method — same shape/size class as the nested repro
    // above (~640 KB, under the 1 MB source_guard cap), but flat instead
    // of nested so parse+query finish fast and the O(n^2) containment
    // work in assign_captures_to_functions is what must be bounded.
    let mut pathological = String::from("class P { void M() { bool x = true;\n");
    for _ in 0..80_000 {
        pathological.push_str("if(x){}\n");
    }
    pathological.push_str("} }\n");
    std::fs::write(dir.join("pathological.cs"), &pathological).expect("write pathological fixture");

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
        "the scan must finish (exit 0, never a crash) even with one flat-sibling pathological C# file. stdout: {}, stderr: {}",
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
        "exactly the pathological file should be unmeasurable (a flat-sibling explosion must not be silently measured), got: {:#?}",
        unmeasurable
    );
    assert!(
        unmeasurable[0]["path"]
            .as_str()
            .unwrap()
            .contains("pathological.cs"),
        "got: {:#?}",
        unmeasurable[0]
    );
    assert_eq!(
        unmeasurable[0]["reason"], "SourceTooComplex",
        "got: {:#?}",
        unmeasurable[0]
    );
}

// ── US16 T2 retry #2 (Security HIGH) — MANY small functions must be
// MEASURED FAST, not refused. Retry #1 gated the second loop's O(n^2)
// containment helpers per function (MAX_QUADRATIC_CAPTURES_PER_FUNCTION);
// this reproduces a DIFFERENT ungated path — the FIRST loop's
// innermost_function_index scan, O(functions x captures), triggered by
// MANY functions each individually under the per-function cap. 58,000
// tiny one-`if` functions (~1.04 MB, matching Security's external repro
// shape exactly — duplicate method names are syntactically fine for
// tree-sitter even though a real C# compiler would reject them) is large
// but entirely legitimate code: the correct outcome is a FAST, CORRECT
// measurement, never Unmeasurable.
#[test]
fn e2e_analyze_path_with_many_small_csharp_functions_measures_fast() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_csharp_many_functions_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated scan dir");

    let mut source = String::from("class C {\n");
    for _ in 0..58_000 {
        source.push_str("void a(){if(x){}}\n");
    }
    source.push_str("}\n");
    std::fs::write(dir.join("many_functions.cs"), &source).expect("write fixture");

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
        "exit 0 expected. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(
        json["metrics"]["unmeasurable_files_count"], 0,
        "58,000 tiny valid functions is large but legitimate code — it must be MEASURED, not refused: {}",
        stdout
    );
    assert_eq!(
        json["metrics"]["cyclomatic_complexity"], 58_001,
        "1 (base) + 1 (if) per function, summed across 58,000 functions: {}",
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

// US8 T5 — stress-test gains the same threshold flags as analyze. Checked
// via --help (cheap) rather than a real stress-test run (running the
// project's actual `cargo test` subprocess is expensive and not otherwise
// exercised at the e2e level in this suite).
#[test]
fn e2e_stress_test_help_shows_threshold_flags() {
    let binary = binary_path();
    let output = Command::new(binary)
        .args(["stress-test", "--help"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit 0 expected");
    for flag in ["--max-kwh", "--max-co2", "--strict", "--config"] {
        assert!(
            stdout.contains(flag),
            "stress-test help should show {}: {}",
            flag,
            stdout
        );
    }
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

// US8 (QA re-review sweep, energy swap, issue #8) — the removed --max-cpu
// flag must actually be gone from the CLI surface, not merely renamed in
// intent. clap rejects it as an unrecognized argument (exit 2, its own
// reserved arg-parse code — distinct from our exit 1 validation errors).
#[test]
fn e2e_analyze_old_max_cpu_flag_is_rejected() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--max-cpu", "0"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "the removed --max-cpu flag must be rejected by clap itself. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("--max-cpu"),
        "stderr should name the unrecognized flag, got: {}",
        stderr
    );
}

// US8 (QA review sweep, issue #8) — CLI-relay error branches: an invalid
// --max-kwh/--max-co2 value or an unreadable --config path must be relayed
// as a real process failure (exit 1, "erreur: ..." on stderr), not silently
// swallowed or accepted. Same shape as e2e_analyze_invalid_format_errors/
// e2e_analyze_nonexistent_file_exits_1 above — this is characterization
// coverage of already-passing production code (AlertThresholds::new's
// validation, already unit-pinned in alert_thresholds_test.rs; the config
// reader's not-found path, already unit-pinned in
// file_system_config_reader_test.rs), verified once more at the real CLI
// boundary.
//
// Test List:
// 1. a negative --max-kwh is rejected (note: `--max-kwh=-5`, not
//    `--max-kwh -5` — the latter is swallowed by clap's own arg parser as
//    an unrecognized flag before ever reaching AlertThresholds::new,
//    exiting 2 instead of exercising our validation at all)
// 2. --max-kwh inf is rejected (non-finite)
// 3. --max-kwh nan is rejected (non-finite)
// 4. a --config path that does not exist is rejected, not silently ignored

#[test]
fn e2e_analyze_negative_max_kwh_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--max-kwh=-5"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a negative threshold must be rejected by our own validation (exit 1, not clap's \
         reserved exit 2). stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr,
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain error: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_infinite_max_kwh_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--max-kwh", "inf"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr);
    assert!(
        stderr.contains("erreur"),
        "stderr should contain error: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_nan_max_kwh_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--max-kwh", "nan"])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr);
    assert!(
        stderr.contains("erreur"),
        "stderr should contain error: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_nonexistent_config_path_errors() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--config",
            "/tmp/nonexistent_codeimpact_config_xyz_12345.json",
        ])
        .output()
        .expect("failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unreadable --config path must not be silently ignored. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr,
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain error: {}",
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

// US8 slice 1 (issue #8) — AC1/AC3/AC6: `analyze --path <dir> --max-kwh
// <kWh>` compares the project's aggregate energy against the threshold and
// prints a warning on a breach, without changing the exit code (--strict is
// T2 scope). Change request on issue #8: energy replaces CPU cost as the
// gate's first metric.
//
// Test List:
// 1. a maximally strict --max-kwh 0 breaches any real project -> warning
//    printed, exit 0 (AC1, AC3)
// 2. no --max-kwh/--max-co2 flags at all -> no warning, unchanged behavior
//    (AC6)

#[test]
fn e2e_analyze_path_with_breached_max_kwh_warns_and_still_exits_0() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args(["analyze", "--path", dir.to_str().unwrap(), "--max-kwh", "0"])
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
        stdout.contains("SEUIL") && stdout.contains("ÉNERGIE"),
        "expected a threshold breach warning naming the energy metric, got: {}",
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

// US8 slice 2 (AC4/AC6) — --strict maps a breach to exit 3, naming which
// threshold(s) were exceeded and by how much; without a breach it stays 0.

#[test]
fn e2e_analyze_path_strict_breach_exits_3_naming_the_threshold() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--max-kwh",
            "0",
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");

    assert_eq!(
        output.status.code(),
        Some(3),
        "a strict breach must exit 3 (AC4). stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("SEUIL") && stderr.contains("ÉNERGIE"),
        "stderr must name which threshold was exceeded and by how much, got: {}",
        stderr
    );
}

// US8 slice 3 (AC3) — the breach must reach every surface, not just
// console: JSON embeds a structured thresholds object; HTML embeds it in
// the data island (rendered client-side, same mechanism as
// unmeasurable_files). Also covers the single-file target (T3).

#[test]
fn e2e_analyze_path_json_format_breach_embeds_thresholds_object() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--format",
            "json",
            "--max-kwh",
            "0",
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");

    assert_eq!(
        output.status.code(),
        Some(3),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(json["metrics"]["thresholds"]["has_breach"], true);
    assert_eq!(
        json["metrics"]["thresholds"]["breaches"][0]["metric"],
        "ÉNERGIE"
    );
}

#[test]
fn e2e_analyze_path_html_format_breach_embeds_thresholds_in_data_island() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output_path = std::env::temp_dir().join(format!(
        "codeimpact_threshold_report_{}.html",
        std::process::id()
    ));
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
            "--max-kwh",
            "0",
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");

    assert_eq!(
        output.status.code(),
        Some(3),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let html =
        std::fs::read_to_string(&output_path).expect("html output file should have been created");
    let _ = std::fs::remove_file(&output_path);
    assert!(html.contains(r#""has_breach":true"#), "got: {}", html);
}

#[test]
fn e2e_analyze_single_file_breach_warns_via_console() {
    let binary = binary_path();
    let fixture = fixtures_dir().join("sample.rs");
    let output = Command::new(binary)
        .args(["analyze", fixture.to_str().unwrap(), "--max-kwh", "0"])
        .output()
        .expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "non-strict breach must still exit 0. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("SEUIL") && stdout.contains("ÉNERGIE"),
        "got: {}",
        stdout
    );
}

// US8 slice 4 (AC2) — thresholds may come from `.codeimpact.json`; a CLI
// flag overrides the file value for the same metric.

#[test]
fn e2e_analyze_reads_threshold_from_config_file() {
    let binary = binary_path();
    let dir =
        std::env::temp_dir().join(format!("codeimpact_e2e_config_file_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated dir");
    std::fs::write(dir.join("good.rs"), "fn good() {}").expect("write fixture");
    std::fs::write(
        dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_energy_kwh":0.0}}"#,
    )
    .expect("write config");

    let output = Command::new(&binary)
        .args(["analyze", "--path", dir.to_str().unwrap(), "--strict"])
        .output()
        .expect("failed to execute binary");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(3),
        "the config file's threshold alone must be enough to breach. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn e2e_analyze_cli_flag_overrides_config_file_value() {
    let binary = binary_path();
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_config_override_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated dir");
    std::fs::write(dir.join("good.rs"), "fn good() {}").expect("write fixture");
    // The file alone would breach (max-kwh 0); the CLI flag for the SAME
    // metric must win and let it through.
    std::fs::write(
        dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_energy_kwh":0.0}}"#,
    )
    .expect("write config");

    let output = Command::new(&binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--max-kwh",
            "1000000",
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "the CLI flag must override the config file's stricter value for the same metric. \
         stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn e2e_analyze_explicit_config_flag_is_honored() {
    let binary = binary_path();
    let target_dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_explicit_config_target_{}",
        std::process::id()
    ));
    let config_dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_explicit_config_elsewhere_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&target_dir);
    let _ = std::fs::remove_dir_all(&config_dir);
    std::fs::create_dir_all(&target_dir).expect("create target dir");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(target_dir.join("good.rs"), "fn good() {}").expect("write fixture");
    let explicit_config = config_dir.join("thresholds.json");
    std::fs::write(&explicit_config, r#"{"thresholds":{"max_energy_kwh":0.0}}"#)
        .expect("write explicit config");

    let output = Command::new(&binary)
        .args([
            "analyze",
            "--path",
            target_dir.to_str().unwrap(),
            "--config",
            explicit_config.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");
    let _ = std::fs::remove_dir_all(&target_dir);
    let _ = std::fs::remove_dir_all(&config_dir);

    assert_eq!(
        output.status.code(),
        Some(3),
        "an explicit --config path (not next to the target) must still be read. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn e2e_analyze_path_strict_without_breach_exits_0() {
    let binary = binary_path();
    let dir = fixtures_dir();
    let output = Command::new(binary)
        .args([
            "analyze",
            "--path",
            dir.to_str().unwrap(),
            "--max-kwh",
            "1000000",
            "--strict",
        ])
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "no breach: --strict must still exit 0 (AC6). stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ── US31 (#31) — include/exclude/respectGitignore in `.codeimpact.json` ──
//
// The console writer prints "Fichiers analysés: N" (aggregated.total_files)
// — the cheapest, most direct user-observable signal of the analyzed-file
// COUNT, which is exactly what each slice's acceptance criterion pins.

fn isolated_us31_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_e2e_us31_{}_{}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated us31 dir");
    dir
}

fn files_analyzed_count(binary: &Path, dir: &Path) -> (std::process::Output, String) {
    let output = Command::new(binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output, stdout)
}

// Slice 1 (AC per tech spec) — a `.codeimpact.json` `exclude` glob prunes
// files from the walk: the analyzed-file COUNT drops.
//
// Test List:
// 1. exclude glob prunes a matching file -> count drops from 2 to 1
// 2. non-regression: the SAME directory layout with NO config file at all
//    analyzes every file exactly as before (D4)

#[test]
fn e2e_analyze_exclude_glob_in_config_drops_the_excluded_file_from_the_count() {
    let binary = binary_path();
    let dir = isolated_us31_dir("exclude_count_drop");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::create_dir_all(dir.join("generated")).unwrap();
    std::fs::write(dir.join("generated").join("drop.rs"), "fn drop_fn() {}").unwrap();
    std::fs::write(
        dir.join(".codeimpact.json"),
        r#"{"exclude":["generated/**"]}"#,
    )
    .unwrap();

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 1"),
        "the excluded file must drop the count to 1, got: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_same_layout_without_any_config_file_analyzes_every_file_unchanged() {
    let binary = binary_path();
    let dir = isolated_us31_dir("exclude_count_drop_non_regression");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::create_dir_all(dir.join("generated")).unwrap();
    std::fs::write(dir.join("generated").join("drop.rs"), "fn drop_fn() {}").unwrap();
    // No .codeimpact.json at all — D4: must reproduce today's behavior
    // byte-for-byte (every file walked, nothing pruned).

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 2"),
        "with no config file, every file must still be analyzed (D4 non-regression), got: {}",
        stdout
    );
}

// Slice 2 — `include` restricts the walk; a file matched by BOTH include
// and exclude is excluded (exclude wins).

#[test]
fn e2e_analyze_include_glob_restricts_the_count() {
    let binary = binary_path();
    let dir = isolated_us31_dir("include_restricts");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::write(dir.join("other.rs"), "fn other() {}").unwrap();
    std::fs::write(dir.join(".codeimpact.json"), r#"{"include":["src/**"]}"#).unwrap();

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 1"),
        "only src/keep.rs is within include, got: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_file_matched_by_both_include_and_exclude_is_excluded() {
    let binary = binary_path();
    let dir = isolated_us31_dir("both_match_excluded");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("both.rs"), "fn both() {}").unwrap();
    std::fs::write(dir.join("src").join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::write(
        dir.join(".codeimpact.json"),
        r#"{"include":["src/**"],"exclude":["src/both.rs"]}"#,
    )
    .unwrap();

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 1"),
        "both.rs matches both include and exclude (exclude wins), only keep.rs remains, got: {}",
        stdout
    );
}

// Slice 3 — `respectGitignore` (default true when the config file is
// present) drops `.gitignore`d files; `false` reintegrates them; an absent
// config file is unaffected by any `.gitignore` present in the tree (D4).

#[test]
fn e2e_analyze_respect_gitignore_default_true_drops_gitignored_file() {
    let binary = binary_path();
    let dir = isolated_us31_dir("gitignore_default_true");
    std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("ignored.rs"), "fn ignored() {}").unwrap();
    std::fs::write(dir.join(".codeimpact.json"), r#"{}"#).unwrap();

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 1"),
        "a present config file defaults respectGitignore to true, dropping ignored.rs, got: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_respect_gitignore_false_reintegrates_the_file() {
    let binary = binary_path();
    let dir = isolated_us31_dir("gitignore_explicit_false");
    std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("ignored.rs"), "fn ignored() {}").unwrap();
    std::fs::write(
        dir.join(".codeimpact.json"),
        r#"{"respectGitignore":false}"#,
    )
    .unwrap();

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 2"),
        "respectGitignore=false must reintegrate ignored.rs, got: {}",
        stdout
    );
}

#[test]
fn e2e_analyze_gitignore_present_but_no_config_file_leaves_gitignored_file_included() {
    let binary = binary_path();
    let dir = isolated_us31_dir("gitignore_no_config");
    std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("ignored.rs"), "fn ignored() {}").unwrap();
    // No .codeimpact.json at all (D4): unrestricted, .gitignore not honored.

    let (output, stdout) = files_analyzed_count(&binary, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "exit 0 expected. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("Fichiers analysés: 2"),
        "with no config file at all, .gitignore must not be honored (D4), got: {}",
        stdout
    );
}

// Slice 4 — hostile config: an explicit, actionable error naming the
// offending JSON line/key/glob/pattern, exit 1, NEVER a crash/panic.

#[test]
fn e2e_analyze_malformed_config_json_names_the_line_and_exits_1() {
    let binary = binary_path();
    let dir = isolated_us31_dir("hostile_malformed_json");
    std::fs::write(dir.join("good.rs"), "fn good() {}").unwrap();
    std::fs::write(dir.join(".codeimpact.json"), "not json at all @@@").unwrap();

    let output = Command::new(&binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "malformed config JSON must exit 1 cleanly (never a crash/signal). stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur") && stderr.contains("line"),
        "stderr must name the offending line, got: {}",
        stderr
    );
    assert!(
        !stderr.contains(dir.to_str().unwrap()),
        "error message must not leak the absolute path (ADR-0006): {}",
        stderr
    );
}

#[test]
fn e2e_analyze_unknown_config_key_names_the_key_and_exits_1() {
    let binary = binary_path();
    let dir = isolated_us31_dir("hostile_unknown_key");
    std::fs::write(dir.join("good.rs"), "fn good() {}").unwrap();
    // "includ" is a typo of "include" — deny_unknown_fields must reject it.
    std::fs::write(dir.join(".codeimpact.json"), r#"{"includ":["src/**"]}"#).unwrap();

    let output = Command::new(&binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "an unknown/typo'd config key must exit 1 cleanly. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("includ"),
        "stderr must name the offending key, got: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_invalid_glob_in_config_errors_without_panicking() {
    let binary = binary_path();
    let dir = isolated_us31_dir("hostile_invalid_glob");
    std::fs::write(dir.join("good.rs"), "fn good() {}").unwrap();
    std::fs::write(dir.join(".codeimpact.json"), r#"{"exclude":["src/["]}"#).unwrap();

    let output = Command::new(&binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "an invalid glob pattern must exit 1 cleanly (never a crash/signal). stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
}

#[test]
fn e2e_analyze_path_traversal_include_pattern_is_rejected() {
    let binary = binary_path();
    let dir = isolated_us31_dir("hostile_traversal");
    std::fs::write(dir.join("good.rs"), "fn good() {}").unwrap();
    std::fs::write(dir.join(".codeimpact.json"), r#"{"include":["../etc/**"]}"#).unwrap();

    let output = Command::new(&binary)
        .args(["analyze", "--path", dir.to_str().unwrap()])
        .output()
        .expect("failed to execute binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a path-traversal include pattern must exit 1 cleanly. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("erreur"),
        "stderr should contain a clear error message: {}",
        stderr
    );
}
