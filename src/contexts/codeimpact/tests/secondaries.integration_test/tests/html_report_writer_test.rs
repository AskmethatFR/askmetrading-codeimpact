use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
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
use codeimpact_hexagon::analysis::UnmeasurableFile;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_hexagon::analysis::WarningPattern;
use codeimpact_hexagon::analysis::WarningSeverity;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::HtmlReportWriter;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(cc, tc, 0, vec![], vec![])
}

/// Like `make_metrics`, but with one measured function attached, so
/// `complexity_level()` reports a real threshold level instead of D3's
/// (#50 slice S4) "none" — for tests whose actual intent is to exercise
/// level-dependent behavior (e.g. worst-of-children ranking), not the
/// "nothing measured" state.
fn make_metrics_measured(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(
        cc,
        tc,
        0,
        vec![],
        vec![FunctionDetail::new(
            "f".to_string(),
            CodeLocation::new("f.rs".into(), 1, 1),
            cc,
            0,
            0,
            false,
        )],
    )
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

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed");

    assert!(
        html.contains("src/main.rs"),
        "project view must list file paths: {}",
        html
    );
    assert!(
        html.contains("src/lib.rs"),
        "project view must list file paths: {}",
        html
    );
    // US7 T2 S2: FileNodeVm.level_label is replaced by NodeVm.level (the tree
    // node carries the level directly, not a flat per-file row).
    // D3 (#50 slice S4): make_metrics() carries no function_details, so
    // these fixtures now correctly read "none" ("nothing to measure"), not
    // a fabricated "low" — this test's intent (a level field is present
    // per node) is unchanged.
    assert!(
        html.contains("\"level\":\"none\""),
        "project view must carry a level per node: {}",
        html
    );
    assert_eq!(
        html.matches("<html").count(),
        1,
        "expected a single html root"
    );
    assert!(
        html.contains("<!DOCTYPE html>"),
        "expected a valid html document: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_script_breakout_payload_in_file_path() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>.rs";
    let graph = graph_with_files(vec![(payload, 1, 1)]);

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed");

    assert!(
        !html.contains("<img"),
        "payload must not inject a literal <img> tag: {}",
        html
    );
}

#[test]
fn write_html_empty_graph_returns_valid_single_root_shell() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![]);

    let html = writer
        .write_html(&graph, "empty-project")
        .expect("write_html should succeed on an empty project");

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

    let html = writer.write_html(&graph, "my-project").expect(
        "write_html should succeed for an all-zero-complexity project, not panic on div-by-zero",
    );

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

// D3 (#50 slice S4), test case 22 — the level_rank/LVL traps: a zero-function
// file must render its OWN grey "none" class, never fall into the critical
// catch-all (level_rank's old `_ => 3`) nor the JS LVL fallback (`lvl-low`).
// Two separate, independently-failing assertions on purpose:
// 1. the data island's level field is "none", not a fabricated "critical"
//    (defends the level_rank/level_name catch-all trap — the class names
//    themselves are static CSS/JS boilerplate always present in the
//    document regardless of node data, so this is the only meaningful
//    server-rendered signal for that trap).
// 2. the emitted JS's LVL lookup map explicitly maps "none" to "lvl-none"
//    (defends the JS fallback trap — without this entry, `cls(LVL, "none",
//    "lvl-low")` would silently render every zero-function node green).
#[test]
fn zero_function_file_renders_level_none_and_js_maps_it_to_its_own_class() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("empty.rs", make_metrics(0, 0))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains(r#""id":"empty.rs","name":"empty.rs","kind":"file","path":"empty.rs","child_ids":[],"score":0,"level":"none""#),
        "a zero-function file must report level \"none\", not a fabricated threshold: {}",
        html
    );
    assert!(
        html.contains(r#"LVL = { none: "lvl-none""#),
        "the JS LVL map must explicitly map \"none\" to its own class, not fall through to lvl-low: {}",
        html
    );
    assert!(
        html.contains(".lvl-none {"),
        "the CSS must define a dedicated .lvl-none rule: {}",
        html
    );
}

