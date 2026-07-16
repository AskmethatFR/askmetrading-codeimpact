use codeimpact_hexagon::analysis::source_guard;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::LoopCall;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use syn::spanned::Spanned;

/// The parent's re-parse budget once the canary (`codeimpact-parse-probe`)
/// has proven a source terminates cleanly (exit 0 or 2). Deliberately
/// double the probe's own 16 MiB (`PROBE_STACK_BYTES` in
/// `src/bin/parse_probe.rs`) — stack *dominance*, not equality (D2, #63):
/// the same computation under a strictly larger budget cannot newly
/// overflow, closing the class rather than narrowing it.
const PARENT_REPARSE_STACK_BYTES: usize = 32 * 1024 * 1024;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeVerdict {
    Admissible,
    SyntaxError,
    TooComplex,
}

/// Pure mapping from the canary's exit code to a verdict (D3, #63) — no
/// signal introspection, no `#[cfg(unix)]` branch: `Some(0)`/`Some(2)` are
/// the only codes the canary can emit for a proven-clean termination,
/// everything else (a killed-by-signal `None`, a Windows structured
/// exception code, a stray exit(7), a timeout-kill) is refused.
fn verdict_from(status_code: Option<i32>) -> ProbeVerdict {
    match status_code {
        Some(0) => ProbeVerdict::Admissible,
        Some(2) => ProbeVerdict::SyntaxError,
        _ => ProbeVerdict::TooComplex,
    }
}

/// Locates the `codeimpact-parse-probe` binary (#63): (1) an explicit
/// override — also the escape hatch for fake probes in tests — (2) next to
/// the current executable (production: both binaries ship side by side),
/// (3) next to the current executable's *parent* directory (an
/// integration-test binary lives one level deeper, under `target/*/deps/`,
/// than the workspace's own bin artifacts).
fn discover_probe_path() -> Option<PathBuf> {
    if let Ok(configured) = std::env::var("CODEIMPACT_PARSE_PROBE") {
        return Some(PathBuf::from(configured));
    }

    let exe_name = format!("codeimpact-parse-probe{}", std::env::consts::EXE_SUFFIX);
    let current_exe = std::env::current_exe().ok()?;
    let dir = current_exe.parent()?;

    let sibling = dir.join(&exe_name);
    if sibling.is_file() {
        return Some(sibling);
    }

    let cousin = dir.parent()?.join(&exe_name);
    if cousin.is_file() {
        return Some(cousin);
    }

    None
}

/// Spawns the canary, feeds it `source` over stdin, and waits up to
/// `PROBE_TIMEOUT` — killing it on timeout, which is itself a difference of
/// *nature* (the process never proved it terminates), not a timing margin
/// (ADR-0010's lesson). Never returns an `Err` for the canary's own
/// crash/timeout — only for the canary being unreachable at all, which is
/// an installation problem the caller must surface loudly.
fn probe_source(source: &str) -> Result<ProbeVerdict, AnalysisError> {
    let probe_path = discover_probe_path().ok_or_else(probe_missing_error)?;

    let mut child = std::process::Command::new(&probe_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| probe_missing_error())?;

    if let Some(mut stdin) = child.stdin.take() {
        // A broken pipe (child died before reading) is not this call's
        // concern — the exit status collected below is what decides the
        // verdict either way.
        let _ = stdin.write_all(source.as_bytes());
    }

    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(verdict_from(status.code())),
            Ok(None) if Instant::now() < deadline => {
                // A short poll interval, not `wait-timeout` (cc-kiss, #63
                // T3): the canary's own parse is typically sub-millisecond
                // for an ordinary file, so a coarser interval would turn
                // polling latency into the dominant per-file cost instead
                // of the fork+exec it is meant to be.
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(ProbeVerdict::TooComplex);
            }
            Err(_) => return Ok(ProbeVerdict::TooComplex),
        }
    }
}

fn probe_missing_error() -> AnalysisError {
    AnalysisError::AnalysisFailed(
        "sonde d'analyse introuvable (définissez CODEIMPACT_PARSE_PROBE)".to_string(),
    )
}

