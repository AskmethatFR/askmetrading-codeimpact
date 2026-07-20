use codeimpact_hexagon::analysis::source_guard;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::DependencyContext;
use codeimpact_hexagon::analysis::IoClassification;
use codeimpact_hexagon::analysis::LoopCall;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use syn::spanned::Spanned;

/// Named byte-unit constant (PR #70 review: no bare `1024` magic number).
const BYTES_PER_MIB: usize = 1024 * 1024;

/// The parent's re-parse budget once the canary (`codeimpact-parse-probe`)
/// has proven a source terminates cleanly (exit 0 or 2). Deliberately
/// double the probe's own 16 MiB (`PROBE_STACK_BYTES` in
/// `src/bin/parse_probe.rs`) — stack *dominance*, not equality (D2, #63):
/// the same computation under a strictly larger budget cannot newly
/// overflow, closing the class rather than narrowing it.
const PARENT_REPARSE_STACK_BYTES: usize = 32 * BYTES_PER_MIB;

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
/// Security finding retry 1, CWE-674) so the canary exercises the SAME
/// recursion the parent's re-parse will actually run, not just
/// `syn::parse_file`'s first stage.
///
/// Honest status (retry 2, Dev-B): the empirical search this fix was
/// based on (binary-searching for a depth where a bare `syn::parse_file`
/// succeeds in 16 MiB but this full walk crashes in the same budget)
/// found NO such gap for either recursive vector tested (nested `mod`,
/// the `!!!!...x` unary chain) — `collect_functions`/`FunctionVisitor`
/// run AFTER `syn::parse_file`'s own recursion has fully unwound, so the
/// two do not compound, and reverting the canary to a bare
/// `syn::parse_file` leaves the whole test suite green (no test here
/// reddens on that reversion). This function is therefore an
/// ARCHITECTURAL justification — the canary now runs like-for-like with
/// the parent by construction, closing the *assumption* gap CWE-674
/// flagged — not something a failing test proved necessary. See
/// `parse_probe_full_pipeline_test.rs` for the same disclosure applied to
/// its regression test.
///
/// # Safety (not `unsafe fn` — a process-boundary invariant, not a memory
/// one)
///
/// This function recurses over the parsed source with NO stack bound and
/// NO memory cap of its own — on pathological input it aborts (SIGABRT)
/// WHATEVER process calls it. The entire #63 guarantee depends on it only
/// ever being invoked inside a stack-bounded thread (`PROBE_STACK_BYTES`)
/// in a process that has also applied `RLIMIT_AS` (unix) — i.e. only from
/// `src/bin/parse_probe.rs`'s `main()`. Do NOT lift this call into an
/// unguarded in-process path (e.g. "avoid the duplicate walk, call it
/// directly from `parse()`") — that would silently reopen the crash this
/// ticket closes.
pub fn exercise_full_pipeline(source: &str) -> Result<(), String> {
    let syntax_tree = syn::parse_file(source).map_err(|e| format!("erreur de syntaxe: {}", e))?;

    let mut pending = Vec::new();
    collect_functions(&syntax_tree.items, "", &mut pending);
    dedupe_names(&mut pending);
    let locally_declared_types = Rc::new(collect_locally_declared_type_names(&syntax_tree.items));

    for pf in pending {
        let mut visitor = FunctionVisitor::new(
            pf.enclosing_type,
            pf.params,
            Rc::clone(&locally_declared_types),
        );
        visitor.visit_block(pf.block);
    }

    Ok(())
}

pub struct SynCodeParser {
    /// Single-entry verdict cache (#63, T2): `parse` and
    /// `resolve_dependencies` are called back-to-back on the same
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
    fn language(&self) -> codeimpact_hexagon::analysis::Language {
        codeimpact_hexagon::analysis::Language::Rust
    }

    fn capabilities(&self) -> codeimpact_hexagon::analysis::LanguageCapabilities {
        codeimpact_hexagon::analysis::LanguageCapabilities::all_supported(self.language())
    }

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        self.guard_admissible(source)?;

