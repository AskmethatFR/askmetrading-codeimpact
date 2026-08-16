use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FileDependency;
use codeimpact_hexagon::analysis::FunctionDetail;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::LanguageCapabilities;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::MetricSupport;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::UnmeasurableFile;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_hexagon::analysis::WarningPattern;
use codeimpact_hexagon::analysis::WarningSeverity;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use std::path::PathBuf;

#[test]
fn write_console_does_not_panic() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_zero_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(0);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_high_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(50);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_economic_impact() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(18.5, 12600, 19.7, "moderate");
    let metrics = CodeMetrics::new(27).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_memory_mb() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(50.0, 2_000_000, 50.2, "high");
    let metrics = CodeMetrics::new(30).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_ecological_impact() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(6000.0, 0, 6000.0, "low");
    let ecological = EcologicalImpact::new(2.4, 21600.0, EfficiencyClass::B);
    let metrics = CodeMetrics::new(27)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_ecological_zero_co2() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(0.0, 0, 0.0, "low");
    let ecological = EcologicalImpact::new(0.0, 0.0, EfficiencyClass::A);
    let metrics = CodeMetrics::new(1)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn write_project_report_with_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::new(5)
                .with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low"))
                .with_ecological_impact(EcologicalImpact::new(1.0, 9000.0, EfficiencyClass::B)),
        ),
        (
            path("b.rs"),
            CodeMetrics::new(3)
                .with_economic_impact(EconomicImpact::new(20.0, 200, 21.0, "high"))
                .with_ecological_impact(EcologicalImpact::new(2.0, 18000.0, EfficiencyClass::D)),
        ),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

#[test]
fn write_project_report_without_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.rs"), CodeMetrics::new(5)),
        (path("b.rs"), CodeMetrics::new(3)),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

// MED-1 (#34 T2 review sweep, Security MEDIUM, ADR-0010) — never skipped,
// even when 0 (see console_report_writer.rs's own comment on the same
// convention `unmeasurable_files_count`/`default_excluded_files_count`
// already follow in the JSON writer).
#[test]
fn write_project_report_shows_default_excluded_count() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("a.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_default_excluded_count(4);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Fichiers exclus par défaut: 4"),
        "expected the exact count in the summary, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_zero_default_excluded_count_by_default() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("a.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Fichiers exclus par défaut: 0"),
        "zero must still be printed (never skipped), got: {}",
        output
    );
}

#[test]
fn write_console_with_io_in_loops() {
    let writer = ConsoleReportWriter::new();
    let warnings = vec![
        IoInLoopWarning {
            function: "read_file".to_string(),
            io_call: "std::fs::read".to_string(),
            location: CodeLocation::new("".into(), 5, 9),
        },
        IoInLoopWarning {
            function: "write_data".to_string(),
            io_call: "std::fs::write".to_string(),
            location: CodeLocation::new("".into(), 10, 5),
        },
    ];
    let metrics = CodeMetrics::new(5).with_io_in_loops(warnings);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

// #56 T2 — abstention (ADR-0010/ADR-0014 §4): a synthesis line, never
// per-line detail — abstention must not become a pseudo-warning.
#[test]
fn write_console_shows_unclassifiable_io_in_loops_count() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5).with_unclassifiable_io_in_loops_count(2);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Appels en boucle non classifiables: 2"),
        "expected the unclassifiable synthesis line, got: {}",
        output
    );
}

// T3 (US16, #33): a language whose io_in_loops capability is Unsupported
// (C#, Q1 human-approved) must render an honest "n/a", never a `0` that
// reads as "measured, nothing found" — the DISCRIMINATING assertion is the
// second one: a naive implementation that just always prints
// `unclassifiable_io_in_loops_count()` (0 by construction here, since
// CodeMetrics::new never populates it) would pass the first assertion by
// accident too, so the negative assertion is what actually pins the fix.
#[test]
fn write_console_shows_na_for_unsupported_io_capability_never_a_zero_count() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp)
        .with_io_in_loops(MetricSupport::Unsupported);
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("n/a — non supporté pour C#"),
        "expected the honest n/a line for an unsupported I/O capability, got: {}",
        output
    );
    assert!(
        !output.contains("Appels en boucle non classifiables: 0"),
        "an unsupported io_in_loops capability must never render as a measured 0, got: {}",
        output
    );
}