/// Parses `source` and runs `extract` against the resulting `syn::File`,
/// both inside one thread carrying `PARENT_REPARSE_STACK_BYTES` of stack.
/// `syn::File` itself is not `Send` (it borrows through `proc_macro2`), so
/// only `extract`'s Send-safe result — never the tree — crosses back out.
fn parse_and_extract<F, T>(source: &str, extract: F) -> Result<T, AnalysisError>
where
    F: FnOnce(&syn::File) -> T + Send + 'static,
    T: Send + 'static,
{
    let owned = source.to_string();
    std::thread::Builder::new()
        .stack_size(PARENT_REPARSE_STACK_BYTES)
        .spawn(move || {
            syn::parse_file(&owned)
                .map(|tree| extract(&tree))
                .map_err(|e| format!("erreur de syntaxe: {}", e))
        })
        .expect("failed to spawn parser thread")
        .join()
        .expect("parser thread panicked")
        .map_err(AnalysisError::AnalysisFailed)
}

/// Runs the SAME parse-and-walk work `parse()` performs — `syn::parse_file`
/// followed by function collection and expression-tree visiting — but
/// discards the result. Used by the canary (`src/bin/parse_probe.rs`,
/// Security finding retry 1, CWE-674): "child survived the canary's 16 MiB
/// ⇒ parent survives its 32 MiB" only holds when the child exercises the
/// SAME recursion the parent will actually run. A canary that only proved
/// `syn::parse_file` succeeds said nothing about `collect_functions`'s
/// mod-nesting walk or `FunctionVisitor`'s expression-tree recursion — a
/// DIFFERENT recursion whose per-frame cost the bare parse never measured.
/// Routing the canary through this exact function closes that gap by
/// construction instead of assuming it.
pub fn exercise_full_pipeline(source: &str) -> Result<(), String> {
    let syntax_tree = syn::parse_file(source).map_err(|e| format!("erreur de syntaxe: {}", e))?;

    let mut pending = Vec::new();
    collect_functions(&syntax_tree.items, "", &mut pending);
    dedupe_names(&mut pending);

    for pf in pending {
        let mut visitor = FunctionVisitor::new(pf.enclosing_type);
        visitor.visit_block(pf.block);
    }

    Ok(())
}

pub struct SynCodeParser {
    /// Single-entry verdict cache (#63, T2): `parse` and
    /// `parse_file_dependencies` are called back-to-back on the same
    /// file's source, so remembering only the *last* probed source avoids
    /// a second fork+exec per file without the complexity of a full map —
    /// nothing in this crate probes more than one file at a time.
    ///
    /// Keyed by FULL SOURCE EQUALITY, not a hash (Security finding,
    /// A04/CWE-354, retry 1): `DefaultHasher` uses fixed keys — it is
    /// deterministic across every process invocation, not randomized like
    /// `HashMap`'s `RandomState` — so a 64-bit collision is precomputable
    /// offline once and reused forever against any deployment. A String
    /// compare is cheap here regardless: `source_guard` already caps every
    /// source at `MAX_MEASURABLE_SOURCE_BYTES` (1 MB) before it reaches
    /// this cache.
    probe_verdict_cache: Mutex<Option<(String, ProbeVerdict)>>,
}

impl Default for SynCodeParser {
    fn default() -> Self {
        Self {
            probe_verdict_cache: Mutex::new(None),
        }
    }
}

