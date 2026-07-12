use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::ParsedFunction;
use syn::spanned::Spanned;

#[derive(Default)]
pub struct SynCodeParser;

impl SynCodeParser {
    pub fn new() -> Self {
        Self
    }
}

impl CodeParser for SynCodeParser {
    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        let syntax_tree = syn::parse_file(source)
            .map_err(|e| AnalysisError::AnalysisFailed(format!("erreur de syntaxe: {}", e)))?;

        let mut functions = Vec::new();

        for item in &syntax_tree.items {
            if let syn::Item::Fn(func) = item {
                let name = func.sig.ident.to_string();
                let mut visitor = FunctionVisitor::new();
                visitor.visit_block(&func.block);
                functions.push(ParsedFunction {
                    name,
                    start_line: func.span().start().line,
                    calls: visitor.calls,
                    has_loop: visitor.has_loop,
                    has_nested_loop: visitor.has_nested_loop,
                    decision_points: visitor.decision_points,
                    depth: visitor.max_depth,
                    match_arms: visitor.match_arms,
                    calls_in_loops: visitor.calls_in_loops,
                });
            }
        }

        Ok(functions)
    }

    fn parse_file_dependencies(&self, source: &str) -> Result<Vec<String>, AnalysisError> {
        let syntax_tree = syn::parse_file(source)
            .map_err(|e| AnalysisError::AnalysisFailed(format!("erreur de syntaxe: {}", e)))?;

        let mut deps = Vec::new();

        for item in &syntax_tree.items {
            match item {
                syn::Item::Mod(m) => {
                    // `mod foo;` (path-style, external file) — no content, has semicolon
                    if m.content.is_none() {
                        deps.push(format!("mod:{}", m.ident));
                    }
                }
                syn::Item::Use(u) => {
                    let use_path = Self::format_use_tree(&u.tree);
                    let lower = use_path.to_lowercase();
                    if !lower.starts_with("std::")
                        && !lower.starts_with("core::")
                        && !lower.starts_with("alloc::")
                    {
                        deps.push(format!("use:{}", use_path));
                    }
                }
                _ => {}
            }
        }

        Ok(deps)
    }
}

// ── Private helpers ──

const IO_PREFIXES: &[&str] = &["std::fs::", "tokio::fs::", "std::net::", "reqwest::"];

fn is_io_call(call_name: &str) -> bool {
    IO_PREFIXES
        .iter()
        .any(|prefix| call_name.starts_with(prefix))
}

impl SynCodeParser {
    fn format_use_tree(tree: &syn::UseTree) -> String {
        match tree {
            syn::UseTree::Path(path) => {
                let prefix = path.ident.to_string();
                let suffix = Self::format_use_tree(&path.tree);
                format!("{}::{}", prefix, suffix)
            }
            syn::UseTree::Name(name) => name.ident.to_string(),
            syn::UseTree::Glob(_) => "*".to_string(),
            syn::UseTree::Rename(rename) => rename.ident.to_string(),
            syn::UseTree::Group(group) => {
                let items: Vec<String> = group.items.iter().map(Self::format_use_tree).collect();
                items.join(", ")
            }
        }
    }
}

#[derive(Default)]
struct FunctionVisitor {
    decision_points: u32,
    calls: Vec<String>,
    calls_in_loops: Vec<(String, usize, usize)>,
    has_loop: bool,
    has_nested_loop: bool,
    max_depth: u32,
    current_depth: u32,
    loop_depth: u32,
    match_arms: u32,
}

impl FunctionVisitor {
    fn new() -> Self {
        Self::default()
    }