// QA retry #1 (#33 T3): the summary line's n/a text alone does not
// discriminate the DETAIL-section branch (`if let Some(language) =
// io_na_language { ... "=== I/O dans boucles ===" ... }`) — QA proved by
// mutation that deleting that whole branch (falling back to plain `if
// !io_in_loops.is_empty()`) left every prior test green, because the
// summary line above already contains the same "n/a — non supporté pour
// C#" substring `write_console_shows_na_for_unsupported_io_capability_
// never_a_zero_count` was matching against. This test pins the detail
// section itself, independent of the summary line.
#[test]
fn write_console_shows_na_in_the_io_detail_section_not_just_the_summary_line() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp)
        .with_io_in_loops(MetricSupport::Unsupported);
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("=== I/O dans boucles ===\nn/a — non supporté pour C#"),
        "expected the honest n/a DETAIL section (not just the summary line), got: {}",
        output
    );
}

#[test]
fn write_console_supported_io_capability_still_shows_the_real_count() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp);
    let metrics = CodeMetrics::new(5)
        .with_unclassifiable_io_in_loops_count(4)
        .with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Appels en boucle non classifiables: 4"),
        "a Supported io_in_loops capability must still show the real measured count, got: {}",
        output
    );
}

#[test]
fn write_console_appends_degraded_note_to_transitive_complexity_line() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp).with_call_graph(
        MetricSupport::Degraded("name-based resolution; ambiguous edges dropped".to_string()),
    );
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains(
            "Complexité transitive: 5 (dont 0 cachée dans les appels) [dégradé: name-based resolution; ambiguous edges dropped]"
        ),
        "expected the degraded note appended to the transitive-complexity line, got: {}",
        output
    );
}

// US16 T4.2 (#33) — coordination fix flagged by Dev-B on T3: once
// io_in_loops can be Degraded (not just Unsupported), the console line must
// carry the same "[dégradé: <reason>]" append the transitive-complexity
// line already has for a Degraded call_graph.
#[test]
fn write_console_appends_degraded_note_to_io_in_loops_line() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp).with_io_in_loops(
        MetricSupport::Degraded(
            "syntactic only; instance/EF receivers abstained, not asserted".to_string(),
        ),
    );
    let metrics = CodeMetrics::new(5)
        .with_unclassifiable_io_in_loops_count(3)
        .with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains(
            "Appels en boucle non classifiables: 3 [dégradé: syntactic only; instance/EF receivers abstained, not asserted]"
        ),
        "expected the degraded note appended to the unclassifiable-io line, got: {}",
        output
    );
}

#[test]
fn write_console_rust_output_unchanged_when_capabilities_all_supported() {
    // Zero behavior change for Rust (all-Supported, or no capabilities
    // attached at all): the transitive line carries no note, and the
    // unclassifiable line still shows its real (possibly-zero) count.
    let writer = ConsoleReportWriter::new();
    let metrics_no_capabilities = CodeMetrics::new(5);
    let metrics_all_supported =
        CodeMetrics::new(5).with_capabilities(LanguageCapabilities::all_supported(Language::Rust));

    for metrics in [metrics_no_capabilities, metrics_all_supported] {
        let mut buf = Vec::new();
        writer.write_console_to(&mut buf, &metrics);
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("Complexité transitive: 5 (dont 0 cachée dans les appels)\n"),
            "expected no degraded note on the transitive line, got: {}",
            output
        );
        assert!(
            output.contains("Appels en boucle non classifiables: 0"),
            "expected the real (zero) count, not n/a, got: {}",
            output
        );
        assert!(!output.contains("n/a — non supporté"));
    }
}

