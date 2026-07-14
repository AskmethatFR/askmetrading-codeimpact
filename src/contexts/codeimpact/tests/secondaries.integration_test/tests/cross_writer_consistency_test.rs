use std::path::PathBuf;

use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FunctionDetail;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::HtmlReportWriter;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;

// Test List (#46 + #49 — the test that was missing, ADR-0012):
// 1. json_and_html_report_the_same_hidden_complexity — JSON and HTML (both
//    the stat tile AND the root node detail) must report the SAME hidden
//    complexity for the SAME graph, and it must be the value the additive
//    per-function formula gives (3 on this fixture), not `max(0, ΣT-ΣC)`
//    (1, the JSON-shaped bug) nor `Σ max(0, Tᵢ-Cᵢ)` (2, the HTML-shaped
//    bug). This test cannot be satisfied by making the writers wrong in the
//    same way — it asserts the VALUE, not just the cross-writer equality.
// 2. direct/transitive/max_call_depth also agree across writers (they
//    already did before the fix — this pins the regression guard).
// 3. console_project_summary_reports_hidden_total (T3) — the console's
//    "=== Résumé du projet ===" gains a "Complexité cachée totale" line
//    reporting the SAME value (3) as JSON and HTML, on the SAME graph.

/// Fixture (tech spec §5): two files whose functions do NOT call each other
/// within the same file except a.rs's f1 -> f2, discriminating the three
/// candidate formulas:
///   a.rs: f1{direct=2, transitive=5, hidden=3}, f2{direct=3, transitive=3, hidden=0}
///         C(a)=1+2+3=6   T(a)=5+3=8   hidden(a)=3
///   b.rs: g{direct=1, transitive=1, hidden=0}
///         C(b)=1+1=2     T(b)=1       hidden(b)=0
///   PROJECT: ΣC=8  ΣT=9  hidden CORRECT=3 (not max(0,9-8)=1, not
///            max(0,8-6)+max(0,1-2)=2)
fn detail(name: &str, file: &str, direct: u32, hidden: u32, call_depth: usize) -> FunctionDetail {
    FunctionDetail::new(
        name.to_string(),
        CodeLocation::new(file.to_string(), 1, 1),
        direct,
        hidden,
        call_depth,
        false,
    )
}

fn a_project_graph() -> FileConsumptionGraph {
    let a = CodeMetrics::with_call_graph(
        6,
        8,
        1,
        vec![],
        vec![detail("f1", "a.rs", 2, 3, 1), detail("f2", "a.rs", 3, 0, 0)],
    );
    let b = CodeMetrics::with_call_graph(2, 1, 0, vec![], vec![detail("g", "b.rs", 1, 0, 0)]);

    FileConsumptionGraph::build(
        &[(PathBuf::from("a.rs"), a), (PathBuf::from("b.rs"), b)],
        vec![],
    )
    .unwrap()
}

#[test]
fn json_and_html_report_the_same_hidden_complexity() {
    let graph = a_project_graph();

    let json_writer = JsonReportWriter::new();
    let json_str = json_writer
        .write_project_json(&graph, "t")
        .expect("write_project_json should succeed");
    let json: serde_json::Value =
        serde_json::from_str(&json_str).expect("write_project_json output should be valid JSON");

    // VALUE — mords the formula itself, not just cross-writer equality.
    assert_eq!(
        json["metrics"]["hidden_complexity"], 3,
        "hidden must be the additive per-function sum (3), not max(0, ΣT-ΣC)=1: {}",
        json_str
    );
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 8);
    assert_eq!(json["metrics"]["transitive_complexity"], 9);
    assert_eq!(json["metrics"]["max_call_depth"], 1);

    let html_writer = HtmlReportWriter::new();
    let html = html_writer
        .write_html(&graph, "t")
        .expect("write_html should succeed");

    // Stat tile: "Transitive Σ" sub must read "3 hidden", not "2 hidden"
    // (the Σ max(0, Tᵢ-Cᵢ) HTML-shaped bug).
    assert!(
        html.contains("\"label\":\"Transitive \u{3a3}\",\"value\":\"9\",\"sub\":\"3 hidden\""),
        "HTML stat tile must report transitive=9 with 3 hidden: {}",
        html
    );
    assert!(
        html.contains("\"label\":\"Direct \u{3a3}\",\"value\":\"8\""),
        "HTML stat tile must report direct=8: {}",
        html
    );

    // Root node detail (id == "", the project root — spec §2 "the root is
    // the node with id == \"\"") must carry the SAME hidden value.
    assert!(
        html.contains(r#""id":"","name":"t","kind":"project""#),
        "root node with id \"\" must be present: {}",
        html
    );
    assert!(
        html.contains("\"label\":\"Hidden complexity\",\"value\":\"3\""),
        "root node detail must report hidden complexity = 3, matching the stat tile and JSON: {}",
        html
    );
}

#[test]
fn console_project_summary_reports_hidden_total() {
    let graph = a_project_graph();

    let console_writer = ConsoleReportWriter::new();
    let mut buf: Vec<u8> = Vec::new();
    console_writer.write_project_report_to(&mut buf, &graph);
    let console = String::from_utf8(buf).expect("console output should be valid UTF-8");

    assert!(
        console.contains("Complexité directe totale: 8"),
        "console must report the same direct total as JSON/HTML: {}",
        console
    );
    assert!(
        console.contains("Complexité transitive totale: 9"),
        "console must report the same transitive total as JSON/HTML: {}",
        console
    );
    assert!(
        console.contains("Complexité cachée totale: 3"),
        "console must report the SAME hidden total (3) as JSON and HTML, in \
         the project summary section: {}",
        console
    );
}
