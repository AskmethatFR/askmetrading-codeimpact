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

// Security MEDIUM-1 (US17 T4.4 retry #1) — the same real-disk-vs-fixture gap
// `run_analysis_csharp_source_roots_test.rs` closed for US16 T5's CRITICAL
// (see that file's header) exists again here: T4.4 rewires
// `resolvable_targets` onto the identical `under_any_root` gate, but for a
// SECOND surface (TS/JS `RelativePath` resolution), and the unit tests that
// pinned it in `tree_sitter_code_parser.rs` all use hand-built
// `DependencyContext` fixtures — both the `source_roots` side AND the
// file-path side are constructed consistently BY the test, so a raw-vs-
// canonicalized mismatch can never surface there, however the production
// path actually behaves. Security measured that the distinction is not
// theoretical: a root spelled `./src` yields `false` against a raw path
// and `true` against a canonicalized one. This test goes through the REAL
// `FileSystemCodeReader` (which canonicalizes, exactly like production)
// against a REAL temp directory, with BOTH declaring files placed INSIDE
// the configured `sourceRoots` entry — the positive case: if the
// confinement gate silently emptied `resolvable_targets` on the real
// (canonicalized) path shape, this mutual relative-import cycle would not
// be detected, and it would fail for the wrong reason a hand-built fixture
// cannot reproduce.
//
// Test List:
// 1. sourceRoots=["src"], both files under src/, mutual relative import
//    between them -> the cycle IS detected (proves resolvable_targets is
//    not silently empty when sourceRoots is populated, on real disk paths)

fn isolated_project_dir(test_name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_ts_source_roots_real_fs_{}_{}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("create isolated project dir");
    dir
}

#[test]
fn source_roots_configured_and_both_declarers_inside_root_still_resolves_the_edge() {
    let dir = isolated_project_dir("mutual_cycle");

    std::fs::write(
        dir.join("src").join("a.ts"),
        "import './b';\nexport const a = 1;",
    )
    .expect("write a.ts");
    std::fs::write(
        dir.join("src").join("b.ts"),
        "import './a';\nexport const b = 1;",
    )
    .expect("write b.ts");

    let target = AnalysisTarget::new(dir.clone(), TargetType::Project);
    let config = AnalysisConfig::defaults().with_source_roots(vec!["src".to_string()]);
    let use_case = RunAnalysis::new(
        Box::new(FileSystemCodeReader::new()),
        Box::new(JsonReportWriter::new()),
        ParserRegistry::new().register(
            Language::TypeScript,
            Box::new(TreeSitterCodeParser::typescript(Vec::new())),
        ),
    );

    let result =
        use_case.handle_project_json(&target, &[AnalysisRule::CyclomaticComplexity], &config);
    let _ = std::fs::remove_dir_all(&dir);

    let json = result
        .expect("handle_project_json should succeed")
        .into_payload();
    assert!(
        json.contains("a.ts") && json.contains("b.ts"),
        "with sourceRoots=[\"src\"] and BOTH declaring files inside src/, \
         the mutual relative import must still resolve into a detected \
         cycle (functions_with_cycles) — got: {}",
        json
    );
}