#[test]
fn write_console_shows_pattern_name() {
    let writer = ConsoleReportWriter::new();
    let warning = ComplexityWarning {
        pattern: WarningPattern::QuadraticLoop,
        severity: WarningSeverity::Critical,
        function: "process_data".to_string(),
        location: CodeLocation::new("src/lib.rs".into(), 42, 1),
        message: "boucle quadratique détectée".to_string(),
        suggestion: "utiliser un HashMap".to_string(),
    };
    let metrics = CodeMetrics::new(5).with_warnings(vec![warning]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("[CRITICAL][QuadraticLoop]"),
        "expected [CRITICAL][QuadraticLoop] in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_per_file_warnings() {
    let writer = ConsoleReportWriter::new();
    let warning = ComplexityWarning {
        pattern: WarningPattern::NestedLoops,
        severity: WarningSeverity::Warning,
        function: "search".to_string(),
        location: CodeLocation::new("src/search.rs".into(), 15, 1),
        message: "boucles imbriquées".to_string(),
        suggestion: "extraire la logique".to_string(),
    };
    let metrics = CodeMetrics::new(5).with_warnings(vec![warning]);
    let files = vec![(path("src/search.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("NestedLoops"),
        "expected NestedLoops in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_per_file_io_in_loops() {
    let writer = ConsoleReportWriter::new();
    let io_warning = IoInLoopWarning {
        function: "read_file".to_string(),
        io_call: "std::fs::read".to_string(),
        location: CodeLocation::new("src/reader.rs".into(), 10, 5),
    };
    let metrics = CodeMetrics::new(5).with_io_in_loops(vec![io_warning]);
    let files = vec![(path("src/reader.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("I/O dans boucle"),
        "expected I/O warning in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_unclassifiable_io_in_loops_total() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::new(5).with_unclassifiable_io_in_loops_count(2),
        ),
        (
            path("b.rs"),
            CodeMetrics::new(3).with_unclassifiable_io_in_loops_count(1),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Appels en boucle non classifiables (total): 3"),
        "expected the aggregate unclassifiable synthesis line, got: {}",
        output
    );
}

// #132 T3 (AD-6) — the dependency edge count is never displayed without
// saying what the graph cannot see. Models the per-file degraded-note
// format at :81 (write_console_appends_degraded_note_to_transitive_
// complexity_line), applied to the project summary.
// @scenario: dependency-graph-integrity/S2
#[test]
fn write_project_report_appends_degraded_note_to_dependances_totales_line() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::TypeScript)
        .with_cross_file_dependencies(MetricSupport::Degraded(
            "literal relative specifiers only".to_string(),
        ));
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let files = vec![(path("a.ts"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains(
            "Dépendances totales: 0 [dégradé: partial: 0/1 files measured this metric; \
             literal relative specifiers only]"
        ),
        "expected the degraded note appended to the Dépendances totales line, got: {}",
        output
    );
}

// Negative arm (S2's third And): a project whose language resolves every
// dependency carries no such statement — discriminates against a writer
// that appends the note unconditionally.
#[test]
fn write_project_report_no_note_on_dependances_totales_line_when_fully_resolved() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5)
        .with_capabilities(LanguageCapabilities::all_supported(Language::TypeScript));
    let files = vec![(path("a.rs"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("Dépendances totales: 0\n")
            && !output.contains("Dépendances totales: 0 [dégradé:"),
        "a fully-resolved project must carry no degraded note, got: {}",
        output
    );
}

// #132 T4 (human-approved Q2) — the call-graph caveat goes on Complexité
// cachée totale (the one project-summary number entirely derived from the
// call graph), NOT on Complexité transitive totale (which also includes
// direct complexity, reliable regardless of call-graph resolution).
fn function_detail_with_hidden(hidden: u32) -> FunctionDetail {
    FunctionDetail::new(
        "f".to_string(),
        CodeLocation::new("a.ts".into(), 1, 1),
        5,
        hidden,
        2,
        false,
    )
}

#[test]
fn write_project_report_appends_degraded_note_to_complexite_cachee_totale_line() {
    let writer = ConsoleReportWriter::new();
    let capabilities = LanguageCapabilities::all_supported(Language::TypeScript).with_call_graph(
        MetricSupport::Degraded("name-based resolution; anonymous functions merge".to_string()),
    );
    let metrics =
        CodeMetrics::with_call_graph(5, 8, 2, vec![], vec![function_detail_with_hidden(3)])
            .with_capabilities(capabilities);
    let files = vec![(path("a.ts"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains(
            "Complexité cachée totale: 3 [dégradé: partial: 0/1 files measured this metric; \
             name-based resolution; anonymous functions merge]"
        ),
        "expected the degraded note appended to the Complexité cachée totale line, got: {}",
        output
    );
    assert!(
        !output.contains("Complexité transitive totale: 8 [dégradé:"),
        "the call-graph caveat must not duplicate onto Complexité transitive totale (Q2), got: {}",
        output
    );
}

// Retry 1 (Dev-B F6 / Security F1) — the degraded-note reason is analyzed-
// repo-derived input (a `LanguageCapabilities` string a parser attaches),
// exactly like `function`/`message`/`io_call`/path above, but it reached
// the console through 4 sinks (write_console_to's call_graph + io_in_loops
// notes, write_project_report_to's dependencies + hidden-complexity notes)
// with NO sanitization at all. Security proved a hostile reason containing
// a raw ESC + a Trojan-Source RLO override forges a terminal line. One
// test per writer method is enough: all 4 sinks now share the same
// `degraded_note` helper, so proving it sanitizes on one call site per
// method proves it for its sibling call site too.
#[test]
fn write_console_neutralizes_ansi_escape_and_rlo_in_degraded_call_graph_note() {
    let writer = ConsoleReportWriter::new();
    let hostile_reason = "\x1b[2K\rtout est mesuré\u{202e}evil";
    let capabilities = LanguageCapabilities::all_supported(Language::CSharp)
        .with_call_graph(MetricSupport::Degraded(hostile_reason.to_string()));
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b') && !output.contains('\u{202e}'),
        "a raw ESC/RLO byte reached the console via the degraded call_graph note: {:?}",
        output
    );
}

// Matches Security's exact proof: a hostile cross_file_dependencies reason
// forges a "Dépendances totales: 999 [tout est mesuré]" line unless the
// note is sanitized before reaching the terminal.
#[test]
fn write_project_report_neutralizes_ansi_escape_and_rlo_in_degraded_dependances_note() {
    let writer = ConsoleReportWriter::new();
    let hostile_reason = "\x1b[2K\rtout est mesuré\u{202e}evil";
    let capabilities = LanguageCapabilities::all_supported(Language::TypeScript)
        .with_cross_file_dependencies(MetricSupport::Degraded(hostile_reason.to_string()));
    let metrics = CodeMetrics::new(5).with_capabilities(capabilities);
    let files = vec![(path("a.ts"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b') && !output.contains('\u{202e}'),
        "a raw ESC/RLO byte reached the console via the degraded dependencies note: {:?}",
        output
    );
}

#[test]
fn write_project_report_no_note_on_complexite_cachee_totale_line_when_fully_resolved() {
    let writer = ConsoleReportWriter::new();
    let metrics =
        CodeMetrics::with_call_graph(5, 8, 2, vec![], vec![function_detail_with_hidden(3)])
            .with_capabilities(LanguageCapabilities::all_supported(Language::TypeScript));
    let files = vec![(path("a.rs"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("Complexité cachée totale: 3\n"),
        "a fully-resolved call graph must carry no degraded note, got: {}",
        output
    );
}

// #36 — the central acceptance criterion for the whole ticket: the tool
// must never render `0` for a metric it could not measure. `0` reads as
// "free", which is a lie.
#[test]
fn write_stress_test_shows_na_not_zero_when_unmeasurable() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        1,
        1,
        None,
    );
    let impact = Measurement::Unmeasurable(UnmeasurableReason::NoSampler);
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("Temps CPU: 0 ms") && !output.contains("Mémoire: 0.0 MB"),
        "must never render a bare 0 for an unmeasured metric, got: {}",
        output
    );
    assert!(
        output.contains("Temps CPU: n/a") && output.contains("Mémoire: n/a"),
        "expected n/a for unmeasured metrics, got: {}",
        output
    );
}

#[test]
fn write_stress_test_shows_real_numbers_when_measured() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Available(1200),
        Measurement::Available(8192),
        42,
        50,
        None,
    );
    let impact = Measurement::Available(EconomicImpact::new(33.3, 8192 * 1024, 34.1, "low"));
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(output.contains("Temps CPU: 1200 ms"), "got: {}", output);
    assert!(!output.contains("n/a"), "got: {}", output);
}

// #39 — a 0-test run must render the reason, never a confident cost
// figure. This is the console-writer mirror of
// reactive_analyzer_zero_tests_yields_unmeasurable_no_tests_executed:
// the writer already renders Unmeasurable(reason) as "n/a (reason)" for
// every field (#36 machinery), so it needs zero code changes once the
// hexagon returns NoTestsExecuted — this test proves that.
#[test]
fn write_stress_test_shows_no_tests_executed_instead_of_a_cost() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Available(1200),
        Measurement::Available(8192),
        0,
        0,
        None,
    );
    let impact = Measurement::Unmeasurable(UnmeasurableReason::NoTestsExecuted);
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("aucun test exécuté"),
        "expected the no-tests-executed reason in output, got: {}",
        output
    );
    assert!(
        !output.contains("Coût total: $") && !output.contains("Coût total: 0"),
        "must never render a confident cost figure for a 0-test run, got: {}",
        output
    );
}

#[test]
fn write_project_report_no_warnings_does_not_show_section() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5); // no warnings, no io_in_loops
    let files = vec![(path("src/clean.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        !output.contains("avertissements:"),
        "should not show warnings section when no warnings, got: {}",
        output
    );
    assert!(
        !output.contains("I/O dans boucles:"),
        "should not show I/O section when no io_in_loops, got: {}",
        output
    );
}

// D3 (#50 slice S4), test case 21 — console project report must surface
// unmeasurable files as their own section, with path and reason, not
// silently omit them.
#[test]
fn write_project_report_shows_non_mesures_section_with_path_and_reason() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(vec![UnmeasurableFile {
            path: path("src/bad.rs"),
            reason: UnmeasurableReason::SourceUnparseable,
        }]);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("=== Fichiers NON MESURÉS (1) ==="),
        "expected a NON MESURÉS section header with the count, got: {}",
        output
    );
    assert!(
        output.contains("src/bad.rs"),
        "expected the unmeasurable file's path in the section, got: {}",
        output
    );
    assert!(
        output.contains("code source non analysable"),
        "expected the human-readable reason in the section, got: {}",
        output
    );
}

#[test]
fn write_project_report_no_unmeasurable_files_does_not_show_section() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("NON MESURÉS"),
        "should not show the NON MESURÉS section when there are no unmeasurable files, got: {}",
        output
    );
}

