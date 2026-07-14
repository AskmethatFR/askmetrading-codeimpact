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
    let source =
        "fn test() {\n    for _ in 0..10 {\n        std::fs::read_to_string(\"f\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    let call = &functions[0].calls_in_loops[0];
    assert_eq!(call.name, "std::fs::read_to_string");
    assert_eq!(call.line, 3);
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
    assert_eq!(functions[0].calls_in_loops[0].name, "std::fs::read");
    assert_eq!(
        functions[0].calls_in_loops[1].name,
        "std::net::TcpStream::connect"
    );
}

#[test]
fn tokio_fs_call_in_loop_tracked() {
    let parser = SynCodeParser::new();
    let source =
        "fn test() {\n    for _ in 0..10 {\n        tokio::fs::read_to_string(\"f\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    assert_eq!(
        functions[0].calls_in_loops[0].name,
        "tokio::fs::read_to_string"
    );
}

#[test]
fn reqwest_call_in_loop_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        reqwest::get(\"url\");\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    assert_eq!(functions[0].calls_in_loops[0].name, "reqwest::get");
}

// #47 retry 2 — calls_in_loops must record EVERY call nested in a loop as a
// fact (is_io classifies, it does not filter), not just I/O calls. Two gaps
// in the prior behavior:
//   (a) a plain, non-I/O `Expr::Call` was silently dropped (gated on
//       is_io_call before being pushed at all) — this is the actual root
//       cause of #47: QuadraticLoop reads calls_in_loops and could never see
//       a nested call to a plain, non-I/O helper function.
//   (b) `Expr::MethodCall` never touched calls_in_loops at all, regardless
//       of loop nesting or is_io — most intra-type calls in Rust are method
//       calls.
// Test List:
// 1. non_io_plain_call_in_loop_is_tracked_with_is_io_false — (a) above
// 2. method_call_in_loop_is_tracked_with_is_io_false — (b) above
// 3. non_io_call_after_loop_is_not_tracked — sequential (non-nested) call
//    stays absent regardless of is_io (regression pin, already true today)

#[test]
fn non_io_plain_call_in_loop_is_tracked_with_is_io_false() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 {\n        validate();\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    let call = &functions[0].calls_in_loops[0];
    assert_eq!(call.name, "validate");
    assert!(!call.is_io);
}

#[test]
fn method_call_in_loop_is_tracked_with_is_io_false() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for x in &xs {\n        x.helper();\n    }\n}\n";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions[0].calls_in_loops.len(), 1);
    let call = &functions[0].calls_in_loops[0];
    assert_eq!(call.name, "helper");
    assert!(!call.is_io);
}

#[test]
fn non_io_call_after_loop_is_not_tracked() {
    let parser = SynCodeParser::new();
    let source = "fn test() {\n    for _ in 0..10 { }\n    validate();\n}\n";
    let functions = parser.parse(source).unwrap();
    assert!(
        functions[0].calls_in_loops.is_empty(),
        "a sequential call after the loop must not be recorded"
    );
}

// #50 — Type::method qualification (D1) + self/Self call resolution (D2).
// Test List (S1 — declarations):
// 1. impl_method_is_qualified_by_type_name
// 2. impl_trait_for_type_uses_type_name_not_trait_name
// 3. impl_with_generics_erases_generic_params
// 4. trait_default_method_is_qualified_by_trait_name_abstract_method_excluded
// 5. inline_mod_free_fn_is_qualified_by_mod_path
// 6. inline_mod_impl_method_is_qualified_by_mod_and_type_path
// 7. duplicate_qualified_names_are_suffixed_not_clobbered
// 8. NON-REGRESSION free_fn_names_stay_bare
// 9. NON-REGRESSION nested_fn_stays_folded_into_parent
// Test List (S2 — call-graph edges):
// 10. self_method_call_resolves_to_qualified_callee
// 11. self_colon_colon_method_call_resolves_to_qualified_callee
// 12. mutual_self_recursion_is_detected_as_a_cycle
// 13. NO FABRICATION non_self_receiver_method_call_stays_bare
// 14. quadratic_loop_detected_through_resolved_self_call

#[test]
fn impl_method_is_qualified_by_type_name() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { fn foo(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "S::foo");
    assert_eq!(functions[0].decision_points, 1);
}

#[test]
fn impl_trait_for_type_uses_type_name_not_trait_name() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl Display for S { fn fmt(&self) { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "S::fmt");
}

#[test]
fn impl_with_generics_erases_generic_params() {
    let parser = SynCodeParser::new();
    let source = "struct Wrapper<T>(T); impl<T> Wrapper<T> { fn get(&self) { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "Wrapper::get");
}

#[test]
fn trait_default_method_is_qualified_by_trait_name_abstract_method_excluded() {
    let parser = SynCodeParser::new();
    let source = "trait Tr { fn hook(&self) { if x { } } fn abstract_(&self); }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "Tr::hook");
}

#[test]
fn inline_mod_free_fn_is_qualified_by_mod_path() {
    let parser = SynCodeParser::new();
    let source = "mod inner { fn helper() { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "inner::helper");
}
