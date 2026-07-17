use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FunctionDetail;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::UnmeasurableFile;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;

// Test List:
// 1. json_report_writer writes valid JSON with all fields
// 2. json_report_writer handles empty metrics
// 3. json_report_writer includes economic/ecological impact in JSON
// 4. json_report_writer includes warnings in JSON
// 5. json_report_writer includes io_in_loops in JSON
// 6. json_report_writer includes function_details in JSON
// 7. json_report_writer write_console returns error
// 8. console_report_writer write_json returns valid JSON too

fn make_metrics_with_impacts() -> CodeMetrics {
    let economic = EconomicImpact::new(12.5, 5000, 13.0, "moderate");
    let ecological = EcologicalImpact::new(2.4, 21600.0, EfficiencyClass::B);
    CodeMetrics::with_call_graph(
        5,
        8,
        2,
        vec!["foo".into()],
        vec![codeimpact_hexagon::analysis::FunctionDetail::new(
            "main".into(),
            codeimpact_hexagon::analysis::CodeLocation::new("src/main.rs".into(), 1, 1),
            5,
            3,
            2,
            false,
        )],
    )
    .with_economic_impact(economic)
    .with_ecological_impact(ecological)
    .with_io_in_loops(vec![IoInLoopWarning {
        function: "read_file".into(),
        io_call: "std::fs::read".into(),
        location: CodeLocation::new("src/main.rs".into(), 5, 9),
    }])
}

#[test]
fn json_writer_produces_valid_json() {
    let writer = JsonReportWriter::new();
    let metrics = make_metrics_with_impacts();
    let result = writer.write_json(&metrics, "test.rs", "file");

    assert!(result.is_ok(), "write_json should succeed");
    let json_str = result.unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&json_str).expect("output should be valid JSON");

    // Check tool metadata
    assert_eq!(json["tool"]["name"], "codeimpact");
    assert!(json["tool"]["version"].is_string());
    assert!(json["timestamp"].is_string());
    assert_eq!(json["target"], "test.rs");
    assert_eq!(json["target_type"], "file");

    // Check metrics
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 5);
    assert_eq!(json["metrics"]["transitive_complexity"], 8);
    assert_eq!(json["metrics"]["hidden_complexity"], 3);
    assert_eq!(json["metrics"]["max_call_depth"], 2);
    assert_eq!(json["metrics"]["complexity_level"], "low");
    assert_eq!(json["metrics"]["functions_with_cycles"][0], "foo");

    // Check function_details
    let details = &json["metrics"]["function_details"][0];
    assert_eq!(details["name"], "main");
    assert_eq!(details["direct"], 5);
    assert_eq!(details["transitive"], 8);
    assert_eq!(details["call_depth"], 2);
    assert_eq!(details["in_cycle"], false);
    assert_eq!(details["location"]["file"], "src/main.rs");
    assert_eq!(details["location"]["line"], 1);
    assert_eq!(details["location"]["col"], 1);

    // Check economic impact
    let econ = &json["metrics"]["economic_impact"];
    assert!((econ["cpu_cost_microdollars"].as_f64().unwrap() - 12.5).abs() < 1e-9);
    assert_eq!(econ["memory_bytes"], 5000);
    assert!((econ["total_cost_microdollars"].as_f64().unwrap() - 13.0).abs() < 1e-9);
    assert_eq!(econ["level"], "moderate");

    // Check ecological impact
    let eco = &json["metrics"]["ecological_impact"];
    assert!((eco["co2_grams"].as_f64().unwrap() - 2.4).abs() < 1e-9);
    assert!((eco["energy_joules"].as_f64().unwrap() - 21600.0).abs() < 1e-9);
    assert_eq!(eco["efficiency_class"], "B");

    // Check io_in_loops
    let io = &json["metrics"]["io_in_loops"][0];
    assert_eq!(io["function"], "read_file");
    assert_eq!(io["io_call"], "std::fs::read");
    assert_eq!(io["location"]["file"], "src/main.rs");
    assert_eq!(io["location"]["line"], 5);
    assert_eq!(io["location"]["col"], 9);
}

#[test]
fn json_writer_empty_metrics() {
    let writer = JsonReportWriter::new();
    let metrics = CodeMetrics::new(0);
    let result = writer.write_json(&metrics, "empty.rs", "file");

    assert!(
        result.is_ok(),
        "write_json with empty metrics should succeed"
    );
    let json_str = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    // D3 (#50 slice S4): CodeMetrics::new(0) carries no function_details, so
    // this now correctly reads "none" ("nothing to measure"), not a
    // fabricated "low".
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 0);
    assert_eq!(json["metrics"]["complexity_level"], "none");
}