        parse_and_extract(source, |syntax_tree| {
            let mut pending = Vec::new();
            collect_functions(&syntax_tree.items, "", &mut pending);
            dedupe_names(&mut pending);
            let locally_declared_types =
                Rc::new(collect_locally_declared_type_names(&syntax_tree.items));

            let mut functions = Vec::new();
            for pf in pending {
                let mut visitor = FunctionVisitor::new(
                    pf.enclosing_type,
                    pf.params,
                    Rc::clone(&locally_declared_types),
                );
                visitor.visit_block(pf.block);
                functions.push(ParsedFunction {
                    name: pf.name,
                    start_line: pf.start_line,
                    calls: visitor.calls,
                    has_loop: visitor.has_loop,
                    has_nested_loop: visitor.has_nested_loop,
                    decision_points: visitor.decision_points,
                    depth: visitor.max_depth,
                    branch_arms: visitor.branch_arms,
                    calls_in_loops: visitor.calls_in_loops,
                });
            }
            functions
        })
    }

    /// Resolves `source`'s `mod`/`use` declarations to actual files in
    /// `ctx.available_files` (US14 L1/L2) — Rust's module/namespace syntax
    /// (`crate::`, `super::`, `.rs`, `mod.rs`) is entirely owned by this
    /// adapter; the hexagon only ever sees the resolved `PathBuf`s.
    fn resolve_dependencies(
        &self,
        source: &str,
        ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        self.guard_admissible(source)?;

        let ctx = ctx.clone();
        parse_and_extract(source, move |syntax_tree| {
            extract_raw_dependencies(syntax_tree)
                .iter()
                .filter_map(|raw| resolve_dependency(raw, &ctx))
                .collect()
        })
    }
}