    fn visit_block(&mut self, block: &syn::Block) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &syn::Stmt) {
        match stmt {
            syn::Stmt::Expr(expr, _) => {
                self.visit_expr(expr);
            }
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    self.visit_expr(&init.expr);
                }
            }
            syn::Stmt::Item(syn::Item::Fn(func)) => {
                let mut inner = FunctionVisitor::new();
                inner.visit_block(&func.block);
                self.decision_points += inner.decision_points;
                self.calls.extend(inner.calls);
                self.calls_in_loops.extend(inner.calls_in_loops);
                if inner.has_loop {
                    self.has_loop = true;
                }
                if inner.has_nested_loop {
                    self.has_nested_loop = true;
                }
            }
            syn::Stmt::Item(_) => {}
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::If(expr_if) => {
                self.decision_points += 1;
                self.current_depth += 1;
                self.max_depth = self.max_depth.max(self.current_depth);

                self.visit_expr(&expr_if.cond);
                self.visit_block(&expr_if.then_branch);

                if let Some((_, else_expr)) = &expr_if.else_branch {
                    self.visit_else_branch(else_expr);
                }

                self.current_depth -= 1;
            }
            syn::Expr::While(expr_while) => {
                self.decision_points += 1;
                self.has_loop = true;
                self.current_depth += 1;
                self.loop_depth += 1;
                if self.loop_depth > 1 {
                    self.has_nested_loop = true;
                }
                self.max_depth = self.max_depth.max(self.current_depth);

                self.visit_expr(&expr_while.cond);
                self.visit_block(&expr_while.body);

                self.loop_depth -= 1;
                self.current_depth -= 1;
            }
            syn::Expr::ForLoop(expr_for) => {
                self.decision_points += 1;
                self.has_loop = true;
                self.current_depth += 1;
                self.loop_depth += 1;
                if self.loop_depth > 1 {
                    self.has_nested_loop = true;
                }
                self.max_depth = self.max_depth.max(self.current_depth);

                self.visit_expr(&expr_for.expr);
                self.visit_block(&expr_for.body);

                self.loop_depth -= 1;
                self.current_depth -= 1;
            }
            syn::Expr::Loop(expr_loop) => {
                self.decision_points += 1;
                self.has_loop = true;
                self.current_depth += 1;
                self.loop_depth += 1;
                if self.loop_depth > 1 {
                    self.has_nested_loop = true;
                }
                self.max_depth = self.max_depth.max(self.current_depth);

                self.visit_block(&expr_loop.body);

                self.loop_depth -= 1;
                self.current_depth -= 1;
            }
            syn::Expr::Match(expr_match) => {
                let arm_count = expr_match.arms.len() as u32;
                self.match_arms = self.match_arms.max(arm_count);
                if arm_count > 0 {
                    self.decision_points += arm_count;
                }
                self.current_depth += 1;
                self.max_depth = self.max_depth.max(self.current_depth);

                self.visit_expr(&expr_match.expr);
                for arm in &expr_match.arms {
                    if let Some((_, guard)) = &arm.guard {
                        self.visit_expr(guard);
                    }
                    self.visit_expr(&arm.body);
                }

                self.current_depth -= 1;
            }
            syn::Expr::Binary(binary) => {
                if matches!(binary.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
                    self.decision_points += 1;
                }
                self.visit_expr(&binary.left);
                self.visit_expr(&binary.right);
            }
            syn::Expr::Call(call) => {
                if let syn::Expr::Path(path) = call.func.as_ref() {
                    let name = path
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    if self.loop_depth > 0 && is_io_call(&name) {
                        let span = call.func.span();
                        let line_col = span.start();
                        self.calls_in_loops
                            .push((name.clone(), line_col.line, line_col.column));
                    }
                    self.calls.push(name);
                }
                for arg in &call.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::MethodCall(method_call) => {
                self.calls.push(method_call.method.to_string());
                self.visit_expr(&method_call.receiver);
                for arg in &method_call.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::Block(block) => {
                self.visit_block(&block.block);
            }
            syn::Expr::Closure(closure) => {
                self.visit_expr(&closure.body);
            }
            syn::Expr::Tuple(tuple) => {
                for elem in &tuple.elems {
                    self.visit_expr(elem);
                }
            }
            syn::Expr::Paren(paren) => {
                self.visit_expr(&paren.expr);
            }
            syn::Expr::Let(let_expr) => {
                self.visit_expr(&let_expr.expr);
            }
            syn::Expr::TryBlock(try_block) => {
                self.decision_points += 1;
                self.current_depth += 1;
                self.max_depth = self.max_depth.max(self.current_depth);
                self.visit_block(&try_block.block);
                self.current_depth -= 1;
            }
            syn::Expr::Unary(unary) => {
                self.visit_expr(&unary.expr);
            }
            syn::Expr::Field(field) => {
                self.visit_expr(&field.base);
            }
            syn::Expr::Index(index) => {
                self.visit_expr(&index.expr);
                self.visit_expr(&index.index);
            }
            syn::Expr::Range(range) => {
                if let Some(start) = &range.start {
                    self.visit_expr(start);
                }
                if let Some(end) = &range.end {
                    self.visit_expr(end);
                }
            }
            syn::Expr::Cast(cast) => {
                self.visit_expr(&cast.expr);
            }
            syn::Expr::Reference(reference) => {
                self.visit_expr(&reference.expr);
            }
            syn::Expr::Return(ret) => {
                if let Some(expr) = &ret.expr {
                    self.visit_expr(expr);
                }
            }
            syn::Expr::Assign(assign) => {
                self.visit_expr(&assign.left);
                self.visit_expr(&assign.right);
            }
            syn::Expr::Await(await_expr) => {
                self.visit_expr(&await_expr.base);
            }
            syn::Expr::Try(try_expr) => {
                self.visit_expr(&try_expr.expr);
            }
            syn::Expr::Struct(struct_expr) => {
                for field in &struct_expr.fields {
                    self.visit_expr(&field.expr);
                }
            }
            syn::Expr::Repeat(repeat) => {
                self.visit_expr(&repeat.expr);
                self.visit_expr(&repeat.len);
            }
            syn::Expr::Array(array) => {
                for elem in &array.elems {
                    self.visit_expr(elem);
                }
            }
            syn::Expr::Lit(_) => {}
            syn::Expr::Path(_) => {}
            syn::Expr::Continue(_) => {}
            syn::Expr::Break(brk) => {
                if let Some(expr) = &brk.expr {
                    self.visit_expr(expr);
                }
            }
            syn::Expr::Unsafe(unsafe_block) => {
                self.visit_block(&unsafe_block.block);
            }
            syn::Expr::Async(async_expr) => {
                self.visit_block(&async_expr.block);
            }
            _ => {}
        }
    }

    fn visit_else_branch(&mut self, else_expr: &syn::Expr) {
        match else_expr {
            syn::Expr::If(else_if) => {
                self.decision_points += 1;
                self.current_depth += 1;
                self.max_depth = self.max_depth.max(self.current_depth);
                self.visit_expr(&else_if.cond);
                self.visit_block(&else_if.then_branch);
                if let Some((_, deeper_else)) = &else_if.else_branch {
                    self.visit_else_branch(deeper_else);
                }
                self.current_depth -= 1;
            }
            syn::Expr::Block(block) => {
                self.visit_block(&block.block);
            }
            _ => {
                self.visit_expr(else_expr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn match_arm_counts_per_arm() {
        let parser = SynCodeParser::new();
        let source = "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 3);
    }

    #[test]
    fn and_operator_counts_as_decision_point() {
        let parser = SynCodeParser::new();
        let source = "fn test() { if x > 0 && y > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn or_operator_counts_as_decision_point() {
        let parser = SynCodeParser::new();
        let source = "fn test() { if x > 0 || y > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn catch_method_call_not_counted() {
        let parser = SynCodeParser::new();
        let source = "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 0);
    }

    #[test]
    fn and_in_string_not_counted() {
        let parser = SynCodeParser::new();
        let source = "fn test() { let s = \"a && b\"; }";
        let functions = parser.parse(source).unwrap();
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

    // ── parse_file_dependencies tests ──

    #[test]
    fn deps_mod_foo_extracted() {
        let parser = SynCodeParser::new();
        let deps = parser.parse_file_dependencies("mod foo;").unwrap();
        assert_eq!(deps, vec!["mod:foo"]);
    }

    #[test]
    fn deps_mod_with_inline_content_skipped() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("mod foo { fn bar() {} }")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_std_filtered() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use std::collections::HashMap;")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_core_filtered() {
        let parser = SynCodeParser::new();
        let deps = parser.parse_file_dependencies("use core::mem;").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_alloc_filtered() {
        let parser = SynCodeParser::new();
        let deps = parser.parse_file_dependencies("use alloc::vec;").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_crate_extracted() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use crate::foo::bar;")
            .unwrap();
        assert_eq!(deps, vec!["use:crate::foo::bar"]);
    }

    #[test]
    fn deps_use_super_extracted() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use super::foo::bar;")
            .unwrap();
        assert_eq!(deps, vec!["use:super::foo::bar"]);
    }

    #[test]
    fn deps_use_relative_extracted() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use foo::bar::Baz;")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar::Baz"]);
    }

    #[test]
    fn deps_use_group_expanded() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use foo::{bar, baz};")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar, baz"]);
    }

    #[test]
    fn deps_empty_source_returns_empty() {
        let parser = SynCodeParser::new();
        let deps = parser.parse_file_dependencies("").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_no_mod_or_use_returns_empty() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("fn foo() { let x = 1; }")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_glob() {
        let parser = SynCodeParser::new();
        let deps = parser.parse_file_dependencies("use foo::*;").unwrap();
        assert_eq!(deps, vec!["use:foo::*"]);
    }

    #[test]
    fn parse_use_rename_is_captured() {
        let parser = SynCodeParser::new();
        let deps = parser
            .parse_file_dependencies("use foo::bar as baz;\nfn main() {}")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar"]);
    }
}
