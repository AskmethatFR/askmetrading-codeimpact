use codeimpact_hexagon::analysis::proactive_analyzer;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;

fn make_parser(decision_points: u32) -> CodeParserStub {
    CodeParserStub::with_functions(vec![ParsedFunction {
        name: "test".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points,
        depth: 0,
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