/// Extracts raw `"mod:<name>"` / `"use:<path>"` dependency strings from a
/// parsed source — Rust's own `mod foo;` (path-style, external file) and
/// `use foo::bar;` declarations. External crate prefixes (`std::`, `core::`,
/// `alloc::`) are filtered out. Private to this adapter (US14 L2): the
/// hexagon never sees this string protocol, only `resolve_dependencies`'s
/// resolved `PathBuf`s.
fn extract_raw_dependencies(syntax_tree: &syn::File) -> Vec<String> {
    let mut deps = Vec::new();

    for item in &syntax_tree.items {
        match item {
            syn::Item::Mod(m) => {
                if m.content.is_none() {
                    deps.push(format!("mod:{}", m.ident));
                }
            }
            syn::Item::Use(u) => {
                let use_path = SynCodeParser::format_use_tree(&u.tree);
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
}

/// Resolves a raw dependency string (from `extract_raw_dependencies`) to a
/// file path in `ctx.available_files` — Rust's module-resolution semantics
/// (US14 L1: moved here from the hexagon's `file_consumption_graph`, which
/// keeps only pure graph algebra). `None` when the dependency cannot be
/// resolved to any available file.
fn resolve_dependency(raw: &str, ctx: &DependencyContext) -> Option<PathBuf> {
    if let Some(name) = raw.strip_prefix("mod:") {
        let parent = ctx.current_file.parent().unwrap_or(Path::new(""));
        let candidates = vec![
            parent.join(format!("{}.rs", name)),
            parent.join(name).join("mod.rs"),
        ];
        return candidates
            .into_iter()
            .find(|c| ctx.available_files.contains(c));
    }

    if let Some(path) = raw.strip_prefix("use:") {
        // External crate prefixes are already filtered out.
        if path.starts_with("crate::") {
            let rel = path.strip_prefix("crate::").unwrap();
            let candidates = module_path_candidates(rel, &ctx.project_root);
            return candidates
                .into_iter()
                .find(|c| ctx.available_files.contains(c));
        }
        if path.starts_with("super::") {
            let rel = path.strip_prefix("super::").unwrap();
            let parent = ctx.current_file.parent().unwrap_or(Path::new(""));
            let grandparent = parent.parent().unwrap_or(Path::new(""));
            let candidates = module_path_candidates(rel, grandparent);
            return candidates
                .into_iter()
                .find(|c| ctx.available_files.contains(c));
        }
        // Relative use: resolve relative to current file's directory.
        let parent = ctx.current_file.parent().unwrap_or(Path::new(""));
        let candidates = module_path_candidates(path, parent);
        return candidates
            .into_iter()
            .find(|c| ctx.available_files.contains(c));
    }

    None
}

/// Generates candidate file paths for a module path (e.g. `"foo::bar::Baz"`).
///
/// Tries both `base/foo/bar/Baz.rs` / `base/foo/bar/Baz/mod.rs` (full path)
/// and `base/foo/bar.rs` / `base/foo/bar/mod.rs` (last segment is a type).
fn module_path_candidates(module_path: &str, base: &Path) -> Vec<PathBuf> {
    let path = module_path.replace("::", "/");
    let mut candidates = Vec::new();

    candidates.push(base.join(format!("{}.rs", path)));
    candidates.push(base.join(&path).join("mod.rs"));

    if let Some(last_slash) = path.rfind('/') {
        let module_part = &path[..last_slash];
        candidates.push(base.join(format!("{}.rs", module_part)));
        candidates.push(base.join(module_part).join("mod.rs"));
    }

    candidates
}

// ── Private helpers ──

const IO_PREFIXES: &[&str] = &["std::fs::", "tokio::fs::", "std::net::", "reqwest::"];

fn is_io_call(call_name: &str) -> bool {
    IO_PREFIXES
        .iter()
        .any(|prefix| call_name.starts_with(prefix))
}

/// Known I/O types (#56 T1, C1) — a method call is `is_io: true` only when
/// its receiver's declared type resolves to one of these. std/tokio/reqwest,
/// matching the free-function `IO_PREFIXES` this list complements.
const KNOWN_IO_TYPES: &[&str] = &[
    "File",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "Client",
    "Response",
    "BufReader",
    "BufWriter",
    "Stdin",
    "Stdout",
    "Stderr",
];

fn is_io_type(type_name: &str) -> bool {
    KNOWN_IO_TYPES.contains(&type_name)
}

/// Method names suspicious enough that an UNRESOLVED receiver is reported
/// `Unknown` rather than silently written off as `NotIo` (#56 T2,
/// human-approved Q3). Deliberately narrow — `get`/`post`/`fetch`/`open`/
/// `create` are common non-I/O method names on ordinary types (builders,
/// options, factories) and would flood `Unknown` with name-collision noise.
/// Pruned by T3 measurement; kept as a named constant, never inlined.
const SUSPICIOUS_METHOD_NAMES: &[&str] = &[
    "read",
    "read_to_string",
    "read_to_end",
    "read_exact",
    "read_line",
    "write",
    "write_all",
    "flush",
    "send",
    "recv",
    "query",
    "execute",
    "connect",
    "accept",
    "sync_all",
    "copy",
];

fn is_suspicious_method_name(method_name: &str) -> bool {
    SUSPICIOUS_METHOD_NAMES.contains(&method_name)
}

/// Free-function classification (#56 T2) — unchanged semantics from T1's
/// `is_io_call`, now wrapped in the three-state `IoClassification`.
fn classify_free_function_call(call_name: &str) -> IoClassification {
    if is_io_call(call_name) {
        IoClassification::Io
    } else {
        IoClassification::NotIo
    }
}

/// Method-call classification (#56 T2). Four rules, in order:
/// 1. Receiver resolved AND its type is a known I/O type AND that type is
///    NOT declared in this same file (a locally-declared `struct Client`
///    shadowing reqwest's `Client` must never assert `Io` by name alone) →
///    `Io`.
/// 2. Receiver resolved and anything else (unknown type, or a known-I/O
///    type name that is actually a local declaration) → `NotIo`.
/// 3. Receiver unresolved AND the method name is on the suspicious list →
///    `Unknown` — enough doubt to withhold `NotIo`, not enough evidence for
///    `Io`.
/// 4. Receiver unresolved and the name gives no reason for suspicion →
///    `NotIo` (bounds abstention noise, C4).
fn classify_method_call(
    receiver: &syn::Expr,
    method_name: &str,
    type_env: &std::collections::HashMap<String, String>,
    locally_declared_types: &HashSet<String>,
) -> IoClassification {
    if let Some(receiver_type) = resolved_receiver_type(receiver, type_env) {
        return if is_io_type(&receiver_type) && !locally_declared_types.contains(&receiver_type) {
            IoClassification::Io
        } else {
            IoClassification::NotIo
        };
    }

    if is_suspicious_method_name(method_name) {
        IoClassification::Unknown
    } else {
        IoClassification::NotIo
    }
}

/// The resolved type name of a bare-identifier receiver, looked up in
/// `type_env` — `None` for anything else (field access, a chained method
/// call, or a bare identifier with no binding at all). Generalizes T1's
/// `receiver_is_io_type`: this returns the type name itself (whether or not
/// it is a known I/O type), so the caller can also check whether that type
/// is a local declaration (rule 1 above).
fn resolved_receiver_type(
    receiver: &syn::Expr,
    type_env: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let syn::Expr::Path(path) = receiver else {
        return None;
    };
    let ident = path.path.get_ident()?;
    type_env.get(&ident.to_string()).cloned()
}

/// Names of every `struct`/`enum` declared anywhere in this file — including
/// inside inline `mod`s, mirroring `collect_functions`'s own recursion (#56
/// T2). Used to guard against a locally-declared type whose name collides
/// with a known I/O type (e.g. a hand-rolled `struct Client`) — the type
/// resolves, but it is provably NOT reqwest's `Client` because this file
/// declares it itself.
fn collect_locally_declared_type_names(items: &[syn::Item]) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in items {
        match item {
            syn::Item::Struct(item_struct) => {
                names.insert(item_struct.ident.to_string());
            }
            syn::Item::Enum(item_enum) => {
                names.insert(item_enum.ident.to_string());
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, sub_items)) = &item_mod.content {
                    names.extend(collect_locally_declared_type_names(sub_items));
                }
            }
            _ => {}
        }
    }
    names
}