// US8 slice 1 — console surface: a breach must print a human-readable
// warning (AC3), via the ONE shared renderer (AD-3). AC6: no threshold
// evaluated at all (graph.threshold_report() == None) leaves the report
// byte-for-byte unchanged from before US8 — the exact prior behavior.
//
// Test List:
// 1. no threshold_report attached at all -> no warning section (AC6)
// 2. threshold_report attached but no breach -> still no warning section
// 3. threshold_report with a breach -> warning section naming the metric,
//    limit, actual value, and excess (AD-3's "by how much")

// #8 (found while writing the e2e single-file test) — write_console_to (the
// single-file surface) never got the threshold banner, only
// write_project_report_to did. Same shared-renderer wiring, single-file
// twin.

#[test]
fn write_console_breaching_threshold_report_shows_warning_with_the_numbers() {
    let writer = ConsoleReportWriter::new();
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000015), None);
    let metrics = CodeMetrics::new(5).with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("SEUIL") && output.contains("ÉNERGIE"),
        "a breach must print a warning section, got: {}",
        output
    );
}

#[test]
fn write_console_without_a_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "no threshold was ever evaluated, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_without_a_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "no threshold was ever evaluated, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_non_breaching_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let thresholds = AlertThresholds::new(Some(1.0), None).unwrap();
    let report = thresholds.evaluate(Some(0.0000065), None);
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "threshold was evaluated but not breached, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_breaching_threshold_report_shows_warning_with_the_numbers() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000015), None);
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("SEUIL"),
        "a breach must print a warning section, got: {}",
        output
    );
    assert!(
        output.contains("ÉNERGIE"),
        "the warning must name the breached metric, got: {}",
        output
    );
}

