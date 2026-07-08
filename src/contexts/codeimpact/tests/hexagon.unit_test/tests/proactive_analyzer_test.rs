use codeimpact_hexagon::analysis::proactive_analyzer;
use codeimpact_hexagon::analysis::AnalysisRule;

#[test]
fn empty_source_returns_complexity_1() {
    let metrics = proactive_analyzer::analyze("", &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn simple_function_no_branching_returns_complexity_1() {
    let source = "fn hello() { let x = 1; }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn one_if_statement_returns_complexity_2() {
    let source = "fn test() { if x > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_returns_complexity_2() {
    let source = "fn test() { if x > 0 { } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_if_returns_complexity_3() {
    let source = "fn test() { if x > 0 { } else if x < 0 { } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn while_loop_returns_complexity_2() {
    let source = "fn test() { while x > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn for_loop_returns_complexity_2() {
    let source = "fn test() { for i in 0..10 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn match_keyword_adds_1() {
    let source = "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn nested_if_else_returns_complexity_3() {
    let source = "fn test() { if x > 0 { if y > 0 { } } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn multiple_functions_sum_their_complexities() {
    let source = "fn a() { if x > 0 { } }\nfn b() { while true { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn and_operator_counts_as_branch() {
    let source = "fn test() { if x > 0 && y > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn or_operator_counts_as_branch() {
    let source = "fn test() { if x > 0 || y > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn catch_keyword_counts_as_branch() {
    let source = "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn combination_of_all_constructs() {
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
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 7);
}
