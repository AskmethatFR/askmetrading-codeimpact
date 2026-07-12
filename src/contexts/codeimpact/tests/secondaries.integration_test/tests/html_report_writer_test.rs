use std::path::PathBuf;

use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FunctionDetail;
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
    // US7 T2 S2: FileNodeVm.level_label is replaced by NodeVm.level (the tree
    // node carries the level directly, not a flat per-file row).
    assert!(html.contains("\"level\":\"low\""), "project view must carry a level per node: {}", html);
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
    // DEVIATION (US7 T2 spec §5, flagged not silently improvised): the spec
    // lists this test among those that must "survive unmodified", but its
    // `"score_pct"` assertion is a FileNodeVm-only field. The approved
    // NodeVm redesign (spec §2) replaces the single top-level score_pct with
    // a per-metric `metrics[].pct` (4 bars, see `metric_pct_is_zero_when_scale_is_zero`
    // below) — there is no longer a single "score_pct" concept to preserve.
    // Kept the test's INTENT (scale == 0 must yield 0, never panic) and
    // adapted the assertion to the new field.
    assert!(
        html.contains("\"pct\":0"),
        "scale == 0 branch must yield pct 0 for every metric, not divide by zero: {}",
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

    // Node metrics (MetricVm) also carry a "label" field, so count `"sub":`
    // instead — that field only exists on StatVm.
    assert_eq!(
        html.matches("\"sub\":").count(),
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

// ── US7 T2 slice S2: tree + aggregation + detail-pane header ──
//
// Test List (S2):
// 1. files nest under their folder nodes (child_ids), not a flat list
// 2. folder aggregation: direct/transitive/hidden SUM, depth MAX (swapping
//    sum/max must fail this — the mutation gate on spec §3)
// 3. folder score = MAX of descendant file scores (sum/first-child must fail)
// 4. folder level = worst (max-ordinal) descendant level
// 5. metric pct is 0 when its scale is 0 (div-by-zero guard, all 4 metrics)
// 6. metric pct floors at 5 for a tiny nonzero value, caps at 100 for the max
// 7. children sort: folders before files; files by score desc

#[test]
fn tree_nests_files_under_their_folders() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("a/b/one.rs", make_metrics(1, 1)),
        ("a/b/two.rs", make_metrics(1, 1)),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""id":"a/b","name":"b","kind":"folder","path":"a/b","child_ids":["a/b/one.rs","a/b/two.rs"]"#
        ),
        "folder 'a/b' must list both files as children — a flat list would fail this: {}",
        html
    );
}

#[test]
fn folder_aggregates_direct_by_sum_and_depth_by_max() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("x/f1.rs", CodeMetrics::with_call_graph(3, 3, 2, vec![], vec![])),
        ("y/f2.rs", CodeMetrics::with_call_graph(5, 5, 7, vec![], vec![])),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(r#""label":"Direct complexity","value":"8""#),
        "root direct complexity must be the SUM (3+5=8) of both folders, not their max: {}",
        html
    );
    assert!(
        html.contains(r#""label":"Max call depth","value":"7""#),
        "root max call depth must be the MAX (7) across folders, not their sum: {}",
        html
    );
}

#[test]
fn folder_score_is_max_of_descendant_file_scores() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("a/low.rs", make_metrics(1, 2)),
        ("a/high.rs", make_metrics(1, 9)),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""id":"a","name":"a","kind":"folder","path":"a","child_ids":["a/high.rs","a/low.rs"],"score":9"#
        ),
        "folder score must be the MAX descendant file score (9), not their sum (11) or the first child: {}",
        html
    );
}

#[test]
fn folder_level_is_worst_descendant_level() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("a/ok1.rs", make_metrics(1, 1)),
        ("a/ok2.rs", make_metrics(2, 2)),
        ("a/bad.rs", make_metrics(50, 50)),
        ("a/ok3.rs", make_metrics(3, 3)),
        ("a/ok4.rs", make_metrics(4, 4)),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""id":"a","name":"a","kind":"folder","path":"a","child_ids":["a/bad.rs","a/ok4.rs","a/ok3.rs","a/ok2.rs","a/ok1.rs"],"score":50,"level":"critical""#
        ),
        "a folder with one critical file among 4 low files must itself be critical: {}",
        html
    );
}