#[test]
fn json_writer_write_console_returns_error() {
    let writer = JsonReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let result = writer.write_console(&metrics);

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!("expected AnalysisFailed, got {:?}", result),
    }
}

#[test]
fn console_writer_write_json_produces_valid_json() {
    let writer = ConsoleReportWriter::new();
    let metrics = make_metrics_with_impacts();
    let result = writer.write_json(&metrics, "test.rs", "file");

    assert!(result.is_ok(), "console writer write_json should succeed");
    let json_str = result.unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&json_str).expect("output should be valid JSON");

    // Same schema checks
    assert_eq!(json["tool"]["name"], "codeimpact");
    assert_eq!(json["target"], "test.rs");
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 5);
    assert_eq!(json["metrics"]["economic_impact"]["level"], "moderate");
}

// #60: serialize_project_metrics fed complexity_level_for (the PER-FILE
// scale) with the PROJECT TOTAL, so any project of non-trivial size read
// "critical" regardless of its actual health. It must now read the MEDIAN
// per-file complexity instead — the number that stays on that scale.
fn make_measured_file(cc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(
        cc,
        cc,
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

#[test]
fn project_json_complexity_level_reflects_median_not_total() {
    let writer = JsonReportWriter::new();

    // 9 tiny files (cc=2) + 2 huge files (cc=200): total=418 is "critical",
    // median=2 is "low".
    let mut files: Vec<(PathBuf, CodeMetrics)> = (0..9)
        .map(|i| (PathBuf::from(format!("tiny{i}.rs")), make_measured_file(2)))
        .collect();
    files.push((PathBuf::from("huge1.rs"), make_measured_file(200)));
    files.push((PathBuf::from("huge2.rs"), make_measured_file(200)));

    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let json_str = writer
        .write_project_json(&graph, "proj")
        .expect("write_project_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["cyclomatic_complexity"], 418);
    assert_eq!(
        json["metrics"]["complexity_level"], "low",
        "the median (2), not the total (418, off-scale), must drive the level: {}",
        json_str
    );
}

#[test]
fn project_json_complexity_level_empty_project_is_none() {
    let writer = JsonReportWriter::new();
    let graph = FileConsumptionGraph::build(&[], vec![]).unwrap();

    let json_str = writer
        .write_project_json(&graph, "proj")
        .expect("write_project_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["complexity_level"], "none");
}

// D3 (#50 slice S4), test case 20 — project JSON must surface unmeasurable
// files (additive per ADR-0007: no existing field removed or renamed).
#[test]
fn project_json_includes_unmeasurable_files_and_keeps_existing_fields_unchanged() {
    let writer = JsonReportWriter::new();
    let files = vec![(
        PathBuf::from("a.rs"),
        CodeMetrics::with_call_graph(
            5,
            8,
            0,
            vec![],
            vec![FunctionDetail::new(
                "f".to_string(),
                CodeLocation::new("a.rs".into(), 1, 1),
                5,
                0,
                0,
                false,
            )],
        ),
    )];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(vec![UnmeasurableFile {
            path: PathBuf::from("bad.rs"),
            reason: UnmeasurableReason::SourceUnparseable,
        }]);

    let result = writer.write_project_json(&graph, "proj");
    assert!(result.is_ok(), "write_project_json should succeed");
    let json_str = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    // ADR-0007 schema non-regression: every pre-existing field is still there.
    assert_eq!(json["tool"]["name"], "codeimpact");
    assert!(json["tool"]["version"].is_string());
    assert!(json["timestamp"].is_string());
    assert_eq!(json["target"], "proj");
    assert_eq!(json["target_type"], "project");
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 5);
    assert_eq!(json["metrics"]["transitive_complexity"], 8);

    // New, additive field.
    let unmeasurable = json["metrics"]["unmeasurable_files"]
        .as_array()
        .expect("unmeasurable_files should be an array");
    assert_eq!(unmeasurable.len(), 1);
    assert_eq!(unmeasurable[0]["path"], "bad.rs");
    assert_eq!(unmeasurable[0]["reason"], "SourceUnparseable");
    assert_eq!(json["metrics"]["unmeasurable_files_count"], 1);
}

#[test]
fn file_json_reports_zero_unmeasurable_files() {
    let writer = JsonReportWriter::new();
    let metrics = make_metrics_with_impacts();

    let json_str = writer.write_json(&metrics, "test.rs", "file").unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["unmeasurable_files_count"], 0);
    assert!(
        json["metrics"]["unmeasurable_files"].is_null()
            || json["metrics"]["unmeasurable_files"]
                .as_array()
                .unwrap()
                .is_empty(),
        "a single-file report has no notion of other unmeasurable files"
    );
}

// #56 T2 — abstention (ADR-0010/ADR-0014 §4): the count is never skipped,
// same convention as unmeasurable_files_count. Additive field (ADR-0007).
#[test]
fn file_json_includes_unclassifiable_io_in_loops_count() {
    let writer = JsonReportWriter::new();
    let metrics = make_metrics_with_impacts().with_unclassifiable_io_in_loops_count(2);

    let json_str = writer.write_json(&metrics, "test.rs", "file").unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["unclassifiable_io_in_loops_count"], 2);
}

#[test]
fn file_json_reports_zero_unclassifiable_io_in_loops_by_default() {
    let writer = JsonReportWriter::new();
    let metrics = make_metrics_with_impacts();

    let json_str = writer.write_json(&metrics, "test.rs", "file").unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["unclassifiable_io_in_loops_count"], 0);
}

