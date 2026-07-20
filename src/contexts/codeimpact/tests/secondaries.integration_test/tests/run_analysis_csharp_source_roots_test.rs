use codeimpact_hexagon::analysis::AnalysisConfig;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::ParserRegistry;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_parsers::tree_sitter::tree_sitter_code_parser::TreeSitterCodeParser;
use codeimpact_secondaries::gateways::code_readers::file_system_code_reader::FileSystemCodeReader;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;

// Security CRITICAL (Dev-B, retry #1) — `resolve_source_roots`
// (run_analysis.rs) joined the RAW `project_root` (the CLI's
// un-canonicalized `--path`) with a configured `sourceRoots` entry, then
// compared the result against `FileSystemCodeReader::list_source_files`'s
// CANONICALIZED absolute paths via `Path::starts_with` in
// `TreeSitterCodeParser::under_any_root` — a real-disk representation
// mismatch that silently emptied the namespace index whenever
// `sourceRoots` was actually populated, the ONE configuration this slice
// exists to support. A hand-built `DependencyContext` fixture (as used by
// `tree_sitter_code_parser.rs`'s own unit tests) cannot reproduce this —
// both sides are constructed consistently by the test itself there. This
// test goes through the REAL `FileSystemCodeReader` (which canonicalizes,
// exactly like production) against a REAL temp directory, with the
// declaring file placed INSIDE the configured `sourceRoots` entry — the
// case QA's fixture (declarer OUTSIDE the root) could not have caught,
// since "no edge" was the correct answer there either way.
//
// Test List:
// 1. sourceRoots=["src"], both files under src/, mutual `using` between
//    them -> the cycle IS detected (proves the namespace index is not
//    silently empty when sourceRoots is populated)

fn isolated_project_dir(test_name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_source_roots_real_fs_{}_{}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("create isolated project dir");
    dir
}

#[test]
fn source_roots_configured_and_declarer_inside_root_still_resolves_the_edge() {
    let dir = isolated_project_dir("mutual_cycle");

    std::fs::write(
        dir.join("src").join("FileA.cs"),
        "using B;\nnamespace A { class Foo {} }",
    )
    .expect("write FileA.cs");
    std::fs::write(
        dir.join("src").join("FileB.cs"),
        "using A;\nnamespace B { class Bar {} }",
    )
    .expect("write FileB.cs");

    let target = AnalysisTarget::new(dir.clone(), TargetType::Project);
    let config = AnalysisConfig::defaults().with_source_roots(vec!["src".to_string()]);
    let use_case = RunAnalysis::new(
        Box::new(FileSystemCodeReader::new()),
        Box::new(JsonReportWriter::new()),
        ParserRegistry::new().register(Language::CSharp, Box::new(TreeSitterCodeParser::csharp())),
    );

    let result =
        use_case.handle_project_json(&target, &[AnalysisRule::CyclomaticComplexity], &config);
    let _ = std::fs::remove_dir_all(&dir);

    let json = result
        .expect("handle_project_json should succeed")
        .into_payload();
    assert!(
        json.contains("FileA.cs") && json.contains("FileB.cs"),
        "with sourceRoots=[\"src\"] and BOTH declaring files inside src/, \
         the mutual `using` must still resolve into a detected cycle \
         (functions_with_cycles) — got: {}",
        json
    );
}