impl SynCodeParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn cached_probe(&self, source: &str) -> Result<ProbeVerdict, AnalysisError> {
        {
            let cache = self.probe_verdict_cache.lock().unwrap();
            if let Some((cached_source, verdict)) = cache.as_ref() {
                if cached_source == source {
                    return Ok(*verdict);
                }
            }
        }
        let verdict = probe_source(source)?;
        *self.probe_verdict_cache.lock().unwrap() = Some((source.to_string(), verdict));
        Ok(verdict)
    }

    /// The shared guard in front of both `CodeParser` entry points:
    /// refuses an oversized source (#62), and asks the canary whether this
    /// source is safe to parse (#63). `Ok(())` means the canary proved
    /// this exact source terminates cleanly — the caller may now re-parse
    /// it in a stack-dominant thread via `parse_and_extract`.
    fn guard_admissible(&self, source: &str) -> Result<(), AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;

        match self.cached_probe(source)? {
            ProbeVerdict::TooComplex => Err(AnalysisError::Unmeasurable(
                UnmeasurableReason::SourceTooComplex,
            )),
            ProbeVerdict::Admissible | ProbeVerdict::SyntaxError => Ok(()),
        }
    }
}

impl CodeParser for SynCodeParser {
    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        self.guard_admissible(source)?;

        parse_and_extract(source, |syntax_tree| {
            let mut pending = Vec::new();
            collect_functions(&syntax_tree.items, "", &mut pending);
            dedupe_names(&mut pending);

            let mut functions = Vec::new();
            for pf in pending {
                let mut visitor = FunctionVisitor::new(pf.enclosing_type);
                visitor.visit_block(pf.block);
                functions.push(ParsedFunction {
                    name: pf.name,
                    start_line: pf.start_line,
                    calls: visitor.calls,
                    has_loop: visitor.has_loop,
                    has_nested_loop: visitor.has_nested_loop,
                    decision_points: visitor.decision_points,
                    depth: visitor.max_depth,
                    match_arms: visitor.match_arms,
                    calls_in_loops: visitor.calls_in_loops,
                });
            }
            functions
        })
    }

    fn parse_file_dependencies(&self, source: &str) -> Result<Vec<String>, AnalysisError> {
        self.guard_admissible(source)?;

        parse_and_extract(source, |syntax_tree| {
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

            deps
        })
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

/// A function/method declaration collected from the syntax tree, still
/// carrying its qualified name (D1) and — for methods — the enclosing type
/// name used to resolve `self`/`Self` calls (D2).
struct PendingFn<'a> {
    name: String,
    enclosing_type: Option<String>,
    block: &'a syn::Block,
    start_line: usize,
}

/// Returns the last path segment of a type — generics erased — or `None`
/// when the type has no nameable segment (tuple, array, …). Recurses
/// through `&Type` / `(Type)` so `impl Trait for &Type` still yields `Type`.
fn type_last_segment(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(type_path) => type_path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(reference) => type_last_segment(&reference.elem),
        syn::Type::Paren(paren) => type_last_segment(&paren.elem),
        syn::Type::Group(group) => type_last_segment(&group.elem),
        _ => None,
    }
}

/// The trait name of an `impl Trait for Type` block (D1's fallback qualifier
/// when `self_ty` has no nameable segment — a tuple, an array, ...). `None`
/// for an inherent impl (`impl Type { ... }`, no `for Trait` clause), which
/// has no trait to fall back to.
fn trait_name(item_impl: &syn::ItemImpl) -> Option<String> {
    item_impl
        .trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()))
}

/// Whether a method-call receiver is the bare identifier `self` — not
/// `self.field` or any other expression. Only this exact shape is eligible
/// for `self`-call resolution (D2, #50).
fn is_bare_self_receiver(receiver: &syn::Expr) -> bool {
    matches!(receiver, syn::Expr::Path(path) if path.path.is_ident("self"))
}

/// Whether an item carries `#[cfg(test)]` (D6, #50 slice S3). Rust's own
/// test harness — `#[cfg(test)] mod tests { ... }` — is not production code;
/// leaving it in would count every test function as a production function,
/// inflating the call graph and `hidden_complexity` with code that never
/// runs in production. `#[cfg(test)]` is Rust syntax (ADR-0013: the domain
/// names the concept, the adapter names the syntax), so the exclusion lives
/// here, not in the hexagon.
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .map(|ident| ident == "test")
                .unwrap_or(false)
    })
}