// US17 T1 retry (Security MEDIUM, CWE-117/CWE-150, BLOCKING 2) — a JS/TS
// string-literal method name can carry raw ANSI escape sequences (a
// closed character set for Rust/C# identifiers, impossible before the
// tree-sitter TS/JS adapter). Both console print sites (single-file
// `write_console_to` and per-file `write_project_report_to`) must
// neutralize control characters in a function's name before printing it —
// the JSON payload (a separate writer, separately verified safe) must
// keep the real name; this test asserts only on the CONSOLE writer.
//
// Test List:
//   1. write_console_to's "=== Détails par fonction ===" line neutralizes
//      an ANSI-escape-laden function name.
//   2. write_project_report_to's per-file function-detail line does the
//      same (a DIFFERENT print site, its own assertion).

fn function_detail_named(name: &str) -> FunctionDetail {
    FunctionDetail::new(
        name.to_string(),
        CodeLocation::new("a.js".into(), 1, 1),
        0,
        0,
        1,
        false,
    )
}

#[test]
fn write_console_neutralizes_ansi_escape_sequences_in_a_function_name() {
    let writer = ConsoleReportWriter::new();
    let hostile_name = "\x1b[2J\x1b[1;31mCRITICAL: system compromised\x1b[0m";
    let metrics =
        CodeMetrics::new(1).with_function_details(vec![function_detail_named(hostile_name)]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console output — the sanitizer did not run: {:?}",
        output
    );
    assert!(
        output.contains("\\u{1b}[2J"),
        "the escape sequence should still be visible, just neutralized: {:?}",
        output
    );
}

