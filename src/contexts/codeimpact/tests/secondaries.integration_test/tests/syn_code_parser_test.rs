use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;

#[test]
fn empty_source_returns_no_functions() {
    let parser = SynCodeParser::new();
    let functions = parser.parse("").unwrap();
    assert!(functions.is_empty());
}

#[test]
fn no_branching_returns_no_decision_points() {
    let parser = SynCodeParser::new();
    let source = "fn hello() { let x = 1; }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "hello");
    assert_eq!(functions[0].decision_points, 0);
}

#[test]
fn one_if_statement_counts_one_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].decision_points, 1);
}

#[test]
fn if_else_counts_one_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 { } else { } }";
    let functions = parser.parse(source).unwrap();
    // if + else = 1 decision point (else is not a branch, just the alternative)
    assert_eq!(functions[0].decision_points, 1);
}

#[test]
fn if_else_if_counts_two_decision_points() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 { } else if x < 0 { } else { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].decision_points, 2);
}

#[test]
fn while_loop_counts_one_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { while x > 0 { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].decision_points, 1);
    assert!(functions[0].has_loop);
}

#[test]
fn for_loop_counts_one_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { for i in 0..10 { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].decision_points, 1);
    assert!(functions[0].has_loop);
}

#[test]
fn match_arms_count_per_arm() {
    let parser = SynCodeParser::new();
    let source = "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }";
    let functions = parser.parse(source).unwrap();
    // 3 match arms = 3 decision points
    assert_eq!(functions[0].decision_points, 3);
}

#[test]
fn and_operator_counts_as_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 && y > 0 { } }";
    let functions = parser.parse(source).unwrap();
    // 1 if + 1 && = 2
    assert_eq!(functions[0].decision_points, 2);
}

#[test]
fn or_operator_counts_as_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 || y > 0 { } }";
    let functions = parser.parse(source).unwrap();
    // 1 if + 1 || = 2
    assert_eq!(functions[0].decision_points, 2);
}

#[test]
fn catch_method_call_not_counted() {
    let parser = SynCodeParser::new();
    let source = "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }";
    let functions = parser.parse(source).unwrap();
    // catch is a method call, not a keyword — no decision point
    assert_eq!(functions[0].decision_points, 0);
}

#[test]
fn string_and_operator_not_counted() {
    let parser = SynCodeParser::new();
    let source = "fn test() { let s = \"a && b\"; }";
    let functions = parser.parse(source).unwrap();
    // && in string literal — not a binary operator
    assert_eq!(functions[0].decision_points, 0);
}

#[test]
fn function_calls_are_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() { foo(); bar::baz(); }";
    let functions = parser.parse(source).unwrap();
    assert!(functions[0].calls.contains(&"foo".to_string()));
    assert!(functions[0].calls.contains(&"bar::baz".to_string()));
}

#[test]
fn method_calls_are_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() { let _ = x.foo().bar(); }";
    let functions = parser.parse(source).unwrap();
    assert!(functions[0].calls.contains(&"foo".to_string()));
    assert!(functions[0].calls.contains(&"bar".to_string()));
}

#[test]
fn nested_loop_detected() {
    let parser = SynCodeParser::new();
    let source = "fn test() { for i in 0..10 { while true { } } }";
    let functions = parser.parse(source).unwrap();
    assert!(functions[0].has_loop);
    assert!(functions[0].has_nested_loop);
}

#[test]
fn nesting_depth_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() { if x > 0 { if y > 0 { if z > 0 { } } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].depth, 3);
}

#[test]
fn multiple_functions_parsed_separately() {
    let parser = SynCodeParser::new();
    let source = "fn a() { if x > 0 { } }\nfn b() { while true { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 2);
    assert_eq!(functions[0].name, "a");
    assert_eq!(functions[0].decision_points, 1);
    assert_eq!(functions[1].name, "b");
    assert_eq!(functions[1].decision_points, 1);
    assert!(functions[1].has_loop);
}

#[test]
fn complex_function_accumulates_all_decision_points() {
    let parser = SynCodeParser::new();
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
    let functions = parser.parse(source).unwrap();
    let f = &functions[0];
    // 1 if + 1 else if + 1 for + 1 inner if + 1 while + 2 match arms = 7
    assert_eq!(f.decision_points, 7);
    assert!(f.has_loop);
    // for and while are at the same nesting level, not inside each other
    assert!(!f.has_nested_loop);
}

#[test]
fn non_rust_syntax_returns_error() {
    let parser = SynCodeParser::new();
    let result = parser.parse("this is not valid rust code @@@");
    assert!(result.is_err());
}

#[test]
fn triple_nested_loops_detected() {
    let parser = SynCodeParser::new();
    let source = "fn test() { for i in 0..10 { for j in 0..10 { for k in 0..10 { } } } }";
    let functions = parser.parse(source).unwrap();
    assert!(functions[0].has_loop);
    assert!(functions[0].has_nested_loop);
    // 3 for loops = 3 decision points
    assert_eq!(functions[0].decision_points, 3);
}

#[test]
fn loop_expression_counts_as_decision_point() {
    let parser = SynCodeParser::new();
    let source = "fn test() { loop { break; } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].decision_points, 1);
    assert!(functions[0].has_loop);
}

#[test]
fn closure_inside_if_does_not_create_false_positive() {
    let parser = SynCodeParser::new();
    let source = "fn test() { let f = |x| if x > 0 { 1 } else { 0 }; }";
    let functions = parser.parse(source).unwrap();
    // 1 if inside closure = 1 decision point
    // The if inside the closure is still parsed — it's Rust code
    assert_eq!(functions[0].decision_points, 1);
}

#[test]
fn io_call_in_loop_is_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        std::fs::read_to_string(\"f\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    let (call_name, line, _col) = &functions[0].calls_in_loops[0];
    assert_eq!(call_name, "std::fs::read_to_string");
    assert_eq!(*line, 3);
}

#[test]
fn non_io_call_in_loop_not_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        println!(\"x\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert!(
        functions[0].calls_in_loops.is_empty(),
        "println! should not be tracked as I/O in loop"
    );
}

#[test]
fn io_call_not_in_loop_not_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    std::fs::read_to_string(\"f\");\n}\n";
    let functions = parser.parse(source).unwrap();
    assert!(
        functions[0].calls_in_loops.is_empty(),
        "I/O call outside loop should not be tracked"
    );
}

#[test]
fn multiple_io_calls_in_loop_all_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        std::fs::read(\"a\");\n        std::net::TcpStream::connect(\"b\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 2);
    assert_eq!(functions[0].calls_in_loops[0].0, "std::fs::read");
    assert_eq!(functions[0].calls_in_loops[1].0, "std::net::TcpStream::connect");
}

#[test]
fn tokio_fs_call_in_loop_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        tokio::fs::read_to_string(\"f\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    assert_eq!(functions[0].calls_in_loops[0].0, "tokio::fs::read_to_string");
}

#[test]
fn reqwest_call_in_loop_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        reqwest::get(\"url\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    assert_eq!(functions[0].calls_in_loops[0].0, "reqwest::get");
}
