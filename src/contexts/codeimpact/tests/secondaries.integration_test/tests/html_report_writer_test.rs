use std::path::PathBuf;

use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
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

// Test List:
// 1. write_html produces a self-contained document (no external <link>/<script src=>)
// 2. write_html output shows the project view: file paths and level badges present
// 3. a `</script><script>alert(1)</script>` file path is neutralized (no script breakout)
// 4. a `"><img onerror>` file path is neutralized (no raw `<`/`>` around it)
// 5. write_html on an empty graph (0 files) still returns Ok with a single-root shell
// 6. JsonReportWriter.write_html returns Err (does not support html output)
// 7. ConsoleReportWriter.write_html returns Err (does not support html output)
// 8. SharedReportWriterStub.write_html captures last_html

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
