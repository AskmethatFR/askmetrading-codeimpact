use std::path::PathBuf;

use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::WarningPattern;
use codeimpact_hexagon::analysis::WarningSeverity;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::HtmlReportWriter;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(cc, tc, 0, vec![], vec![])
}

fn graph_with_files(files: Vec<(&str, u32, u32)>) -> FileConsumptionGraph {
    let entries: Vec<(PathBuf, CodeMetrics)> = files
        .into_iter()
        .map(|(path, cc, tc)| (PathBuf::from(path), make_metrics(cc, tc)))
        .collect();
    FileConsumptionGraph::build(&entries, vec![]).unwrap()
}

/// Builder-style helper (US7 T2): wraps pre-built `CodeMetrics` (already
/// carrying warnings / io / functions / economic / ecological via the
/// domain's own `with_*` builder methods) into a graph, so slice tests can
/// attach exactly the fixtures they need without a combinatorial explosion
/// of `graph_with_files`-style positional params.
fn graph_from(files: Vec<(&str, CodeMetrics)>) -> FileConsumptionGraph {
    let entries: Vec<(PathBuf, CodeMetrics)> = files
        .into_iter()
        .map(|(path, metrics)| (PathBuf::from(path), metrics))
        .collect();
    FileConsumptionGraph::build(&entries, vec![]).unwrap()
}

fn warning_in(function: &str, severity: WarningSeverity) -> ComplexityWarning {
    ComplexityWarning {
        pattern: WarningPattern::DeepConditional,
        severity,
        function: function.to_string(),
        location: CodeLocation::new("a.rs".into(), 1, 1),
        message: "msg".into(),
        suggestion: "sugg".into(),
    }
}

fn io_in(function: &str) -> IoInLoopWarning {
    IoInLoopWarning {
        function: function.to_string(),
        io_call: "std::fs::read_to_string".into(),
        location: CodeLocation::new("a.rs".into(), 2, 1),
    }
}

// Test List:
// 1. write_html produces a self-contained document (no external <link>/<script src=>)
// 2. write_html output shows the project view: file paths and level badges present
// 3. a `</script><script>alert(1)</script>` file path is neutralized (no script breakout)
// 4. a `"><img onerror>` file path is neutralized (no raw `<`/`>` around it)
// 5. write_html on an empty graph (0 files) still returns Ok with a single-root shell
// 6. JsonReportWriter.write_html returns Err (does not support html output)
// 7. ConsoleReportWriter.write_html returns Err (does not support html output)
// 8. SharedReportWriterStub.write_html captures last_html
// 9. a graph with >=1 file whose complexity is 0 (max_score == 0) yields score_pct 0, no div-by-zero panic

#[test]
fn write_html_is_self_contained() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![("src/main.rs", 5, 8), ("src/lib.rs", 2, 2)]);

    let html = writer.write_html(&graph, "my-project").expect("write_html should succeed");

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
}

#[test]
fn write_html_shows_project_view_with_files_and_levels() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![("src/main.rs", 5, 8), ("src/lib.rs", 2, 2)]);

    let html = writer.write_html(&graph, "my-project").expect("write_html should succeed");

    assert!(html.contains("src/main.rs"), "project view must list file paths: {}", html);
    assert!(html.contains("src/lib.rs"), "project view must list file paths: {}", html);
    assert!(html.contains("\"level_label\":\"low\""), "project view must carry a level badge per file: {}", html);
    assert_eq!(html.matches("<html").count(), 1, "expected a single html root");
    assert!(html.contains("<!DOCTYPE html>"), "expected a valid html document: {}", html);
}

#[test]
fn write_html_neutralizes_script_breakout_payload_in_file_path() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>.rs";
    let graph = graph_with_files(vec![(payload, 1, 1)]);

    let html = writer.write_html(&graph, "my-project").expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "payload must not appear as a literal script breakout in the output: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "expected exactly the two legitimate <script> tags (data island + renderer), payload must not add a third: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_img_onerror_payload_in_file_path() {
    let writer = HtmlReportWriter::new();
    let payload = "\"><img onerror>evil.rs";
    let graph = graph_with_files(vec![(payload, 1, 1)]);

    let html = writer.write_html(&graph, "my-project").expect("write_html should succeed");

    assert!(!html.contains("<img"), "payload must not inject a literal <img> tag: {}", html);
}

#[test]
fn write_html_empty_graph_returns_valid_single_root_shell() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![]);

    let html = writer.write_html(&graph, "empty-project").expect("write_html should succeed on an empty project");

    assert!(html.contains("<!DOCTYPE html>"), "missing doctype: {}", html);
    assert_eq!(html.matches("<html").count(), 1, "expected a single html root");
}