/// Unwraps a bounded, syntactic chain of `?` / `.unwrap()` / `.expect(..)` /
/// `.await` around a constructor call, e.g. `File::open(p)?` or
/// `TcpStream::connect(a).await.unwrap()`. Bounded and closed (cc-kiss, C3)
/// — this is not a general inferer, just enough to see through the handful
/// of idiomatic ways a fallible constructor's result reaches a `let`
/// binding.
fn unwrap_result_chain(expr: &syn::Expr) -> &syn::Expr {
    match expr {
        syn::Expr::Try(try_expr) => unwrap_result_chain(&try_expr.expr),
        syn::Expr::Await(await_expr) => unwrap_result_chain(&await_expr.base),
        syn::Expr::MethodCall(method_call)
            if method_call.method == "unwrap" || method_call.method == "expect" =>
        {
            unwrap_result_chain(&method_call.receiver)
        }
        _ => expr,
    }
}

/// The type name asserted by a constructor-shaped call expression — the
/// segment immediately before the final path segment, e.g. `File::open(..)`
/// → `File`, `std::net::TcpStream::connect(..)` → `TcpStream`. `None` when
/// the expression isn't a qualified-path call, or has fewer than two
/// segments (no type to name).
fn constructor_type_name(expr: &syn::Expr) -> Option<String> {
    let syn::Expr::Call(call) = expr else {
        return None;
    };
    let syn::Expr::Path(path) = call.func.as_ref() else {
        return None;
    };
    let segments = &path.path.segments;
    // The `< 2` guard is exactly what makes `segments[len - 2]` below safe:
    // it requires at least a `Type::ctor` pair before indexing one before
    // the last segment ever runs.
    if segments.len() < 2 {
        return None;
    }
    Some(segments[segments.len() - 2].ident.to_string())
}

/// Variable-name → resolved-type-name bindings seeded from a function's
/// typed parameters (#56 T1, C1 resolution step 1). `self`/`&self`
/// (`FnArg::Receiver`) contributes no name to bind. A parameter pattern
/// richer than a bare identifier (destructuring) is skipped — still
/// intra-file and syntactic (C3), not a pattern-matcher.
fn param_type_bindings(sig: &syn::Signature) -> Vec<(String, String)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => {
                let syn::Pat::Ident(ident) = pat_type.pat.as_ref() else {
                    return None;
                };
                type_last_segment(&pat_type.ty).map(|ty| (ident.ident.to_string(), ty))
            }
            syn::FnArg::Receiver(_) => None,
        })
        .collect()
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
    /// Variable-name → resolved-type-name bindings from this function's own
    /// signature (#56 T1) — seeds the body's type environment before a
    /// single statement is visited.
    params: Vec<(String, String)>,
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
                params: param_type_bindings(&func.sig),
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
                        params: param_type_bindings(&method.sig),
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
                            params: param_type_bindings(&method.sig),
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
    branch_arms: u32,
    /// The qualified name of the enclosing `impl`/`trait` type, when this
    /// visitor is walking a method body. Used to resolve `self.m()` and
    /// `Self::m()` to the callee's qualified declaration (D2, #50) — `None`
    /// for a free function, where no such resolution applies.
    enclosing_type: Option<String>,
    /// Variable-name → resolved-type-name, seeded from the function's own
    /// signature params and updated by every `let` binding as it is
    /// visited (#56 T1, C3) — flat, function-body-scoped: a `let` re-using
    /// a name overwrites the earlier entry outright (shadowing-by-overwrite,
    /// a documented limitation, not block-scope restored).
    type_env: std::collections::HashMap<String, String>,
    /// Names of every `struct`/`enum` declared in this same file (#56 T2) —
    /// shared (`Rc`, cheap clone) across every `FunctionVisitor` walking the
    /// same file, including nested `fn` visitors. See `classify_method_call`
    /// rule 1.
    locally_declared_types: Rc<HashSet<String>>,
}

