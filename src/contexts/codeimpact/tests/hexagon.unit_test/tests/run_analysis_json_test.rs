use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::AnalysisConfig;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::FileFilter;
use codeimpact_hexagon::analysis::GateCoverage;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::ParserRegistry;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;
use codeimpact_secondaries::gateways::code_parsers::tree_sitter::tree_sitter_code_parser::TreeSitterCodeParser;
use codeimpact_secondaries::gateways::code_readers::code_reader_stub::CodeReaderStub;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::File)
}

fn make_project_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::Project)
}

// Test List:
// 1. handle_json returns a non-empty string for valid file
// 2. handle_json with nonexistent file returns error
// 3. handle_project_json returns a non-empty string for project target
//
// Test List (US16 T5 — C# cross-file dependency graph, slice-level/
// behavioral: real TreeSitterCodeParser::csharp() through the full
// RunAnalysis wiring, the user-observable outcome of the slice):
// 4. a 2-file C# project where fileA `using`s fileB's namespace and
//    vice versa is reported as a real dependency CYCLE in the project
//    JSON (functions_with_cycles) — proves config -> run_analysis ->
//    DependencyContext -> TreeSitterCodeParser -> FileConsumptionGraph is
//    wired end-to-end, with NO sourceRoots configured (AnalysisConfig::
//    defaults()), pinning the "absent sourceRoots" default in the same
//    breath.

#[test]
fn handle_json_returns_string_for_valid_file() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(
        PathBuf::from("test.rs"),
        "fn test() { if x > 0 { } }".into(),
    );
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "test".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_json(
        &make_target("test.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    assert!(result.is_ok(), "handle_json should succeed");
    let json = result.unwrap().into_payload();
    assert!(!json.is_empty(), "JSON string should not be empty");
    assert!(json.contains("test.rs"), "JSON should contain target path");
    assert!(json.contains("codeimpact"), "JSON should contain tool name");
}

#[test]
fn handle_json_nonexistent_file_returns_error() {
    let reader = CodeReaderStub::new();
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_json(
        &make_target("nonexistent.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::IoError(_)) => {}
        _ => panic!("expected IoError, got {:?}", result),
    }
}

#[test]
fn handle_project_json_returns_string() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "main".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    assert!(result.is_ok(), "handle_project_json should succeed");
    let json = result.unwrap().into_payload();
    assert!(!json.is_empty(), "JSON string should not be empty");
    assert!(
        json.contains("project"),
        "project JSON should contain target_type project"
    );
}

#[test]
fn csharp_project_with_mutual_using_reports_a_dependency_cycle() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(
        PathBuf::from("FileA.cs"),
        "using B;\nnamespace A { class Foo {} }".into(),
    );
    reader.add_source_file(PathBuf::from("FileA.cs"));
    reader.add_source(
        PathBuf::from("FileB.cs"),
        "using A;\nnamespace B { class Bar {} }".into(),
    );
    reader.add_source_file(PathBuf::from("FileB.cs"));

    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(JsonReportWriter::new()),
        ParserRegistry::new().register(
            Language::CSharp,
            Box::new(TreeSitterCodeParser::csharp(Vec::new())),
        ),
    );

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    let json = result
        .expect("handle_project_json should succeed")
        .into_payload();
    assert!(
        json.contains("\"FileA.cs\"") && json.contains("\"FileB.cs\""),
        "both files of the mutual-using cycle must appear in functions_with_cycles: {}",
        json
    );
}

#[test]
fn handle_project_json_empty_project_returns_error() {
    let reader = CodeReaderStub::new(); // no files added
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!(
            "expected AnalysisFailed for empty project, got {:?}",
            result
        ),
    }
}

// BLOCKER 2 (#50 QA retry 1) — build_project_graph's unmeasurable branches
// (behind handle_project_json) had no test at all. Mirrors the console-path
// pins in run_analysis_test.rs (handle_project_records_un{readable,parseable}
// _file_as_unmeasurable) one layer up, on the JSON path.

#[test]
fn handle_project_json_records_unreadable_file_as_unmeasurable_and_excludes_it_from_sums() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs")); // no source configured — read_source fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_json must pass the built graph through");
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/bad.rs"));
    assert_eq!(unmeasurable[0].reason, UnmeasurableReason::SourceUnreadable);
    assert_eq!(
        graph.aggregated_metrics().unmeasurable_files,
        1,
        "aggregated_metrics must count it too"
    );
    assert_eq!(
        graph.aggregated_metrics().total_files,
        1,
        "the unreadable file must enter no sum — only good.rs counts as measured"
    );
}

