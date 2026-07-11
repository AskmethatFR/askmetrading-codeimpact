use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::ReportWriter;
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
    CodeMetrics::with_call_graph(5, 8, 2, vec!["foo".into()], vec![
        codeimpact_hexagon::analysis::FunctionDetail {
            name: "main".into(),
            direct: 5,
            transitive: 8,
            call_depth: 2,
            in_cycle: false,
        },
    ])
    .with_economic_impact(economic)
    .with_ecological_impact(ecological)
    .with_io_in_loops(vec![
        IoInLoopWarning {
            function: "read_file".into(),
            io_call: "std::fs::read".into(),
            location: CodeLocation::new("src/main.rs".into(), 5, 9),
        },
    ])
}

#[test]
fn json_writer_produces_valid_json() {
    let writer = JsonReportWriter::new();
    let metrics = make_metrics_with_impacts();
    let result = writer.write_json(&metrics, "test.rs", "file");

    assert!(result.is_ok(), "write_json should succeed");
    let json_str = result.unwrap();

    let json: serde_json::Value = serde_json::from_str(&json_str)
        .expect("output should be valid JSON");

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

    assert!(result.is_ok(), "write_json with empty metrics should succeed");
    let json_str = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(json["metrics"]["cyclomatic_complexity"], 0);
    assert_eq!(json["metrics"]["complexity_level"], "low");
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

    let json: serde_json::Value = serde_json::from_str(&json_str)
        .expect("output should be valid JSON");

    // Same schema checks
    assert_eq!(json["tool"]["name"], "codeimpact");
    assert_eq!(json["target"], "test.rs");
    assert_eq!(json["metrics"]["cyclomatic_complexity"], 5);
    assert_eq!(json["metrics"]["economic_impact"]["level"], "moderate");
}