/// Confident (type-proof) C# I/O prefixes — US16 T4.1, ADR-0016 §1 applied
/// to C#: a call is classified `Io` only when the syntax ITSELF proves the
/// receiver's type, never from a name/variable alone. `File.` and
/// `Directory.` are the two BCL classes whose I/O members are called
/// statically — the class name literally appears in the call text
/// (`File.ReadAllText(...)`), so no type resolution is needed to trust it.
///
/// `HttpClient`/`SqlCommand`/`Stream`/`DbContext` were T2's provisional
/// guess for this table but are normally INSTANCE-typed (`_client.GetAsync
/// (...)`, `_context.Users`) — a literal `HttpClient.` prefix match on an
/// instance receiver would be a name-only assertion ADR-0016 §1 forbids.
/// T4.2's `SUSPICIOUS_RECEIVER_MARKERS` below carries them forward instead,
/// as abstention markers (`Unknown`), never confident `Io` assertions —
/// human-approved Q1.
pub const IO_PREFIXES: &[&str] = &["File.", "Directory."];

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
/// - `_httpClient.`/`_sqlCommand.`/`_stream.`/`_dbContext.` — T2's original
///   (provisional) confident-prefix guesses (`HttpClient`/`SqlCommand`/
///   `Stream`/`DbContext`), demoted here: these BCL types are normally
///   INSTANCE-typed, so a literal prefix match on the TYPE name is a
///   name-only assertion ADR-0016 §1 forbids — abstention, not `Io`. The
///   marker itself is the idiomatic underscore-camelCase FIELD name a real
///   receiver is actually written as (`_httpClient.GetAsync(...)`), not the
///   PascalCase type name — retry #1 (Dev-B BLOCKING): the original
///   PascalCase markers (`"HttpClient."` etc.) never match real C# field
///   receivers at all (case-sensitive `contains`), silently falling through
///   to a fabricated `NotIo` — exactly the ADR-0016 §3 silent-false-
///   negative failure this list exists to prevent.
///
/// A call matching NONE of these AND no confident prefix is an honest
/// negative (`NotIo`) — flooding `Unknown` with every unresolved receiver
/// would drown the signal the same way an unbounded Rust suspicious-name
/// list would (ADR-0016 §3).
///
/// Moved here from `classifier.rs` (US17 T1 refactor step) now that
/// `classify_call` is language-agnostic and takes this list as a
/// parameter instead of reading a hardcoded C#-only constant — same
/// values, same doc, new home.
pub const SUSPICIOUS_RECEIVER_MARKERS: &[&str] = &[
    "_context.",
    "_db.",
    ".AsQueryable(",
    "DbSet",
    "_httpClient.",
    "_sqlCommand.",
    "_stream.",
    "_dbContext.",
];