#[test]
fn handle_project_json_records_unparseable_file_as_unmeasurable_and_excludes_it_from_sums() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source(PathBuf::from("src/bad.rs"), "@@@ not rust".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "@@@",
        codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed("parse error".to_string()),
    );
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_json must pass the built graph through");
    assert!(
        graph
            .per_file_metrics()
            .contains_key(&PathBuf::from("src/good.rs")),
        "good.rs should still be measured"
    );
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/bad.rs"));
    assert_eq!(
        unmeasurable[0].reason,
        UnmeasurableReason::SourceUnparseable
    );
    let pm = graph.aggregated_metrics();
    assert_eq!(pm.total_files, 1, "only good.rs counts as measured");
    assert_eq!(pm.unmeasurable_files, 1);
}

// US128 T2 (issue #128) — the console surface (`handle_project`,
// run_analysis_test.rs) already carries `GateCoverage` on its
// `GatedOutput`; this pins the SAME wiring on the JSON surface
// (`handle_project_json`), reusing the identical `derive_gate_coverage`
// helper. A prior ticket on this exact call-site pair (T4.2, see the
// dangling-edges tests above) caught a fix applied to one site and not its
// symmetric twin — this test exists so that mistake can't repeat here.

#[test]
fn handle_project_json_with_threshold_configured_and_unmeasured_files_reports_partial_coverage() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source(PathBuf::from("src/bad.rs"), "OVERSIZED".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]).failing_when_source_contains(
        "OVERSIZED",
        codeimpact_hexagon::analysis::AnalysisError::Unmeasurable(
            UnmeasurableReason::SourceTooLarge,
        ),
    );
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let config = AnalysisConfig::new(
        AlertThresholds::new(Some(1000.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle_project_json(
        &make_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &config,
    );

    let gated = result.expect("a project with a mix of good/bad files should still succeed");
    assert_eq!(
        gated.coverage(),
        GateCoverage::Partial {
            unmeasurable_files: 1
        },
        "the JSON surface's own GatedOutput must carry the same coverage the console surface does"
    );
}

// Security HIGH (retry 1, #128) — `build_project_graph_with_source_roots`
// (the JSON/HTML surfaces' shared helper) is a SEPARATE per-file pass from
// `handle_project`'s (the console surface) — the same walk-time-dropped-file
// fold-in must be wired on THIS call site too, or the JSON/HTML surfaces
// stay bypassable even after the console surface is fixed (mirrors the
// console-level `handle_project_folds_a_walk_time_dropped_file_into_
// unmeasurable_coverage` pin, run_analysis_test.rs).

// @scenario: alert-threshold-gating/S1
#[test]
fn handle_project_json_folds_a_walk_time_dropped_file_into_unmeasurable_coverage() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_dropped_file(
        PathBuf::from("src/huge.rs"),
        UnmeasurableReason::SourceTooLarge,
    );

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let config = AnalysisConfig::new(
        AlertThresholds::new(Some(1000.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle_project_json(
        &make_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &config,
    );

    let gated = result.expect("a project with one walk-time-dropped file should still succeed");
    assert_eq!(
        gated.coverage(),
        GateCoverage::Partial {
            unmeasurable_files: 1
        },
        "a file the adapter's WALK dropped must count toward the JSON surface's own coverage \
         too, not just the console surface's"
    );
}

// Security HIGH (Dev-B/Security, retry #1) — `read_all_sources` used to
// accumulate every project file's FULL source text into one `Vec` with
// only a per-file cap (`source_guard::MAX_MEASURABLE_SOURCE_BYTES`, 1 MB)
// and no ceiling on the SUM — hundreds of near-cap files could still OOM
// the scan (Security reproduced a multi-GB aggregate on a 4000-file C#
// project). `check_project_admissible` must stop the scan with a
// diagnostic, not silently truncate the file list or let the process
// exhaust memory.
//
// Test List:
// 1. two 60 MB fake sources (120 MB total, over the 100 MB
//    MAX_PROJECT_SOURCE_BYTES ceiling) -> handle_project_json returns
//    Err(AnalysisFailed(_)) naming the aggregate limit, not Ok/panic

#[test]
fn handle_project_json_aborts_when_aggregate_source_exceeds_the_project_ceiling() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("a.rs"), "a".repeat(60 * 1024 * 1024));
    reader.add_source_file(PathBuf::from("a.rs"));
    reader.add_source(PathBuf::from("b.rs"), "a".repeat(60 * 1024 * 1024));
    reader.add_source_file(PathBuf::from("b.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(msg)) => {
            assert!(
                msg.contains("Mo"),
                "expected a diagnostic naming the aggregate ceiling, got: {}",
                msg
            );
        }
        other => panic!("expected Err(AnalysisFailed(_)), got {:?}", other),
    }
}