/// Recursively walks top-level items — including `impl` blocks — collecting
/// every function/method declaration as a [`PendingFn`], per the D1
/// qualification scheme (ADR-0013 / #50). Name uniqueness is enforced by
/// the caller after collection (source-order suffixing).
fn collect_functions<'a>(items: &'a [syn::Item], mod_prefix: &str, out: &mut Vec<PendingFn<'a>>) {
    for item in items {
        if let syn::Item::Fn(func) = item {
            out.push(PendingFn {
                name: format!("{}{}", mod_prefix, func.sig.ident),
                enclosing_type: None,
                block: &func.block,
                start_line: func.span().start().line,
            });
        } else if let syn::Item::Impl(item_impl) = item {
            let qualifier = type_last_segment(&item_impl.self_ty).or_else(|| trait_name(item_impl));
            for impl_item in &item_impl.items {
                if let syn::ImplItem::Fn(method) = impl_item {
                    let name = match &qualifier {
                        Some(q) => format!("{}{}::{}", mod_prefix, q, method.sig.ident),
                        None => format!("{}{}", mod_prefix, method.sig.ident),
                    };
                    let enclosing_type = qualifier.as_ref().map(|q| format!("{}{}", mod_prefix, q));
                    out.push(PendingFn {
                        name,
                        enclosing_type,
                        block: &method.block,
                        start_line: method.span().start().line,
                    });
                }
            }
        } else if let syn::Item::Trait(item_trait) = item {
            let trait_name = item_trait.ident.to_string();
            for trait_item in &item_trait.items {
                if let syn::TraitItem::Fn(method) = trait_item {
                    // A trait method without a default body is a signature,
                    // not a function — it must not be emitted (D1).
                    if let Some(default_block) = &method.default {
                        out.push(PendingFn {
                            name: format!("{}{}::{}", mod_prefix, trait_name, method.sig.ident),
                            enclosing_type: Some(format!("{}{}", mod_prefix, trait_name)),
                            block: default_block,
                            start_line: method.span().start().line,
                        });
                    }
                }
            }
        } else if let syn::Item::Mod(item_mod) = item {
            // Inline module (`mod m { … }`) — recurse with its name folded
            // into the prefix, so nested items qualify as `m::T::foo`. A
            // path-style module (`mod m;`, no body) has nothing to recurse
            // into. `#[cfg(test)] mod tests { … }` is excluded outright
            // (D6, #50 slice S3) — it is not production code.
            if is_cfg_test(&item_mod.attrs) {
                continue;
            }
            if let Some((_, sub_items)) = &item_mod.content {
                let new_prefix = format!("{}{}::", mod_prefix, item_mod.ident);
                collect_functions(sub_items, &new_prefix, out);
            }
        }
    }
}

/// Enforces uniqueness of qualified names in source-collection order: the
/// first declaration keeps its bare name, every later collision is
/// suffixed `#2`, `#3`, … A duplicate that clobbered another (e.g. an
/// inherent `S::f` and a trait-impl `S::f`) would otherwise be dropped by
/// `CallGraph::build`'s `edges.insert(f.name, …)` — losing a whole
/// function's complexity and edges (D1, #50).
fn dedupe_names(pending: &mut [PendingFn]) {
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for pf in pending.iter_mut() {
        let count = seen.entry(pf.name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            pf.name = format!("{}#{}", pf.name, count);
        }
    }
}

#[derive(Default)]
struct FunctionVisitor {
    decision_points: u32,
    calls: Vec<String>,
    calls_in_loops: Vec<LoopCall>,
    has_loop: bool,
    has_nested_loop: bool,
    max_depth: u32,
    current_depth: u32,
    loop_depth: u32,
    match_arms: u32,
    /// The qualified name of the enclosing `impl`/`trait` type, when this
    /// visitor is walking a method body. Used to resolve `self.m()` and
    /// `Self::m()` to the callee's qualified declaration (D2, #50) — `None`
    /// for a free function, where no such resolution applies.
    enclosing_type: Option<String>,
}

impl FunctionVisitor {
    fn new(enclosing_type: Option<String>) -> Self {
        Self {
            enclosing_type,
            ..Self::default()
        }
    }