// US17 T1 retry 2 (BLOCKING 1 — Dev-B + Security convergent) — four MORE
// unsanitized sinks were reported alongside the two round-1 fixed:
// ComplexityWarning.function/.message and IoInLoopWarning.function/
// .io_call, printed at both the single-file "Avertissements"/"I/O dans
// boucles" sections AND their project-report twins. `io_call` is an
// INDEPENDENT vector from `function` — a computed member behind a
// legitimate confident prefix (`fs.promises["<ESC>..."]`) still carries a
// hostile payload even with a perfectly benign function name, so each
// field gets its own row rather than being asserted together.

fn hostile_warning(function: &str, message: &str) -> ComplexityWarning {
    ComplexityWarning {
        pattern: WarningPattern::NestedLoops,
        severity: WarningSeverity::Critical,
        function: function.to_string(),
        location: CodeLocation::new("evil.js".into(), 1, 1),
        message: message.to_string(),
        suggestion: "n/a".to_string(),
    }
}

fn hostile_io_warning(function: &str, io_call: &str) -> IoInLoopWarning {
    IoInLoopWarning {
        function: function.to_string(),
        io_call: io_call.to_string(),
        location: CodeLocation::new("evil.js".into(), 1, 1),
    }
}

const ESC_PAYLOAD: &str = "\x1b[2J\x1b[1;31mPWNED\x1b[0m";

#[test]
fn write_console_neutralizes_ansi_escape_in_warning_function_and_message() {
    let writer = ConsoleReportWriter::new();
    let metrics =
        CodeMetrics::new(1).with_warnings(vec![hostile_warning(ESC_PAYLOAD, ESC_PAYLOAD)]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the single-file \"Avertissements\" section: {:?}",
        output
    );
}

#[test]
fn write_console_neutralizes_ansi_escape_in_io_in_loop_function_and_io_call() {
    let writer = ConsoleReportWriter::new();
    let metrics =
        CodeMetrics::new(1).with_io_in_loops(vec![hostile_io_warning(ESC_PAYLOAD, ESC_PAYLOAD)]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the single-file \"I/O dans boucles\" section: {:?}",
        output
    );
}