// ── #46/#49 T2: build_stats() is a pure render — 9-tile aggregated stat
// grid, everything sourced from ProjectMetrics (zero local calculation) ──
//
// Test List (T2):
// 1. exactly 9 stat tiles are emitted, Direct Σ is the SUM of files'
//    cyclomatic complexity (a max/first-file bug must fail this)
// 2. the Warnings tile counts ComplexityWarning ONLY — critical sub counts
//    critical-severity warnings ONLY. IoInLoopWarning has no severity (it
//    is not a "critical warning" by ubiquitous language) and must never be
//    folded into either count.
// 3. I/O in loops gets its OWN tile, separate from Warnings.
// 4. the Est. cost tile shows "—" (never "$0", never a panic) when no file
//    carries an economic impact.
// 5. every_stat_tile_matches_the_tree_root — the structural guard: no tile
//    may report a number that diverges from the root node detail it
//    summarizes (the #46/#49 anti-recidive guard).

fn extract_data_island(html: &str) -> serde_json::Value {
    let start_marker = r#"<script id="ci-data" type="application/json">"#;
    let start = html
        .find(start_marker)
        .expect("data island should be present")
        + start_marker.len();
    let end = html[start..]
        .find("</script>")
        .expect("data island should be closed")
        + start;
    serde_json::from_str(&html[start..end]).expect("data island should be valid JSON")
}

#[test]
fn stat_grid_has_nine_tiles_with_aggregated_values() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![
        ("a.rs", make_metrics(5, 8)),
        ("b.rs", make_metrics(3, 4)),
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    // Node metrics (MetricVm) also carry a "label" field, so count `"sub":`
    // instead — that field only exists on StatVm.
    assert_eq!(
        html.matches("\"sub\":").count(),
        10,
        "expected exactly 10 stat tiles (#56 T2 adds Unclassifiable): {}",
        html
    );
    assert!(
        html.contains("\"label\":\"Direct \u{3a3}\",\"value\":\"8\""),
        "Direct \u{3a3} must be the SUM of files' cyclomatic complexity (5+3=8), not a max/first-file value: {}",
        html
    );
}