    /// Records a call — free-function or method — reached at any nesting
    /// level. When nested inside a loop, it is also recorded as a
    /// `LoopCall` fact, classified (not filtered) by `is_io_call`: every
    /// detector reading `calls_in_loops` decides for itself which facts it
    /// cares about.
    fn record_call<S: Spanned>(&mut self, name: String, spanned: &S) {
        if self.loop_depth > 0 {
            let line_col = spanned.span().start();
            self.calls_in_loops.push(LoopCall {
                name: name.clone(),
                line: line_col.line,
                col: line_col.column,
                is_io: is_io_call(&name),
            });
        }
        self.calls.push(name);
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
                // A nested `fn` cannot capture (or declare) `self`, so it
                // never needs `self`/`Self` resolution — unlike a closure,
                // which shares this same visitor instance and its context.
                let mut inner = FunctionVisitor::new(None);
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
                    let mut segments: Vec<String> = path
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect();
                    // `Self::b(...)` — the leading `Self` segment is
                    // rewritten to the enclosing type, so the recorded name
                    // matches the callee's own qualified declaration (D2).
                    // A `Type::b(...)` UFCS path already matches naturally
                    // and needs no rewrite.
                    if let (Some(first), Some(qualifier)) =
                        (segments.first_mut(), &self.enclosing_type)
                    {
                        if first == "Self" {
                            *first = qualifier.clone();
                        }
                    }
                    let name = segments.join("::");
                    self.record_call(name, call.func.as_ref());
                }
                for arg in &call.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::MethodCall(method_call) => {
                let method_name = method_call.method.to_string();
                // Only a bare `self.m()` — receiver is exactly `self`, no
                // field/deref in between — is resolved to the enclosing
                // type's declaration. `self.field.m()` or `x.m()` stay bare:
                // resolving those by short-name homonym would fabricate an
                // edge to code that may never actually be called (D2, #50).
                let name = match &self.enclosing_type {
                    Some(qualifier) if is_bare_self_receiver(&method_call.receiver) => {
                        format!("{}::{}", qualifier, method_name)
                    }
                    _ => method_name,
                };
                self.record_call(name, &method_call.method);
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

    /// Every test in this module now goes through the real canary (#63) —
    /// `cargo test -p codeimpact_secondaries --lib` does not build sibling
    /// bin targets on its own, so the probe must be built on demand, the
    /// same way the e2e/integration test crates already do for the CLI
    /// binary itself.
    fn ensure_probe_built() {
        let workspace_root = {
            let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            for _ in 0..4 {
                p.pop();
            }
            p
        };
        let probe = workspace_root.join("target").join("debug").join(format!(
            "codeimpact-parse-probe{}",
            std::env::consts::EXE_SUFFIX
        ));
        if !probe.exists() {
            let status = std::process::Command::new("cargo")
                .args([
                    "build",
                    "-p",
                    "codeimpact_secondaries",
                    "--bin",
                    "codeimpact-parse-probe",
                ])
                .current_dir(&workspace_root)
                .status()
                .expect("failed to build probe binary");
            assert!(status.success(), "probe binary build failed");
        }
    }

    fn parser() -> SynCodeParser {
        ensure_probe_built();
        SynCodeParser::new()
    }

    // ── Test List (source_guard wiring, #62) ──────────────────────────
    //   1. oversized_source_refused_before_syn_runs — >1 MB →
    //      Err(Unmeasurable(SourceTooLarge)), structurally (no RSS assertion).
    //   2. parse_file_dependencies_refused_when_source_too_large — same
    //      guard, mirrored through the parse_file_dependencies entry point.
    //   3. normal_source_still_parses — regression: normal source still
    //      parses with the expected functions.

    // ── Test List (verdict_from mapping, #63) ─────────────────────────
    // One behavior — "only 0/2 are proven-clean, everything else is
    // refused" — six rows, one parameterized cycle:
    //   0 -> Admissible; 2 -> SyntaxError; None (killed by signal),
    //   0xC00000FD (Windows STATUS_STACK_OVERFLOW), 101 (panic exit),
    //   7 (arbitrary unknown code) -> TooComplex.

    #[test]
    fn verdict_from_maps_exit_codes() {
        assert_eq!(verdict_from(Some(0)), ProbeVerdict::Admissible);
        assert_eq!(verdict_from(Some(2)), ProbeVerdict::SyntaxError);
        assert_eq!(verdict_from(None), ProbeVerdict::TooComplex);
        assert_eq!(
            verdict_from(Some(0xC00000FDu32 as i32)),
            ProbeVerdict::TooComplex
        );
        assert_eq!(verdict_from(Some(101)), ProbeVerdict::TooComplex);
        assert_eq!(verdict_from(Some(7)), ProbeVerdict::TooComplex);
    }

    // ── Test List (exercise_full_pipeline, #63 security retry 1, CWE-674) ──
    //   1. exercise_full_pipeline_succeeds_on_normal_source — a small,
    //      shallow source (no crash risk in-process) walks cleanly.
    //   2. exercise_full_pipeline_reports_syntax_errors — a syntax error
    //      surfaces as Err, mirroring parse()'s own message.

    #[test]
    fn exercise_full_pipeline_succeeds_on_normal_source() {
        let result = exercise_full_pipeline("fn f() { if x > 0 { for i in 0..3 {} } }");
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[test]
    fn exercise_full_pipeline_reports_syntax_errors() {
        let result = exercise_full_pipeline("this is not valid rust @@@");
        match result {
            Err(msg) => assert!(
                msg.contains("erreur de syntaxe"),
                "expected a syntax-error message, got: {}",
                msg
            ),
            Ok(()) => panic!("expected Err for invalid syntax"),
        }
    }

    #[test]
    fn oversized_source_refused_before_syn_runs() {
        let source = "a".repeat(1024 * 1024 + 1);
        let parser = parser();
        let result = parser.parse(&source);
        match result {
            Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge)) => {}
            other => panic!("expected Unmeasurable(SourceTooLarge), got {:?}", other),
        }
    }