#[test]
fn metric_pct_is_zero_when_scale_is_zero() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(0, 0))]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""metrics":[{"label":"Direct complexity","value":"0","pct":0},{"label":"Transitive complexity","value":"0","pct":0},{"label":"Hidden complexity","value":"0","pct":0},{"label":"Max call depth","value":"0","pct":0}]"#
        ),
        "all-zero metrics (scale == 0) must yield pct 0 for every metric, not divide by zero: {}",
        html
    );
}

#[test]
fn metric_pct_floors_at_five_and_caps_at_hundred() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("tiny.rs", make_metrics(1, 1)),
        ("huge.rs", make_metrics(100, 100)),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""id":"tiny.rs","name":"tiny.rs","kind":"file","path":"tiny.rs","child_ids":[],"score":1,"level":"low","metrics":[{"label":"Direct complexity","value":"1","pct":5}"#
        ),
        "a small nonzero value (1/100=1%) must floor at 5%, not round down to 0: {}",
        html
    );
    assert!(
        html.contains(
            r#""id":"huge.rs","name":"huge.rs","kind":"file","path":"huge.rs","child_ids":[],"score":100,"level":"critical","metrics":[{"label":"Direct complexity","value":"100","pct":100}"#
        ),
        "the max value itself must cap at 100%: {}",
        html
    );
}

#[test]
fn children_sorted_folders_first_then_files_by_score_desc() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("z_file.rs", make_metrics(1, 99)),
        ("a_folder/inner.rs", make_metrics(1, 1)),
    ]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(r#""child_ids":["a_folder","z_file.rs"]"#),
        "folders must sort before files regardless of score: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_payload_in_a_folder_path_segment() {
    let writer = HtmlReportWriter::new();
    let payload_path = "</script><script>alert(1)</script>/evil.rs".to_string();
    let graph = graph_from(vec![(payload_path.as_str(), make_metrics(1, 1))]);

    let html = writer.write_html(&graph, "my-project").expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "a malicious FOLDER path segment must not appear as a literal script breakout: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "a malicious folder segment must not add a third <script> tag: {}",
        html
    );
}

// ── US7 T2 slice S3: full node detail (children/functions/warnings/io/impact) ──
//
// Test List (S3):
// 1. functions carry location + BOTH true/false in_cycle flags
// 2. a folder's warnings are the CONCAT of its descendant files' warnings
// 3. a folder's economic impact is the domain SUM (EconomicImpact::Add) of
//    children — never a coefficient recomputed from transitive complexity
//    (the anti-dc_script.js check, spec §0 finding 2)
// 4. a folder's ecological class is RECOMPUTED from the summed CO2, not
//    copied from any single child's class

#[test]
fn detail_carries_functions_with_location_and_cycle_flag() {
    let writer = HtmlReportWriter::new();
    let metrics = make_metrics(5, 5).with_function_details(vec![
        FunctionDetail {
            name: "a".to_string(),
            location: CodeLocation::new("f.rs".into(), 10, 1),
            direct: 1,
            transitive: 2,
            call_depth: 1,
            in_cycle: false,
        },
        FunctionDetail {
            name: "b".to_string(),
            location: CodeLocation::new("f.rs".into(), 20, 1),
            direct: 3,
            transitive: 4,
            call_depth: 2,
            in_cycle: true,
        },
    ]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""functions":[{"name":"a","direct":1,"transitive":2,"depth":1,"loc":"f.rs:10:1","in_cycle":false},{"name":"b","direct":3,"transitive":4,"depth":2,"loc":"f.rs:20:1","in_cycle":true}]"#
        ),
        "functions must carry their location and BOTH true/false in_cycle flags: {}",
        html
    );
}