#[test]
fn stat_grid_warnings_tile_counts_warnings_only_io_excluded() {
    let writer = HtmlReportWriter::new();
    let file_metrics = make_metrics(5, 5)
        .with_warnings(vec![
            warning_in("f", WarningSeverity::Critical),
            warning_in("f", WarningSeverity::Warning),
        ])
        .with_io_in_loops(vec![io_in("f")]);
    let graph = graph_from(vec![("a.rs", file_metrics)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("\"label\":\"Warnings\",\"value\":\"2\",\"sub\":\"1 critical\""),
        "Warnings must count the 2 ComplexityWarning only (1 critical); \
         IoInLoopWarning has no severity and must never inflate either count: {}",
        html
    );
}

#[test]
fn stat_grid_io_tile_counts_io_in_loops() {
    let writer = HtmlReportWriter::new();
    let file_metrics = make_metrics(5, 5).with_io_in_loops(vec![io_in("f"), io_in("g")]);
    let graph = graph_from(vec![("a.rs", file_metrics)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("\"label\":\"I/O in loops\",\"value\":\"2\",\"sub\":\"in loops\""),
        "I/O in loops must have its own tile, separate from Warnings: {}",
        html
    );
}

// #56 T2 — abstention (ADR-0010/ADR-0014 §4): project-total tile AND
// per-node aggregate, mirroring the Direct/Transitive/Hidden pattern (a
// scalar summed postorder), NOT the Warnings/I-O-in-loops pattern (a
// per-call Vec) — abstention must never become a per-line pseudo-warning,
// so there is deliberately no Vec here, only counts.
#[test]
fn stat_grid_unclassifiable_tile_shows_project_total() {
    let writer = HtmlReportWriter::new();
    let a = make_metrics(5, 5).with_unclassifiable_io_in_loops_count(2);
    let b = make_metrics(3, 3).with_unclassifiable_io_in_loops_count(1);
    let graph = graph_from(vec![("a.rs", a), ("b.rs", b)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("\"label\":\"Unclassifiable\",\"value\":\"3\""),
        "Unclassifiable tile must show the project SUM (2+1=3): {}",
        html
    );
}

#[test]
fn node_metrics_include_unclassifiable_io_calls_aggregated_to_root() {
    let writer = HtmlReportWriter::new();
    let a = make_metrics(5, 5).with_unclassifiable_io_in_loops_count(2);
    let b = make_metrics(3, 3).with_unclassifiable_io_in_loops_count(1);
    let graph = graph_from(vec![("a.rs", a), ("b.rs", b)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");
    let data = extract_data_island(&html);

    let root = data["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .find(|n| n["id"] == "")
        .expect("root node with id \"\"");
    let root_metric = root["metrics"]
        .as_array()
        .expect("root metrics array")
        .iter()
        .find(|m| m["label"] == "Unclassifiable I/O calls")
        .expect("root metric 'Unclassifiable I/O calls' not found");

    assert_eq!(
        root_metric["value"], "3",
        "the root node's own detail must show the SAME postorder-summed total \
         (2+1=3) as the project stat tile"
    );
}

#[test]
fn every_stat_tile_matches_the_tree_root() {
    let writer = HtmlReportWriter::new();
    let a = make_metrics(5, 9).with_warnings(vec![warning_in("f", WarningSeverity::Critical)]);
    let b = make_metrics(3, 4);
    let graph = graph_from(vec![("a.rs", a), ("b.rs", b)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");
    let data = extract_data_island(&html);

    let stats = data["stats"].as_array().expect("stats array");
    let root = data["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .find(|n| n["id"] == "")
        .expect("root node with id \"\"");

    let root_metric = |label: &str| -> String {
        root["metrics"]
            .as_array()
            .expect("root metrics array")
            .iter()
            .find(|m| m["label"] == label)
            .unwrap_or_else(|| panic!("root metric '{}' not found", label))["value"]
            .as_str()
            .expect("metric value is a string")
            .to_string()
    };
    let stat_value = |label: &str| -> String {
        stats
            .iter()
            .find(|s| s["label"] == label)
            .unwrap_or_else(|| panic!("stat tile '{}' not found", label))["value"]
            .as_str()
            .expect("stat value is a string")
            .to_string()
    };
    let stat_sub = |label: &str| -> String {
        stats
            .iter()
            .find(|s| s["label"] == label)
            .unwrap_or_else(|| panic!("stat tile '{}' not found", label))["sub"]
            .as_str()
            .expect("stat sub is a string")
            .to_string()
    };

    assert_eq!(
        stat_value("Direct \u{3a3}"),
        root_metric("Direct complexity"),
        "Direct \u{3a3} tile must match the root node detail"
    );
    assert_eq!(
        stat_value("Transitive \u{3a3}"),
        root_metric("Transitive complexity"),
        "Transitive \u{3a3} tile must match the root node detail"
    );
    assert_eq!(
        stat_value("Max depth"),
        root_metric("Max call depth"),
        "Max depth tile must match the root node detail"
    );

    let hidden_from_sub = stat_sub("Transitive \u{3a3}")
        .split(' ')
        .next()
        .expect("sub is \"{n} hidden\"")
        .to_string();
    assert_eq!(
        hidden_from_sub,
        root_metric("Hidden complexity"),
        "the \"N hidden\" sub must match the root node's Hidden complexity detail"
    );
}

#[test]
fn stat_grid_shows_dash_when_no_economic_impact() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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
        (
            "x/f1.rs",
            CodeMetrics::with_call_graph(3, 3, 2, vec![], vec![]),
        ),
        (
            "y/f2.rs",
            CodeMetrics::with_call_graph(5, 5, 7, vec![], vec![]),
        ),
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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
    // make_metrics_measured (not make_metrics): this test's intent is the
    // worst-of-children ranking itself, which needs real "low"/"critical"
    // levels to discriminate — D3's "none" state would collapse every file
    // to the same rank and make the assertion vacuous.
    let graph = graph_from(vec![
        ("a/ok1.rs", make_metrics_measured(1, 1)),
        ("a/ok2.rs", make_metrics_measured(2, 2)),
        ("a/bad.rs", make_metrics_measured(50, 50)),
        ("a/ok3.rs", make_metrics_measured(3, 3)),
        ("a/ok4.rs", make_metrics_measured(4, 4)),
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains(
            r#""metrics":[{"label":"Direct complexity","value":"0","pct":0},{"label":"Transitive complexity","value":"0","pct":0},{"label":"Hidden complexity","value":"0","pct":0},{"label":"Max call depth","value":"0","pct":0},{"label":"Unclassifiable I/O calls","value":"0","pct":0}]"#
        ),
        "all-zero metrics (scale == 0) must yield pct 0 for every metric, not divide by zero \
         (#56 T2 adds the Unclassifiable I/O calls metric, always pct 0 by design): {}",
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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    // D3 (#50 slice S4): make_metrics() carries no function_details, so
    // both files now correctly read "none" ("nothing to measure"). This
    // test's intent (pct floors at 5%, caps at 100%) is independent of the
    // level string and is unaffected.
    assert!(
        html.contains(
            r#""id":"tiny.rs","name":"tiny.rs","kind":"file","path":"tiny.rs","child_ids":[],"score":1,"level":"none","metrics":[{"label":"Direct complexity","value":"1","pct":5}"#
        ),
        "a small nonzero value (1/100=1%) must floor at 5%, not round down to 0: {}",
        html
    );
    assert!(
        html.contains(
            r#""id":"huge.rs","name":"huge.rs","kind":"file","path":"huge.rs","child_ids":[],"score":100,"level":"none","metrics":[{"label":"Direct complexity","value":"100","pct":100}"#
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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "my-project")
        .expect("write_html should succeed");

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
        FunctionDetail::new(
            "a".to_string(),
            CodeLocation::new("f.rs".into(), 10, 1),
            1,
            1,
            1,
            false,
        ),
        FunctionDetail::new(
            "b".to_string(),
            CodeLocation::new("f.rs".into(), 20, 1),
            3,
            1,
            2,
            true,
        ),
    ]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains(r#""economic":{"cpu":"$0.000006","memory":"1.0 KB","total":"$0.000006","level":"low"}"#),
        "folder economic must be the domain SUM (fold via EconomicImpact::Add) of its children — never a coefficient recomputed from transitive complexity: {}",
        html
    );
}

#[test]
fn folder_ecological_class_is_recomputed_from_summed_co2() {
    let writer = HtmlReportWriter::new();
    let m1 = make_metrics(1, 1).with_ecological_impact(EcologicalImpact::new(
        0.6,
        100.0,
        EfficiencyClass::A,
    ));
    let m2 = make_metrics(1, 1).with_ecological_impact(EcologicalImpact::new(
        0.6,
        100.0,
        EfficiencyClass::A,
    ));
    let graph = graph_from(vec![("a/one.rs", m1), ("a/two.rs", m2)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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
    let metrics = make_metrics(1, 1).with_function_details(vec![FunctionDetail::new(
        payload.to_string(),
        CodeLocation::new("f.rs".into(), 1, 1),
        1,
        0,
        1,
        false,
    )]);
    let graph = graph_from(vec![("f.rs", metrics)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

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

// ── US7 T2 slice S4: embedded base64 @font-face (ADR-8.11) ──
//
// Test List (S4):
// 1. two @font-face blocks are present, each with a data:font/woff2;base64,
//    src — the report renders in Barlow / Barlow Condensed, not the system
//    fallback, with zero network request
// 2. no remote URL anywhere in the document — every `url(` is `url(data:`,
//    preserving (and strengthening) the zero-network-request invariant

#[test]
fn font_faces_are_embedded_as_data_uris() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert_eq!(
        html.matches("@font-face").count(),
        2,
        "expected exactly two @font-face blocks (Barlow 400, Barlow Condensed 600): {}",
        html
    );
    assert_eq!(
        html.matches("src:url(data:font/woff2;base64,").count(),
        2,
        "both font faces must be embedded as base64 data URIs, not a network font: {}",
        html
    );
}

#[test]
fn report_contains_no_remote_url() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        !html.contains("http://"),
        "report must not reference a remote http:// URL: {}",
        html
    );
    assert!(
        !html.contains("https://"),
        "report must not reference a remote https:// URL: {}",
        html
    );
    let total_urls = html.matches("url(").count();
    let data_urls = html.matches("url(data:").count();
    assert_eq!(
        total_urls, data_urls,
        "every url( in the report must be url(data: — no remote asset: {}",
        html
    );
}

// ── Bug found during manual verification (dogfooding the real CLI, not a
// spec-listed test): the real FileSystemCodeReader canonicalizes every file
// path it returns (file_system_code_reader.rs's `list_rust_files`), while
// `target` reaches `write_html` as the RAW, un-canonicalized `--path` CLI
// argument (run_analysis.rs's `handle_project_html`: `target.path().to_
// string_lossy()`). `strip_prefix` against the raw target therefore almost
// always fails in real usage — even when target and the files' common root
// are the SAME directory — and node_id() falls back to the file's full
// path, producing a degenerate single-child folder chain that mirrors the
// entire filesystem path instead of a real, small project tree. Neither
// the tech spec's §0 "verified findings" nor its `node_id` design anticipated
// this — it assumed `target` and file paths already share a literal prefix.
// Fixed in view_model.rs by canonicalizing `target` before stripping,
// falling back to the raw string when canonicalization fails (e.g. these
// tests' fixture paths, which do not exist on disk) — so this fix changes
// NOTHING for the fixture-based tests above, only real filesystem targets.
#[test]
fn tree_ids_are_relative_when_target_resolves_to_the_files_common_root() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let target = dir.path().to_string_lossy().to_string();
    let canonical_dir = std::fs::canonicalize(dir.path()).expect("canonicalize temp dir");
    let file_path = canonical_dir.join("main.rs");

    let writer = HtmlReportWriter::new();
    let entries = vec![(file_path, make_metrics(1, 1))];
    let graph = FileConsumptionGraph::build(&entries, vec![]).unwrap();

    let html = writer
        .write_html(&graph, &target)
        .expect("write_html should succeed");

    assert!(
        html.contains(r#""id":"main.rs","name":"main.rs","kind":"file""#),
        "when target resolves (canonicalization/symlinks) to the files' common root, the tree \
         must nest the file under a short relative id, not the full absolute path exploded into a \
         single-child folder chain from the filesystem root: {}",
        html
    );
}

// ── BLOCKER 1 (#50 QA retry 1) — the HTML report never surfaced
// unmeasurable_files: no flag, no count, nothing. AC-7 ("a file that could
// not be measured must be reported as NOT MEASURED, never as trivial") was
// satisfied in JSON and console, and NOT in HTML — the report's own
// headline failure mode, reproduced one surface later, invisibly. ──
//
// Test List:
// 1. the data island carries the unmeasurable file's path and count
// 2. the data island carries the human-readable reason
// 3. the renderer JS actually consumes data.unmeasurable_files (not merely
//    embedded-and-ignored)
// 4. a graph with zero unmeasurable files carries an empty array, no panic
// 5. a script-breakout payload in the unmeasurable path is neutralized —
//    same mechanism (json_island_escape + textContent-only rendering) as
//    every other code-derived value in this report, reused, not reinvented

#[test]
fn write_html_surfaces_unmeasurable_files_path_and_count() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![("src/good.rs", 1, 1)]).with_unmeasurable_files(vec![
        UnmeasurableFile {
            path: PathBuf::from("src/bad.rs"),
            reason: UnmeasurableReason::SourceUnparseable,
        },
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains(r#""unmeasurable_files":[{"path":"src/bad.rs""#),
        "the unmeasurable file's path must be surfaced in the report's data, got: {}",
        html
    );
}

#[test]
fn write_html_surfaces_unmeasurable_reason_in_human_readable_form() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![("src/good.rs", 1, 1)]).with_unmeasurable_files(vec![
        UnmeasurableFile {
            path: PathBuf::from("src/bad.rs"),
            reason: UnmeasurableReason::SourceUnparseable,
        },
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("code source non analysable"),
        "the unmeasurable file's reason must be surfaced in human-readable form (same as the \
         console writer's Display text), got: {}",
        html
    );
}

#[test]
fn rendered_js_consumes_unmeasurable_files_from_the_data_island() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("data.unmeasurable_files"),
        "the renderer JS must actually consume unmeasurable_files, not merely embed it unused \
         in the data island: {}",
        html
    );
}

#[test]
fn write_html_no_unmeasurable_files_yields_empty_array_no_panic() {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![("src/good.rs", 1, 1)]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed for a project with no unmeasurable files");

    assert!(
        html.contains(r#""unmeasurable_files":[]"#),
        "a project with no unmeasurable files must carry an empty array, not omit the field: {}",
        html
    );
}

#[test]
fn write_html_neutralizes_script_breakout_payload_in_unmeasurable_path() {
    let writer = HtmlReportWriter::new();
    let payload = "</script><script>alert(1)</script>bad.rs";
    let graph = graph_with_files(vec![("src/good.rs", 1, 1)]).with_unmeasurable_files(vec![
        UnmeasurableFile {
            path: PathBuf::from(payload),
            reason: UnmeasurableReason::SourceUnparseable,
        },
    ]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        !html.contains("</script><script>alert(1)</script>"),
        "unmeasurable file path payload must not appear as a literal script breakout: {}",
        html
    );
    assert_eq!(
        html.matches("<script").count(),
        2,
        "unmeasurable file path payload must not add a third <script> tag: {}",
        html
    );
}

// US8 slice 3 (AC3) — HTML renders a banner on a breach (AD-3: same shared
// renderer's data feeds both console text and this structured banner).
//
// Test List:
// 1. a breaching threshold_report -> has_breach true + the breach's metric
//    in the data island
// 2. no threshold_report attached at all -> has_breach false (never
//    omitted — same "0/false is honest" convention as unmeasurable_files)
// 3. the renderer JS actually consumes data.thresholds, not merely embeds
//    it unused (mirrors rendered_js_consumes_unmeasurable_files_...)

#[test]
fn write_html_surfaces_threshold_breach_in_the_data_island() {
    let writer = HtmlReportWriter::new();
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.00002), None);
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]).with_threshold_report(report);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(html.contains(r#""has_breach":true"#), "got: {}", html);
    assert!(
        html.contains("ÉNERGIE"),
        "expected the energy metric label in the data island, got: {}",
        html
    );
}

#[test]
fn write_html_no_threshold_report_shows_has_breach_false() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains(r#""has_breach":false"#),
        "no threshold was ever evaluated, must still report false honestly: {}",
        html
    );
}

#[test]
fn rendered_js_consumes_thresholds_from_the_data_island() {
    let writer = HtmlReportWriter::new();
    let graph = graph_from(vec![("a.rs", make_metrics(1, 1))]);

    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    assert!(
        html.contains("data.thresholds"),
        "the renderer JS must actually consume thresholds, not merely embed it unused: {}",
        html
    );
}