#[test]
fn write_console_neutralizes_ansi_escape_in_io_call_alone_behind_a_benign_function_name() {
    // Security's independent-vector proof: a computed member expression
    // behind a legitimate confident prefix carries the payload even when
    // the FUNCTION name is entirely benign — `io_call` must be sanitized
    // on its own, not merely "whenever function happens to be hostile
    // too".
    let writer = ConsoleReportWriter::new();
    let hostile_io_call = format!("fs.promises[\"{}\"]", ESC_PAYLOAD);
    let metrics =
        CodeMetrics::new(1).with_io_in_loops(vec![hostile_io_warning("f", &hostile_io_call)]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via io_call alone: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_warning_function_and_message() {
    let writer = ConsoleReportWriter::new();
    let metrics =
        CodeMetrics::new(1).with_warnings(vec![hostile_warning(ESC_PAYLOAD, ESC_PAYLOAD)]);
    let files = vec![(path("evil.js"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the project \"avertissements\" section: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_io_in_loop_function_and_io_call() {
    let writer = ConsoleReportWriter::new();
    let metrics =
        CodeMetrics::new(1).with_io_in_loops(vec![hostile_io_warning(ESC_PAYLOAD, ESC_PAYLOAD)]);
    let files = vec![(path("evil.js"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the project \"I/O dans boucles\" section: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_sequences_in_a_function_name() {
    let writer = ConsoleReportWriter::new();
    let hostile_name = "\x1b[2J\x1b[1;31mCRITICAL: system compromised\x1b[0m";
    let metrics =
        CodeMetrics::new(1).with_function_details(vec![function_detail_named(hostile_name)]);
    let files = vec![(path("src/evil.js"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the project console output — the sanitizer did not run: {:?}",
        output
    );
    assert!(
        output.contains("\\u{1b}[2J"),
        "the escape sequence should still be visible, just neutralized: {:?}",
        output
    );
}

// Sweep, item 4 (Dev-B + Security, folded in per the operator's rule —
// same shape, same file, sanitizer already exists) — FS PATHS are
// analyzed-repo-derived input exactly like a method name: on Unix a
// filename may contain any byte except `/` and NUL, so `0x1b` is
// reachable through a hostile file NAME, not just a hostile SYMBOL name.
// `path.display()`, `CodeLocation`'s embedded `file_path`, and the
// `file_stem`-derived consumption-chain labels are all path-derived
// console strings that need the same treatment as `function`/`message`/
// `io_call` did in the earlier sweep.

const HOSTILE_PATH_ESC: &str = "\x1b[2J\x1b[1;31mPWNED\x1b[0m.js";

#[test]
fn write_project_report_neutralizes_ansi_escape_in_the_per_file_path_header() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path(HOSTILE_PATH_ESC), CodeMetrics::new(1))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the project per-file path header: {:?}",
        output
    );
}

#[test]
fn write_console_neutralizes_ansi_escape_in_the_function_detail_location_path() {
    let writer = ConsoleReportWriter::new();
    let detail = FunctionDetail::new(
        "f".to_string(),
        CodeLocation::new(HOSTILE_PATH_ESC.into(), 1, 1),
        0,
        0,
        1,
        false,
    );
    let metrics = CodeMetrics::new(1).with_function_details(vec![detail]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via a function detail's CodeLocation path: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_a_warning_location_path() {
    let writer = ConsoleReportWriter::new();
    let warning = hostile_warning("f", "boucles imbriquées détectées");
    let metrics = CodeMetrics::new(1).with_warnings(vec![ComplexityWarning {
        location: CodeLocation::new(HOSTILE_PATH_ESC.into(), 1, 1),
        ..warning
    }]);
    let files = vec![(path("clean.js"), metrics)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via a warning's CodeLocation path: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_the_consumption_chain_file_stem() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.js"), CodeMetrics::new(1)),
        (path(HOSTILE_PATH_ESC), CodeMetrics::new(1)),
    ];
    let deps = vec![FileDependency {
        from: path("a.js"),
        to: path(HOSTILE_PATH_ESC),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via a consumption-chain file_stem: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_a_cycle_path() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.js"), CodeMetrics::new(1)),
        (path(HOSTILE_PATH_ESC), CodeMetrics::new(1)),
    ];
    let deps = vec![
        FileDependency {
            from: path("a.js"),
            to: path(HOSTILE_PATH_ESC),
        },
        FileDependency {
            from: path(HOSTILE_PATH_ESC),
            to: path("a.js"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via a cycle's path: {:?}",
        output
    );
}

#[test]
fn write_project_report_neutralizes_ansi_escape_in_an_unmeasurable_file_path() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("clean.js"), CodeMetrics::new(1))];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(vec![UnmeasurableFile {
            path: path(HOSTILE_PATH_ESC),
            reason: UnmeasurableReason::SourceUnparseable,
        }]);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains('\x1b'),
        "a raw ESC byte reached the console via an unmeasurable file's path: {:?}",
        output
    );
}

// The pre-existing (Security INFO, confirmed unrelated to this diff)
// `file_stem().unwrap().to_str().unwrap()` panic risk on the SAME line
// being sanitized: a path ending in `..` has no `file_stem()`. Fixed in
// the same pass rather than leaving a reachable panic behind a fix.
#[test]
fn write_project_report_does_not_panic_on_a_consumption_chain_path_with_no_file_stem() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.js"), CodeMetrics::new(1)),
        (path("dir/.."), CodeMetrics::new(1)),
    ];
    let deps = vec![FileDependency {
        from: path("a.js"),
        to: path("dir/.."),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    // Must not panic.
    writer.write_project_report_to(&mut buf, &graph);
}