impl FunctionVisitor {
    fn new(
        enclosing_type: Option<String>,
        params: Vec<(String, String)>,
        locally_declared_types: Rc<HashSet<String>>,
    ) -> Self {
        Self {
            enclosing_type,
            type_env: params.into_iter().collect(),
            locally_declared_types,
            ..Self::default()
        }
    }

    /// Records a call — free-function or method — reached at any nesting
    /// level. When nested inside a loop, it is also recorded as a
    /// `LoopCall` fact, classified (not filtered) by `io`: every detector
    /// reading `calls_in_loops` decides for itself which facts it cares
    /// about. `io` is supplied by the caller — a free-function call
    /// classifies by qualified-name prefix (`classify_free_function_call`),
    /// a method call by its receiver's resolved type and the suspicious-name
    /// list (`classify_method_call`, #56 T2); the bare method identifier
    /// recorded here can never itself match a qualified prefix (#56 T1 root
    /// cause).
    fn record_call<S: Spanned>(&mut self, name: String, spanned: &S, io: IoClassification) {
        if self.loop_depth > 0 {
            let line_col = spanned.span().start();
            self.calls_in_loops.push(LoopCall {
                name: name.clone(),
                line: line_col.line,
                col: line_col.column,
                io,
            });
        }
        self.calls.push(name);
    }

