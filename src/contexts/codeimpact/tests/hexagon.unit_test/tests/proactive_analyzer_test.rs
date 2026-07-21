use codeimpact_hexagon::analysis::proactive_analyzer;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::IoClassification;
use codeimpact_hexagon::analysis::LoopCall;
use codeimpact_hexagon::analysis::MetricSupport;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;
use codeimpact_secondaries::gateways::code_parsers::tree_sitter::tree_sitter_code_parser::TreeSitterCodeParser;

fn make_parser(decision_points: u32) -> CodeParserStub {
    CodeParserStub::with_functions(vec![ParsedFunction {
        name: "test".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
}

fn make_multi_parser(points: &[u32]) -> CodeParserStub {
    let functions: Vec<ParsedFunction> = points
        .iter()
        .enumerate()
        .map(|(i, &dp)| ParsedFunction {
            name: format!("fn_{}", i),
            start_line: 1,
            calls: vec![],
            has_loop: false,
            has_nested_loop: false,
            decision_points: dp,
            depth: 0,
            branch_arms: 0,
            calls_in_loops: vec![],
        })
        .collect();
    CodeParserStub::with_functions(functions)
}

#[test]
fn empty_source_returns_complexity_1() {
    let parser = CodeParserStub::with_functions(vec![]);
    let metrics = proactive_analyzer::analyze("", &[AnalysisRule::CyclomaticComplexity], &parser)
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn simple_function_no_branching_returns_complexity_1() {
    let parser = make_parser(0);
    let metrics = proactive_analyzer::analyze(
        "fn hello() {}",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn one_if_statement_returns_complexity_2() {
    let parser = make_parser(1);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_returns_complexity_2() {
    let parser = make_parser(1);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { } else { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_if_returns_complexity_3() {
    let parser = make_parser(2);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { } else if x < 0 { } else { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn while_loop_returns_complexity_2() {
    let parser = make_parser(1);
    let metrics = proactive_analyzer::analyze(
        "fn test() { while x > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn for_loop_returns_complexity_2() {
    let parser = make_parser(1);
    let metrics = proactive_analyzer::analyze(
        "fn test() { for i in 0..10 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn match_keyword_adds_1() {
    // With AST parsing, each match arm counts as a decision point
    let parser = make_parser(3);
    let metrics = proactive_analyzer::analyze(
        "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 4);
}

#[test]
fn nested_if_else_returns_complexity_3() {
    let parser = make_parser(2);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { if y > 0 { } } else { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn multiple_functions_sum_their_complexities() {
    let parser = make_multi_parser(&[1, 1]);
    let metrics = proactive_analyzer::analyze(
        "fn a() { if x > 0 { } }\nfn b() { while true { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn and_operator_counts_as_branch() {
    let parser = make_parser(2);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 && y > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn or_operator_counts_as_branch() {
    let parser = make_parser(2);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 || y > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn catch_keyword_no_longer_false_positive() {
    // catch is a method call, not a Rust keyword — no longer counted as decision point
    let parser = make_parser(0);
    let metrics = proactive_analyzer::analyze(
        "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn parser_error_propagates() {
    let parser = CodeParserStub::new(Err(
        codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed("parse error".to_string()),
    ));
    let result = proactive_analyzer::analyze(
        "invalid code",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    );
    assert!(result.is_err());
}

#[test]
fn combination_of_all_constructs() {
    // With AST: 1 if + 1 else if + 1 for + 1 inner if + 1 while + 2 match arms = 7
    let parser = make_parser(7);
    let source = r#"
fn complex(x: i32) {
    if x > 0 {
        for i in 0..x {
            if i % 2 == 0 {
                println!("even");
            }
        }
    } else if x < 0 {
        while x < 0 {
            println!("negative");
        }
    } else {
        match x {
            0 => println!("zero"),
            _ => {}
        }
    }
}
"#;
    let metrics =
        proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity], &parser)
            .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 8);
}

#[test]
fn analysis_includes_economic_impact() {
    let parser = make_parser(3);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { } else if y > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    let impact = metrics.economic_impact();
    assert!(impact.is_some(), "economic impact should be computed");
    let impact = impact.unwrap();
    assert!(impact.cpu_cost_microdollars() > 0.0);
    assert!(impact.memory_bytes() > 0);
    assert!(impact.total_cost_microdollars() > 0.0);
    assert_eq!(impact.level(), "low");
}

#[test]
fn economic_impact_near_zero_for_trivial_code() {
    let parser = CodeParserStub::with_functions(vec![]);
    let metrics = proactive_analyzer::analyze("", &[AnalysisRule::CyclomaticComplexity], &parser)
        .expect("analysis should succeed");
    let impact = metrics.economic_impact();
    assert!(impact.is_some(), "economic impact should be computed");
    let impact = impact.unwrap();
    assert!(
        impact.total_cost_microdollars() < 1.0,
        "trivial code should have near-zero cost"
    );
}

#[test]
fn analysis_includes_ecological_impact() {
    let parser = make_parser(3);
    let metrics = proactive_analyzer::analyze(
        "fn test() { if x > 0 { } else if y > 0 { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    let impact = metrics.ecological_impact();
    assert!(impact.is_some(), "ecological impact should be computed");
    let impact = impact.unwrap();
    assert!(impact.co2_grams() > 0.0);
    assert!(impact.energy_joules() > 0.0);
    assert_eq!(impact.efficiency_class().label(), "A");
}

#[test]
fn io_in_loops_rule_detects_io_in_loops() {
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "read_file".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![LoopCall {
            name: "std::fs::read".to_string(),
            line: 5,
            col: 9,
            io: IoClassification::Io,
        }],
    }]);
    let metrics = proactive_analyzer::analyze(
        "fn read_file() { for _ in 0..10 { std::fs::read(\"file\"); } }",
        &[AnalysisRule::CyclomaticComplexity, AnalysisRule::IoInLoops],
        &parser,
    )
    .expect("analysis should succeed");
    let io = metrics.io_in_loops();
    assert_eq!(io.len(), 1);
    assert_eq!(io[0].function, "read_file");
    assert_eq!(io[0].io_call, "std::fs::read");
    assert_eq!(io[0].location.to_string(), ":5:9");
}

#[test]
fn io_in_loops_rule_not_in_rules_returns_empty() {
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "read_file".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![LoopCall {
            name: "std::fs::read".to_string(),
            line: 5,
            col: 9,
            io: IoClassification::Io,
        }],
    }]);
    let metrics = proactive_analyzer::analyze(
        "fn read_file() { for _ in 0..10 { std::fs::read(\"file\"); } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    let io = metrics.io_in_loops();
    assert!(io.is_empty(), "IoInLoops not in rules should return empty");
}

#[test]
fn io_in_loops_rule_counts_unclassifiable_calls() {
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "process".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![
            LoopCall {
                name: "std::fs::read".to_string(),
                line: 5,
                col: 9,
                io: IoClassification::Io,
            },
            LoopCall {
                name: "connect".to_string(),
                line: 6,
                col: 9,
                io: IoClassification::Unknown,
            },
        ],
    }]);
    let metrics = proactive_analyzer::analyze(
        "fn process() { for _ in 0..10 { std::fs::read(\"file\"); conn.connect(); } }",
        &[AnalysisRule::CyclomaticComplexity, AnalysisRule::IoInLoops],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(
        metrics.unclassifiable_io_in_loops_count(),
        1,
        "exactly the Unknown call should be counted, the Io call must not contribute"
    );
    assert_eq!(
        metrics.io_in_loops().len(),
        1,
        "the Unknown call must never surface as a per-line warning"
    );
}

// T3 (US16, #33): analyze() must attach the parser's own declared
// capabilities to the metrics it builds — this is the calling use case for
// CodeMetrics::with_capabilities (a real C# adapter, not a stub, so the
// wiring is proven end-to-end from a parser that actually degrades a
// metric).
#[test]
fn analyze_through_csharp_parser_attaches_its_declared_capabilities() {
    let parser = TreeSitterCodeParser::csharp();
    let metrics = proactive_analyzer::analyze(
        "class C { void M() { } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");

    let capabilities = metrics
        .capabilities()
        .expect("analyze() should attach the parser's capabilities");
    assert_eq!(*capabilities.io_in_loops(), MetricSupport::Unsupported);
}

#[test]
fn io_in_loops_rule_not_in_rules_returns_zero_unclassifiable() {
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "process".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![LoopCall {
            name: "connect".to_string(),
            line: 6,
            col: 9,
            io: IoClassification::Unknown,
        }],
    }]);
    let metrics = proactive_analyzer::analyze(
        "fn process() { for _ in 0..10 { conn.connect(); } }",
        &[AnalysisRule::CyclomaticComplexity],
        &parser,
    )
    .expect("analysis should succeed");
    assert_eq!(
        metrics.unclassifiable_io_in_loops_count(),
        0,
        "IoInLoops not in rules should not compute the unclassifiable count either"
    );
}
