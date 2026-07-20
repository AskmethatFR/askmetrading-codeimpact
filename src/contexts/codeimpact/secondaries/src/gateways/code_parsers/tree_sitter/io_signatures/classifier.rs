use codeimpact_hexagon::analysis::IoClassification;

/// T4.4 calibration (freeze-then-measure, ADR-0016 §4's discipline applied
/// to C#): the name/marker list below was frozen BEFORE this measurement,
/// then run — via the built `codeimpact` CLI, `analyze --format console`
/// (no dedicated harness needed, unlike ADR-0016's Rust calibration: the
/// CLI already IS the real pipeline) — against `dotnet-architecture/
/// eShopOnWeb` (shallow clone, 2026-07-20), a canonical EF Core reference
/// app with a real `CatalogContext : DbContext` and repository layer.
///
/// | Corpus | Files | `Io` | `Unknown` | False `Io` |
/// |---|---|---|---|---|
/// | eShopOnWeb `src/` | 209 (0 unmeasurable) | 0 | 0 | n/a (0 hits) |
///
/// Honest finding, not a defect: this corpus has only 10 files containing
/// `foreach` at all, and manual inspection of each loop body (e.g.
/// `BasketService.UpdateQuantities`) found no I/O call — static (`File.`/
/// `Directory.`) or EF/instance-shaped — nested inside any of them. The
/// project's repository queries are built OUTSIDE loops (idiomatic EF
/// Core), so this corpus exercises neither the confident-prefix path nor
/// the abstention markers. Zero measured false positives is therefore a
/// true negative on an empty sample, not evidence the classifier is
/// correct on a positive case — a corpus with denser loop+query nesting
/// (or the N+1 pattern the human ruling named) would be needed to measure
/// the `Unknown` marker list's precision. **Decision: no pruning** — freeze
/// as specified in the tech spec; a future architect-scoped calibration
/// (ADR-0022) should target a corpus with actual loop+I/O density.
///
/// Markers suspicious enough that an unproven C# receiver is reported
/// `Unknown` rather than a fabricated `NotIo` (US16 T4.2, human-approved
/// Q1, mirrors `SynCodeParser`'s `SUSPICIOUS_METHOD_NAMES` — ADR-0016 §3).
/// C# has no `type_env`-style receiver resolution, so — unlike Rust's
/// method-name-only heuristic — these markers name text patterns that
/// commonly appear in the raw call text of an unprovable receiver:
///
/// - `_context.`/`_db.` — the two overwhelmingly common EF Core `DbContext`
///   field names.
/// - `.AsQueryable(` / `DbSet` — EF Core query-surface markers (the N+1
///   `IQueryable`-in-`foreach` case named in the human ruling).
/// - `HttpClient.`/`SqlCommand.`/`Stream.`/`DbContext.` — T2's original
///   (provisional) confident-prefix guesses, demoted here: these BCL types
///   are normally INSTANCE-typed, so a literal prefix match on them is a
///   name-only assertion ADR-0016 §1 forbids — abstention, not `Io`.
///
/// A call matching NONE of these AND no confident prefix is an honest
/// negative (`NotIo`) — flooding `Unknown` with every unresolved receiver
/// would drown the signal the same way an unbounded Rust suspicious-name
/// list would (ADR-0016 §3).
const SUSPICIOUS_RECEIVER_MARKERS: &[&str] = &[
    "_context.",
    "_db.",
    ".AsQueryable(",
    "DbSet",
    "HttpClient.",
    "SqlCommand.",
    "Stream.",
    "DbContext.",
];

fn is_suspicious_receiver(call_name: &str) -> bool {
    SUSPICIOUS_RECEIVER_MARKERS
        .iter()
        .any(|marker| call_name.contains(marker))
}

/// C# call classification (US16 T4.1/T4.2) — the real classifier that replaces
/// T2's hardcoded `IoClassification::Unknown` seam. `call_name` is the raw
/// source text of an `invocation_expression`'s `function` field (e.g.
/// `"File.ReadAllText"`, `"_context.Users.Where"`, `"list.Add"`) — no type
/// resolution is available for C# (unlike `SynCodeParser`'s `type_env`), so
/// the ONLY assertion this function is entitled to make is a syntactic one
/// (ADR-0016 §1): the receiver's type is proven when — and only when — the
/// call text itself starts with a known static-class prefix.
///
/// `confident_prefixes` is `TreeSitterCodeParser`'s own `File.`/`Directory.`
/// base (`io_signatures::csharp::IO_PREFIXES`) plus any user-configured
/// additions (US16 T4.3, `.codeimpact.json`'s `ioSignatures` key) — always
/// matched by `starts_with`, never `contains` (T4.1 mutation-bite: a call
/// whose text merely CONTAINS a prefix without starting with it must not
/// match).
pub fn classify_csharp_call(call_name: &str, confident_prefixes: &[String]) -> IoClassification {
    if confident_prefixes
        .iter()
        .any(|prefix| call_name.starts_with(prefix.as_str()))
    {
        return IoClassification::Io;
    }

    if is_suspicious_receiver(call_name) {
        return IoClassification::Unknown;
    }

    IoClassification::NotIo
}