#[test]
fn project_json_sums_unclassifiable_io_in_loops_count_across_files() {
    let writer = JsonReportWriter::new();
    let files = vec![
        (
            PathBuf::from("a.rs"),
            make_measured_file(5).with_unclassifiable_io_in_loops_count(2),
        ),
        (
            PathBuf::from("b.rs"),
            make_measured_file(3).with_unclassifiable_io_in_loops_count(1),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();

    let json_str = writer
        .write_project_json(&graph, "proj")
        .expect("write_project_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["unclassifiable_io_in_loops_count"], 3);
}

// US8 slice 3 (AC3) — JSON embeds a thresholds/breaches object (AD-3: the
// message field reuses the ONE shared renderer, same text as console/HTML).
//
// Test List:
// 1. single-file JSON with a breaching threshold_report -> has_breach true,
//    breaches array with the metric/limit/actual/excess, non-empty message
// 2. single-file JSON with no threshold_report attached at all -> has_breach
//    false, empty breaches array (never omitted, same "0 is honest"
//    convention as unclassifiable_io_in_loops_count)
// 3. project JSON with a breaching threshold_report -> has_breach true

#[test]
fn json_writer_includes_thresholds_object_with_a_breach() {
    let writer = JsonReportWriter::new();
    let thresholds = AlertThresholds::new(Some(10.0), None).unwrap();
    let report = thresholds.evaluate(Some(15.0), None);
    let metrics = CodeMetrics::new(5).with_threshold_report(report);

    let json_str = writer
        .write_json(&metrics, "test.rs", "file")
        .expect("write_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["thresholds"]["has_breach"], true);
    let breaches = json["metrics"]["thresholds"]["breaches"]
        .as_array()
        .expect("breaches should be an array");
    assert_eq!(breaches.len(), 1);
    assert_eq!(breaches[0]["metric"], "CPU");
    assert_eq!(breaches[0]["limit"], 10.0);
    assert_eq!(breaches[0]["actual"], 15.0);
    assert_eq!(breaches[0]["excess"], 5.0);
    assert!(
        !json["metrics"]["thresholds"]["message"]
            .as_str()
            .unwrap()
            .is_empty(),
        "a breach must carry a non-empty human-readable message, got: {}",
        json
    );
}

#[test]
fn json_writer_no_threshold_report_shows_no_breach_and_empty_array() {
    let writer = JsonReportWriter::new();
    let metrics = CodeMetrics::new(5); // no threshold_report attached at all

    let json_str = writer
        .write_json(&metrics, "test.rs", "file")
        .expect("write_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["thresholds"]["has_breach"], false);
    assert_eq!(
        json["metrics"]["thresholds"]["breaches"]
            .as_array()
            .expect("breaches should be an array, never omitted")
            .len(),
        0
    );
}

#[test]
fn project_json_writer_includes_thresholds_object_with_a_breach() {
    let writer = JsonReportWriter::new();
    let files = vec![(PathBuf::from("a.rs"), CodeMetrics::new(5))];
    let thresholds = AlertThresholds::new(Some(1.0), None).unwrap();
    let report = thresholds.evaluate(Some(5.0), None);
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_threshold_report(report);

    let json_str = writer
        .write_project_json(&graph, "proj")
        .expect("write_project_json should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["thresholds"]["has_breach"], true);
    assert_eq!(
        json["metrics"]["thresholds"]["breaches"][0]["metric"],
        "CPU"
    );
}