#[test]
fn folder_detail_collects_warnings_from_descendant_files() {
    let writer = HtmlReportWriter::new();
    let m1 = make_metrics(1, 1).with_warnings(vec![warning_in("f1", WarningSeverity::Warning)]);
    let m2 = make_metrics(1, 1).with_warnings(vec![warning_in("f2", WarningSeverity::Critical)]);
    let graph = graph_from(vec![("a/one.rs", m1), ("a/two.rs", m2)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""warnings":[{"pattern":"DeepConditional","severity":"warning","sev_label":"WARNING","function":"f1","loc":"a.rs:1:1","message":"msg","suggestion":"sugg"},{"pattern":"DeepConditional","severity":"critical","sev_label":"CRITICAL","function":"f2","loc":"a.rs:1:1","message":"msg","suggestion":"sugg"}]"#
        ),
        "folder warnings must be the CONCAT of both descendant files' warnings, not just one: {}",
        html
    );
}

#[test]
fn folder_economic_impact_is_the_sum_of_children_impacts() {
    let writer = HtmlReportWriter::new();
    let m1 = make_metrics(1, 1).with_economic_impact(EconomicImpact::new(3.0, 512, 3.0, "low"));
    let m2 = make_metrics(1, 1).with_economic_impact(EconomicImpact::new(3.0, 512, 3.0, "low"));
    let graph = graph_from(vec![("a/one.rs", m1), ("a/two.rs", m2)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(r#""economic":{"cpu":"$0.000006","memory":"1.0 KB","total":"$0.000006","level":"low"}"#),
        "folder economic must be the domain SUM (fold via EconomicImpact::Add) of its children — never a coefficient recomputed from transitive complexity: {}",
        html
    );
}

#[test]
fn folder_ecological_class_is_recomputed_from_summed_co2() {
    let writer = HtmlReportWriter::new();
    let m1 = make_metrics(1, 1)
        .with_ecological_impact(EcologicalImpact::new(0.6, 100.0, EfficiencyClass::A));
    let m2 = make_metrics(1, 1)
        .with_ecological_impact(EcologicalImpact::new(0.6, 100.0, EfficiencyClass::A));
    let graph = graph_from(vec![("a/one.rs", m1), ("a/two.rs", m2)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        html.contains(
            r#""ecological":{"co2":"1.200 g","energy":"200.0 J (0.000056 kWh)","class":"B"}"#
        ),
        "folder ecological class must be RECOMPUTED from the summed CO2 (0.6+0.6=1.2g -> class B), not copied from either child's class A: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_payload_in_function_name() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>";
    let metrics = make_metrics(1, 1).with_function_details(vec![FunctionDetail {
        name: payload.to_string(),
        location: CodeLocation::new("f.rs".into(), 1, 1),
        direct: 1,
        transitive: 1,
        call_depth: 1,
        in_cycle: false,
    }]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "function name payload must not appear as a literal script breakout: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "function name payload must not add a third <script> tag: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_payload_in_warning_message_and_suggestion() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>";
    let metrics = make_metrics(1, 1).with_warnings(vec![ComplexityWarning {
        pattern: WarningPattern::DeepConditional,
        severity: WarningSeverity::Warning,
        function: "f".to_string(),
        location: CodeLocation::new("f.rs".into(), 1, 1),
        message: payload.to_string(),
        suggestion: payload.to_string(),
    }]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "warning message/suggestion payload must not appear as a literal script breakout: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "warning message/suggestion payload must not add a third <script> tag: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_payload_in_io_call_name() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>";
    let metrics = make_metrics(1, 1).with_io_in_loops(vec![IoInLoopWarning {
        function: "f".to_string(),
        io_call: payload.to_string(),
        location: CodeLocation::new("f.rs".into(), 1, 1),
    }]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer.write_html(&graph, "proj").expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "io_call payload must not appear as a literal script breakout: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "io_call payload must not add a third <script> tag: {}",
        html
    );
}