    /// Updates the type environment for a `let` binding — annotated
    /// (`let x: T = ..`) or constructor-inferred (`let x = T::ctor(..)`,
    /// unwrapped through `unwrap_result_chain`). A binding whose type
    /// cannot be resolved by either route clears any earlier entry for
    /// that name outright (shadowing-by-overwrite, C3 point 4): the name is
    /// unresolved again until the next binding proves otherwise.
    fn bind_let(&mut self, pat: &syn::Pat, init_expr: &syn::Expr) {
        let (name, resolved_type) = match pat {
            syn::Pat::Type(pat_type) => {
                let syn::Pat::Ident(ident) = pat_type.pat.as_ref() else {
                    return;
                };
                (ident.ident.to_string(), type_last_segment(&pat_type.ty))
            }
            syn::Pat::Ident(ident) => (
                ident.ident.to_string(),
                constructor_type_name(unwrap_result_chain(init_expr)),
            ),
            _ => return,
        };

        match resolved_type {
            Some(ty) => {
                self.type_env.insert(name, ty);
            }
            None => {
                self.type_env.remove(&name);
            }
        }
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
                    self.bind_let(&local.pat, &init.expr);
                }
            }
            syn::Stmt::Item(syn::Item::Fn(func)) => {
                // A nested `fn` cannot capture (or declare) `self`, so it
                // never needs `self`/`Self` resolution — unlike a closure,
                // which shares this same visitor instance and its context.
                // Its own signature still seeds its own type environment
                // (#56 T1) — same rule as any other function body.
                let mut inner = FunctionVisitor::new(
                    None,
                    param_type_bindings(&func.sig),
                    Rc::clone(&self.locally_declared_types),
                );
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
                self.branch_arms = self.branch_arms.max(arm_count);
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
                    let io = classify_free_function_call(&name);
                    self.record_call(name, call.func.as_ref(), io);
                }
                for arg in &call.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::MethodCall(method_call) => {
                let method_name = method_call.method.to_string();
                let io = classify_method_call(
                    &method_call.receiver,
                    &method_name,
                    &self.type_env,
                    &self.locally_declared_types,
                );
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
                self.record_call(name, &method_call.method, io);
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

    // ── Test List (port delta — language()/capabilities(), US16 T2 step E) ──
    //   1. language_is_rust — SynCodeParser::language() == Language::Rust.
    //   2. capabilities_reports_every_metric_supported — all-Supported
    //      (human-approved Q1: T2 constructs no Degraded/Unsupported).

    #[test]
    fn language_is_rust() {
        use codeimpact_hexagon::analysis::Language;
        assert_eq!(SynCodeParser::new().language(), Language::Rust);
    }

    #[test]
    fn capabilities_reports_every_metric_supported() {
        use codeimpact_hexagon::analysis::MetricSupport;
        let capabilities = SynCodeParser::new().capabilities();
        assert_eq!(
            *capabilities.cyclomatic_complexity(),
            MetricSupport::Supported
        );
        assert_eq!(*capabilities.io_in_loops(), MetricSupport::Supported);
        assert_eq!(*capabilities.economic_impact(), MetricSupport::Supported);
        assert_eq!(*capabilities.ecological_impact(), MetricSupport::Supported);
        // T3 (US16, #33): call_graph joined the capability set — Rust stays
        // all-Supported, zero behavior change (same-shape sweep, this test's
        // own assertions already prove the other 4 fields untouched).
        assert_eq!(*capabilities.call_graph(), MetricSupport::Supported);
    }

    // ── Test List (source_guard wiring, #62) ──────────────────────────
    //   1. oversized_source_refused_before_syn_runs — >1 MB →
    //      Err(Unmeasurable(SourceTooLarge)), structurally (no RSS assertion).
    //   2. resolve_dependencies_refused_when_source_too_large — same
    //      guard, mirrored through the resolve_dependencies entry point.
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
    fn resolve_dependencies_refused_when_source_too_large() {
        let source = "a".repeat(1024 * 1024 + 1);
        let parser = parser();
        let ctx = DependencyContext::new(PathBuf::from("x.rs"), PathBuf::from("."), vec![]);
        let result = parser.resolve_dependencies(&source, &ctx);
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

    // ── extract_raw_dependencies tests (US14 L2 — private, syntactic-only:
    // no probe/canary needed, this walks an already-parsed syn::File) ──

    fn tree(source: &str) -> syn::File {
        syn::parse_file(source).expect("test source must be valid Rust")
    }

    #[test]
    fn deps_mod_foo_extracted() {
        let deps = extract_raw_dependencies(&tree("mod foo;"));
        assert_eq!(deps, vec!["mod:foo"]);
    }

    #[test]
    fn deps_mod_with_inline_content_skipped() {
        let deps = extract_raw_dependencies(&tree("mod foo { fn bar() {} }"));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_std_filtered() {
        let deps = extract_raw_dependencies(&tree("use std::collections::HashMap;"));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_core_filtered() {
        let deps = extract_raw_dependencies(&tree("use core::mem;"));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_alloc_filtered() {
        let deps = extract_raw_dependencies(&tree("use alloc::vec;"));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_crate_extracted() {
        let deps = extract_raw_dependencies(&tree("use crate::foo::bar;"));
        assert_eq!(deps, vec!["use:crate::foo::bar"]);
    }

    #[test]
    fn deps_use_super_extracted() {
        let deps = extract_raw_dependencies(&tree("use super::foo::bar;"));
        assert_eq!(deps, vec!["use:super::foo::bar"]);
    }

    #[test]
    fn deps_use_relative_extracted() {
        let deps = extract_raw_dependencies(&tree("use foo::bar::Baz;"));
        assert_eq!(deps, vec!["use:foo::bar::Baz"]);
    }

    #[test]
    fn deps_use_group_expanded() {
        let deps = extract_raw_dependencies(&tree("use foo::{bar, baz};"));
        assert_eq!(deps, vec!["use:foo::bar, baz"]);
    }

    #[test]
    fn deps_empty_source_returns_empty() {
        let deps = extract_raw_dependencies(&tree(""));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_no_mod_or_use_returns_empty() {
        let deps = extract_raw_dependencies(&tree("fn foo() { let x = 1; }"));
        assert!(deps.is_empty());
    }

    #[test]
    fn deps_use_glob() {
        let deps = extract_raw_dependencies(&tree("use foo::*;"));
        assert_eq!(deps, vec!["use:foo::*"]);
    }

    #[test]
    fn parse_use_rename_is_captured() {
        let deps = extract_raw_dependencies(&tree("use foo::bar as baz;\nfn main() {}"));
        assert_eq!(deps, vec!["use:foo::bar"]);
    }

    // ── resolve_dependency tests (US14 L1 — moved from the hexagon's
    // file_consumption_graph: this adapter now owns Rust's module/namespace
    // resolution semantics; same behavior, same cases, new home) ──

    fn ctx(current_file: &str, project_root: &str, available_files: &[&str]) -> DependencyContext {
        DependencyContext::new(
            PathBuf::from(current_file),
            PathBuf::from(project_root),
            available_files.iter().map(PathBuf::from).collect(),
        )
    }

    #[test]
    fn resolve_mod_foo_to_foo_rs() {
        let c = ctx("src/main.rs", "src", &["src/foo.rs"]);
        assert_eq!(
            resolve_dependency("mod:foo", &c),
            Some(PathBuf::from("src/foo.rs"))
        );
    }

    #[test]
    fn resolve_mod_foo_to_foo_mod_rs() {
        let c = ctx("src/main.rs", "src", &["src/foo/mod.rs"]);
        assert_eq!(
            resolve_dependency("mod:foo", &c),
            Some(PathBuf::from("src/foo/mod.rs"))
        );
    }

    #[test]
    fn resolve_mod_foo_not_found() {
        let c = ctx("src/main.rs", "src", &["src/bar.rs"]);
        assert_eq!(resolve_dependency("mod:foo", &c), None);
    }

    #[test]
    fn resolve_use_crate_x_to_x_rs() {
        let c = ctx("src/main.rs", "src", &["src/x.rs"]);
        assert_eq!(
            resolve_dependency("use:crate::x", &c),
            Some(PathBuf::from("src/x.rs"))
        );
    }

    #[test]
    fn resolve_use_crate_x_to_x_mod_rs() {
        let c = ctx("src/main.rs", "src", &["src/x/mod.rs"]);
        assert_eq!(
            resolve_dependency("use:crate::x", &c),
            Some(PathBuf::from("src/x/mod.rs"))
        );
    }

    #[test]
    fn resolve_use_super_x_finds_parent_x() {
        let c = ctx("src/sub/mod.rs", "src", &["src/x.rs"]);
        assert_eq!(
            resolve_dependency("use:super::x", &c),
            Some(PathBuf::from("src/x.rs"))
        );
    }

    #[test]
    fn resolve_use_std_external_returns_none() {
        let c = ctx("src/main.rs", "src", &["src/main.rs"]);
        assert_eq!(
            resolve_dependency("use:std::collections::HashMap", &c),
            None
        );
    }

    #[test]
    fn resolve_use_core_external_returns_none() {
        let c = ctx("src/main.rs", "src", &["src/main.rs"]);
        assert_eq!(resolve_dependency("use:core::mem", &c), None);
    }

    #[test]
    fn resolve_relative_use_finds_file() {
        let c = ctx("src/sub/mod.rs", "src", &["src/sub/foo/bar/Baz.rs"]);
        // Resolves relative to current file's parent: src/sub/foo/bar/Baz.rs
        assert_eq!(
            resolve_dependency("use:foo::bar::Baz", &c),
            Some(PathBuf::from("src/sub/foo/bar/Baz.rs"))
        );
    }

    #[test]
    fn resolve_unknown_dependency_returns_none() {
        let c = ctx("src/main.rs", "src", &["src/main.rs"]);
        assert_eq!(resolve_dependency("unknown:format", &c), None);
    }

    // ── resolve_dependencies tests (US14 — the public CodeParser port
    // method: extraction + resolution wired together through the real
    // canary, proving the two private halves above compose correctly) ──

    #[test]
    fn resolve_dependencies_extracts_and_resolves_mod_declaration() {
        let parser = parser();
        let c = ctx("src/main.rs", "src", &["src/foo.rs"]);
        let resolved = parser.resolve_dependencies("mod foo;", &c).unwrap();
        assert_eq!(resolved, vec![PathBuf::from("src/foo.rs")]);
    }

    #[test]
    fn resolve_dependencies_drops_unresolvable_and_external_deps() {
        let parser = parser();
        let c = ctx("src/main.rs", "src", &["src/main.rs"]);
        let resolved = parser
            .resolve_dependencies("use std::collections::HashMap;\nmod missing;", &c)
            .unwrap();
        assert!(resolved.is_empty());
    }
}