#[test]
fn json_writer_write_html_returns_error() {
    let writer = JsonReportWriter::new();
    let graph = graph_with_files(vec![("src/main.rs", 1, 1)]);

    let result = writer.write_html(&graph, "my-project");

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!("expected AnalysisFailed, got {:?}", result),
    }
}

#[test]
fn console_writer_write_html_returns_error() {
    let writer = ConsoleReportWriter::new();
    let graph = graph_with_files(vec![("src/main.rs", 1, 1)]);

    let result = writer.write_html(&graph, "my-project");

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!("expected AnalysisFailed, got {:?}", result),
    }
}

#[test]
fn stub_write_html_captures_last_html() {
    let stub = SharedReportWriterStub::new();
    let graph = graph_with_files(vec![("src/main.rs", 1, 1)]);

    let result = stub.write_html(&graph, "my-project");

    assert!(result.is_ok());
    assert_eq!(*stub.last_html.lock().unwrap(), Some(result.unwrap()));
}

#[test]
fn write_html_zero_complexity_file_has_zero_score_pct_no_panic() {
    let writer = HtmlReportWriter::new();
    // A non-empty graph (unlike write_html_empty_graph_returns_valid_single_root_shell,
    // which has 0 files and never exercises the ternary body at all) where every
    // file's transitive_complexity is 0, so max_score == 0 in build_report_vm.
    let graph = graph_with_files(vec![("src/empty.rs", 0, 0)]);

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed for an all-zero-complexity project, not panic on div-by-zero");

    assert!(
        html.contains("\"score\":0"),
        "zero-complexity file should report score 0: {}",
        html
    );
    assert!(
        html.contains("\"score_pct\":0"),
        "max_score == 0 branch must yield score_pct 0, not divide by zero: {}",
        html
    );
}

// ── US7 T2 slice S1: Industry banner + 8-tile aggregated stat grid ──
//
// Test List (S1):
// 1. exactly 8 stat tiles are emitted, Direct Σ is the SUM of files' cyclomatic
//    complexity (a max/first-file bug must fail this)
// 2. the Warnings tile counts warnings + io together; critical sub sums
//    critical-severity warnings AND io-in-loop count
// 3. the Est. cost tile shows "—" (never "$0", never a panic) when no file
//    carries an economic impact

#[test]
fn stat_grid_has_eight_tiles_with_aggregated_values() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(5, 8)), ("b.rs", make_metrics(3, 4))]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert_eq!(
        html.matches("\"label\":").count(),
        8,
        "expected exactly 8 stat tiles: {}",
        html
    );
    assert!(
        html.contains("\"label\":\"Direct \u{3a3}\",\"value\":\"8\""),
        "Direct \u{3a3} must be the SUM of files' cyclomatic complexity (5+3=8), not a max/first-file value: {}",
        html
    );
}

#[test]
fn stat_grid_counts_critical_warnings_and_io_together() {
    let writer = HtmlReportWriter::new();
    let file_metrics = make_metrics(5, 5)
        .with_warnings(vec![warning_in("f", WarningSeverity::Critical)])
        .with_io_in_loops(vec![io_in("f")]);
    let graph = graph_from(vec![("a.rs", file_metrics)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains("\"label\":\"Warnings\",\"value\":\"2\",\"sub\":\"2 critical\""),
        "1 critical warning + 1 io-in-loop must total 2 warnings and 2 critical: {}",
        html
    );
}

#[test]
fn stat_grid_shows_dash_when_no_economic_impact() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains("\"label\":\"Est. cost\",\"value\":\"\u{2014}\""),
        "no economic impact anywhere must render a dash, not $0 nor a panic: {}",
        html
    );
}

// ── Rendering discipline — structural tests (spec §4, ADR-8.10) ──
//
// These fail the BUILD (not merely production) the moment a banned sink or
// a third `.style` access is introduced anywhere in the emitted JS.

#[test]
fn rendered_js_contains_no_html_sink() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    let banned = [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "setAttribute",
        "eval(",
        "new Function",
        "javascript:",
        "srcdoc",
        "cssText",
    ];
    for pattern in banned {
        assert!(
            !html.contains(pattern),
            "emitted JS must never contain the banned sink '{}': {}",
            pattern,
            html
        );
    }
}

#[test]
fn rendered_js_has_only_two_style_sinks() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    let total_style = html.matches(".style.").count();
    let width_sink = html.matches(".style.width").count();
    let padding_sink = html.matches(".style.paddingLeft").count();
    assert_eq!(
        total_style,
        width_sink + padding_sink,
        "exactly two clamped numeric style sinks are allowed in the whole file (width, paddingLeft): {}",
        html
    );
}