    #[test]
    fn parse_file_dependencies_refused_when_source_too_large() {
        let source = "a".repeat(1024 * 1024 + 1);
        let parser = parser();
        let result = parser.parse_file_dependencies(&source);
        match result {
            Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge)) => {}
            other => panic!("expected Unmeasurable(SourceTooLarge), got {:?}", other),
        }
    }

    #[test]
    fn normal_source_still_parses() {
        let parser = parser();
        let source = "fn a() { if x > 0 { } }\nfn b() { while true { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, "a");
        assert_eq!(functions[1].name, "b");
    }

    #[test]
    fn empty_source_returns_no_functions() {
        let parser = parser();
        let functions = parser.parse("").unwrap();
        assert!(functions.is_empty());
    }

    #[test]
    fn no_branching_returns_no_decision_points() {
        let parser = parser();
        let source = "fn hello() { let x = 1; }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "hello");
        assert_eq!(functions[0].decision_points, 0);
    }

    #[test]
    fn one_if_statement_counts_one_decision_point() {
        let parser = parser();
        let source = "fn test() { if x > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
    }

    #[test]
    fn if_else_counts_one_decision_point() {
        let parser = parser();
        let source = "fn test() { if x > 0 { } else { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
    }

    #[test]
    fn if_else_if_counts_two_decision_points() {
        let parser = parser();
        let source = "fn test() { if x > 0 { } else if x < 0 { } else { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn while_loop_counts_one_decision_point() {
        let parser = parser();
        let source = "fn test() { while x > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
        assert!(functions[0].has_loop);
    }

    #[test]
    fn for_loop_counts_one_decision_point() {
        let parser = parser();
        let source = "fn test() { for i in 0..10 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
        assert!(functions[0].has_loop);
    }

    #[test]
    fn match_arm_counts_per_arm() {
        let parser = parser();
        let source = "fn test() { match x { 1 => {}, 2 => {}, _ => {} } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 3);
    }

    #[test]
    fn and_operator_counts_as_decision_point() {
        let parser = parser();
        let source = "fn test() { if x > 0 && y > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn or_operator_counts_as_decision_point() {
        let parser = parser();
        let source = "fn test() { if x > 0 || y > 0 { } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn catch_method_call_not_counted() {
        let parser = parser();
        let source = "fn test() { let _ = std::fs::read(\"file\").catch(|_| {}); }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 0);
    }

    #[test]
    fn and_in_string_not_counted() {
        let parser = parser();
        let source = "fn test() { let s = \"a && b\"; }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 0);
    }

    #[test]
    fn function_calls_are_tracked() {
        let parser = parser();
        let source = "fn test() { foo(); bar::baz(); }";
        let functions = parser.parse(source).unwrap();
        assert!(functions[0].calls.contains(&"foo".to_string()));
        assert!(functions[0].calls.contains(&"bar::baz".to_string()));
    }

    #[test]
    fn method_calls_are_tracked() {
        let parser = parser();
        let source = "fn test() { let _ = x.foo().bar(); }";
        let functions = parser.parse(source).unwrap();
        assert!(functions[0].calls.contains(&"foo".to_string()));
        assert!(functions[0].calls.contains(&"bar".to_string()));
    }

    #[test]
    fn nested_loop_detected() {
        let parser = parser();
        let source = "fn test() { for i in 0..10 { while true { } } }";
        let functions = parser.parse(source).unwrap();
        assert!(functions[0].has_loop);
        assert!(functions[0].has_nested_loop);
    }

    #[test]
    fn nesting_depth_tracked() {
        let parser = parser();
        let source = "fn test() { if x > 0 { if y > 0 { if z > 0 { } } } }";
        let functions = parser.parse(source).unwrap();
        assert_eq!(functions[0].depth, 3);
    }

    #[test]
    fn multiple_functions_parsed_separately() {
        let parser = parser();
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
        let parser = parser();
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
        let parser = parser();
        let result = parser.parse("this is not valid rust code @@@");
        assert!(result.is_err());
    }

    // ── parse_file_dependencies tests ──

    #[test]
    fn deps_mod_foo_extracted() {
        let parser = parser();
        let deps = parser.parse_file_dependencies("mod foo;").unwrap();
        assert_eq!(deps, vec!["mod:foo"]);
    }

    #[test]
    fn deps_mod_with_inline_content_skipped() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("mod foo { fn bar() {} }")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_std_filtered() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use std::collections::HashMap;")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_core_filtered() {
        let parser = parser();
        let deps = parser.parse_file_dependencies("use core::mem;").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_alloc_filtered() {
        let parser = parser();
        let deps = parser.parse_file_dependencies("use alloc::vec;").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_crate_extracted() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use crate::foo::bar;")
            .unwrap();
        assert_eq!(deps, vec!["use:crate::foo::bar"]);
    }

    #[test]
    fn deps_use_super_extracted() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use super::foo::bar;")
            .unwrap();
        assert_eq!(deps, vec!["use:super::foo::bar"]);
    }

    #[test]
    fn deps_use_relative_extracted() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use foo::bar::Baz;")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar::Baz"]);
    }

    #[test]
    fn deps_use_group_expanded() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use foo::{bar, baz};")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar, baz"]);
    }

    #[test]
    fn deps_empty_source_returns_empty() {
        let parser = parser();
        let deps = parser.parse_file_dependencies("").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_no_mod_or_use_returns_empty() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("fn foo() { let x = 1; }")
            .unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_glob() {
        let parser = parser();
        let deps = parser.parse_file_dependencies("use foo::*;").unwrap();
        assert_eq!(deps, vec!["use:foo::*"]);
    }

    #[test]
    fn parse_use_rename_is_captured() {
        let parser = parser();
        let deps = parser
            .parse_file_dependencies("use foo::bar as baz;\nfn main() {}")
            .unwrap();
        assert_eq!(deps, vec!["use:foo::bar"]);
    }
}
