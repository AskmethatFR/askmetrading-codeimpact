use codeimpact_hexagon::analysis::CallGraph;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::ComplexityDetector;
use codeimpact_hexagon::analysis::DetectionConfig;
use codeimpact_hexagon::analysis::WarningPattern;
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

#[test]
fn inline_mod_impl_method_is_qualified_by_mod_and_type_path() {
    let parser = SynCodeParser::new();
    let source = "mod inner { struct S; impl S { fn m() { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "inner::S::m");
}

#[test]
fn duplicate_qualified_names_are_suffixed_not_clobbered() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { fn f() { } } impl Default for S { fn f() { } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 2);
    let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["S::f", "S::f#2"]);
}

#[test]
fn free_fn_names_stay_bare() {
    let parser = SynCodeParser::new();
    let source = "fn a() { if x > 0 { } }\nfn b() { }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 2);
    assert_eq!(functions[0].name, "a");
    assert_eq!(functions[0].decision_points, 1);
    assert_eq!(functions[1].name, "b");
    assert_eq!(functions[1].decision_points, 0);
}

#[test]
fn nested_fn_stays_folded_into_parent() {
    let parser = SynCodeParser::new();
    let source = "fn outer() { fn inner() { if x > 0 { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "outer");
    assert_eq!(functions[0].decision_points, 1);
}

#[test]
fn self_method_call_resolves_to_qualified_callee() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { fn a(&self) { self.b(); } fn b(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    let a = functions.iter().find(|f| f.name == "S::a").unwrap();
    assert!(
        a.calls.contains(&"S::b".to_string()),
        "self.b() must resolve to S::b, got {:?}",
        a.calls
    );
    // The test that bites: without the self rule, calls would contain the
    // bare "b", which never matches the declaration "S::b" — hidden_of
    // would stay 0 and every intra-type edge would silently disappear.
    let graph = CallGraph::build(&functions);
    assert_eq!(graph.hidden_of("S::a"), 1);
}

#[test]
fn self_colon_colon_method_call_resolves_to_qualified_callee() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { fn a() { Self::b(); } fn b() { if x { } } }";
    let functions = parser.parse(source).unwrap();
    let a = functions.iter().find(|f| f.name == "S::a").unwrap();
    assert!(
        a.calls.contains(&"S::b".to_string()),
        "Self::b() must resolve to S::b, got {:?}",
        a.calls
    );
}

#[test]
fn mutual_self_recursion_is_detected_as_a_cycle() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { fn a(&self) { self.b(); } fn b(&self) { self.a(); } }";
    let functions = parser.parse(source).unwrap();
    let graph = CallGraph::build(&functions);
    assert!(graph.has_cycle("S::a"), "S::a must be reported in_cycle");
    assert!(graph.has_cycle("S::b"), "S::b must be reported in_cycle");
}

#[test]
fn non_self_receiver_method_call_stays_bare() {
    let parser = SynCodeParser::new();
    let source =
        "struct S; impl S { fn a(&self, v: Vec<u8>) { v.len(); } fn len(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    let a = functions.iter().find(|f| f.name == "S::a").unwrap();
    assert!(
        a.calls.contains(&"len".to_string()),
        "v.len() must stay bare \"len\", got {:?}",
        a.calls
    );
    assert!(
        !a.calls.contains(&"S::len".to_string()),
        "v.len() must NOT be fabricated into S::len, got {:?}",
        a.calls
    );
    let graph = CallGraph::build(&functions);
    assert_eq!(
        graph.hidden_of("S::a"),
        0,
        "no fabricated edge means no hidden complexity from S::len"
    );
}

// D6 (#50, slice S3) — `#[cfg(test)] mod tests { ... }` is Rust's own test
// harness, not production code: leaving it in makes every test function
// enter production metrics (function count, call graph, hidden_complexity),
// inflating cost/CO2 for code that never runs in production (ADR-0013: the
// domain names the concept, the adapter — here, `#[cfg(test)]` — names the
// Rust syntax that expresses it).
// Test List:
// 15. cfg_test_mod_is_excluded_from_parsing — the attribute is honored
// 16. inline_mod_without_cfg_test_attribute_is_still_parsed — the exclusion
//     is scoped to `cfg(test)`, not to inline `mod` in general (S1 already
//     descends into legitimate inline mods; S3 must not throw that away)

#[test]
fn cfg_test_mod_is_excluded_from_parsing() {
    let parser = SynCodeParser::new();
    let source = "fn prod() {} #[cfg(test)] mod tests { fn t1() { if x {} } }";
    let functions = parser.parse(source).unwrap();

    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(functions[0].name, "prod");
    let total_decision_points: u32 = functions.iter().map(|f| f.decision_points).sum();
    assert_eq!(
        total_decision_points, 0,
        "t1's `if x {{}}` must not contribute a decision point — it was never parsed"
    );
    assert!(
        !functions.iter().any(|f| f.name.contains("t1")),
        "t1 must be absent, got {:?}",
        functions
    );
}

#[test]
fn inline_mod_without_cfg_test_attribute_is_still_parsed() {
    let parser = SynCodeParser::new();
    let source = "mod util { fn helper() {} }";
    let functions = parser.parse(source).unwrap();

    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(functions[0].name, "util::helper");
}

