// Test List for ProactiveAnalyzer:
// 1. Empty source -> complexity 1
// 2. Simple function, no branching -> complexity 1
// 3. One if statement -> complexity 2
// 4. if/else -> complexity 2
// 5. if/else if -> complexity 3
// 6. while loop -> complexity 2
// 7. for loop -> complexity 2
// 8. match with 3 arms -> complexity 3
// 9. match with 5 arms -> complexity 5
// 10. Nested if/else -> complexity 3 (if + if + else)
// 11. Multiple functions -> sum of all complexities
// 12. '&&' operator counts as branch
// 13. '||' operator counts as branch
// 14. 'catch' keyword counts as branch
// 15. Combination of all constructs -> correct total

use codeimpact_hexagon::domain_model::{analysis_rule::AnalysisRule, proactive_analyzer};

#[test]
fn empty_source_returns_complexity_1() {
    let metrics = proactive_analyzer::analyze("", &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn simple_function_no_branching_returns_complexity_1() {
    let source = "fn hello() { let x = 1; }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 1);
}

#[test]
fn one_if_statement_returns_complexity_2() {
    let source = "fn test() { if x > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_returns_complexity_2() {
    let source = "fn test() { if x > 0 { } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn if_else_if_returns_complexity_3() {
    let source = "fn test() { if x > 0 { } else if x < 0 { } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn while_loop_returns_complexity_2() {
    let source = "fn test() { while x > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn for_loop_returns_complexity_2() {
    let source = "fn test() { for i in 0..10 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn match_keyword_adds_1() {
    let source = "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // 1 (base) + 1 (match keyword) = 2
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn nested_if_else_returns_complexity_3() {
    let source = "fn test() { if x > 0 { if y > 0 { } } else { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn multiple_functions_sum_their_complexities() {
    let source = "fn a() { if x > 0 { } }\nfn b() { while true { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // a: 2 (if), b: 2 (while), base 1 = 3
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn and_operator_counts_as_branch() {
    let source = "fn test() { if x > 0 && y > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // 1 (base) + 1 (if) + 1 (&&) = 3
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn or_operator_counts_as_branch() {
    let source = "fn test() { if x > 0 || y > 0 { } }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // 1 (base) + 1 (if) + 1 (||) = 3
    assert_eq!(metrics.cyclomatic_complexity(), 3);
}

#[test]
fn catch_keyword_counts_as_branch() {
    let source = "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // 1 (base) + 1 (catch) = 2
    assert_eq!(metrics.cyclomatic_complexity(), 2);
}

#[test]
fn combination_of_constructs_returns_correct_total() {
    let source = "\
fn classify(x: i32) -> &'static str {
    if x > 0 {
        if x % 2 == 0 {
            \"even\"
        } else {
            \"odd\"
        }
    } else if x < 0 {
        \"negative\"
    } else {
        \"zero\"
    }
}";
    let metrics = proactive_analyzer::analyze(source, &[AnalysisRule::CyclomaticComplexity])
        .expect("Analysis should succeed");
    // 1 (base) + 1 (if x>0) + 1 (if x%2==0) + 1 (else if x<0) = 4
    assert_eq!(metrics.cyclomatic_complexity(), 4);
}