#[test]
fn quadratic_loop_detected_through_resolved_self_call() {
    let parser = SynCodeParser::new();
    let source = "struct S; impl S { \
        fn a(&self) { for i in 0..n { self.b(); } } \
        fn b(&self) { for j in 0..n { } } \
    }";
    let functions = parser.parse(source).unwrap();
    let graph = CallGraph::build(&functions);
    let warnings = ComplexityDetector::detect(&functions, &graph, &DetectionConfig::default());
    let quadratic: Vec<_> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::QuadraticLoop) && w.function == "S::a")
        .collect();
    assert_eq!(
        quadratic.len(),
        1,
        "S::a must get exactly one QuadraticLoop warning now that self.b() resolves \
         to the looping S::b, got {:?}",
        warnings
    );
}

// BLOCKER 3 (#50 QA retry 1) — `type_last_segment`'s Reference/Paren/Group
// fallback arms, and the `collect_functions` branch that consumes a `None`
// qualifier, had no test at all. Per the D1 table:
//   - `impl Trait for &Type` / `(Type)` -> last segment of the dereferenced
//     type (the Reference/Paren recursion).
//   - a `self_ty` that is NOT nameable (tuple, array, ...) -> fall back to
//     the trait name (`Trait::foo`); failing that (inherent impl, no
//     trait), the bare name (`foo`).
// Test List:
// 17. impl_trait_for_reference_type_dereferences_to_last_segment
// 18. impl_trait_for_parenthesized_type_dereferences_to_last_segment
// 19. impl_for_generic_collection_type_uses_container_name
// 20. impl_trait_for_non_nameable_type_falls_back_to_trait_name
// 21. inherent_impl_for_non_nameable_type_falls_back_to_bare_name

#[test]
fn impl_trait_for_reference_type_dereferences_to_last_segment() {
    let parser = SynCodeParser::new();
    let source =
        "struct S; trait Fmt { fn fmt(&self); } impl Fmt for &S { fn fmt(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(
        functions[0].name, "S::fmt",
        "impl Trait for &Type must dereference to the pointee's last segment, got {:?}",
        functions
    );
}

#[test]
fn impl_trait_for_parenthesized_type_dereferences_to_last_segment() {
    let parser = SynCodeParser::new();
    let source =
        "struct S; trait Fmt { fn fmt(&self); } impl Fmt for (S) { fn fmt(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(
        functions[0].name, "S::fmt",
        "impl Trait for (Type) must dereference the parenthesized type to its last segment, got {:?}",
        functions
    );
}

#[test]
fn impl_for_generic_collection_type_uses_container_name() {
    let parser = SynCodeParser::new();
    let source = "struct Vec<T> { items: Vec<T> } impl<T> Vec<T> { fn push(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(
        functions[0].name, "Vec::push",
        "impl on a generic container type must qualify by the container's own name, got {:?}",
        functions
    );
}

#[test]
fn impl_trait_for_non_nameable_type_falls_back_to_trait_name() {
    let parser = SynCodeParser::new();
    // (i32, i32) is a tuple type: `type_last_segment` has no nameable
    // segment for it (falls to `_ => None`). The abstract trait method
    // has no default body, so only the impl's override is collected.
    let source =
        "trait Tr { fn foo(&self); } impl Tr for (i32, i32) { fn foo(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(
        functions[0].name, "Tr::foo",
        "a trait impl on a non-nameable self_ty must fall back to the trait's name, got {:?}",
        functions
    );
}

#[test]
fn inherent_impl_for_non_nameable_type_falls_back_to_bare_name() {
    let parser = SynCodeParser::new();
    // An inherent impl (no `for Trait`) on a non-nameable self_ty (a fixed-
    // size array type) has no trait to fall back to either — the bare
    // method name is the only option left.
    let source = "impl [i32; 3] { fn foo(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    assert_eq!(functions.len(), 1, "got {:?}", functions);
    assert_eq!(
        functions[0].name, "foo",
        "an inherent impl on a non-nameable self_ty with no trait must fall back to the bare method name, got {:?}",
        functions
    );
}

// NON-BLOCKING (#50 QA retry 1) — is_bare_self_receiver only accepts
// syn::Expr::Path("self"); `self.field.m()`'s receiver is an
// syn::Expr::Field, not a bare path, so it stays unresolved by direct trace
// of the code. But no test pinned it: mutating is_bare_self_receiver to
// also accept a syn::Expr::Field with a `self` base kept the whole suite
// green. Resolving self.field.m() would fabricate an edge to whatever
// method happens to share that short name (D2 forbids exactly this).
#[test]
fn self_field_method_call_stays_bare_not_resolved_to_enclosing_type() {
    let parser = SynCodeParser::new();
    let source =
        "struct S { inner: T } impl S { fn a(&self) { self.inner.m(); } fn m(&self) { if x { } } }";
    let functions = parser.parse(source).unwrap();
    let a = functions.iter().find(|f| f.name == "S::a").unwrap();
    assert!(
        a.calls.contains(&"m".to_string()),
        "self.inner.m() must stay bare \"m\", got {:?}",
        a.calls
    );
    assert!(
        !a.calls.contains(&"S::m".to_string()),
        "self.inner.m() must NOT be fabricated into S::m, got {:?}",
        a.calls
    );
    let graph = CallGraph::build(&functions);
    assert_eq!(
        graph.hidden_of("S::a"),
        0,
        "no fabricated edge means no hidden complexity from S::m"
    );
}
