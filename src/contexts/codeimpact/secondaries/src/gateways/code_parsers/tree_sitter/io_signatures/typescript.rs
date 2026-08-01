/// Confident (type-proof) TypeScript/JavaScript I/O prefixes (US17 T1,
/// human-approved Q2, ADR-0016 §1 applied to TS/JS: a call is classified
/// `Io` only when the syntax ITSELF proves the receiver's module, never
/// from a name/variable alone). `fs.`/`fsPromises.` are Node's own I/O
/// module namespaces, called via a literal, statically-named import
/// (`import * as fs from 'fs'`, `fs.readFileSync(...)`) — the module name
/// literally appears in the call text, so no type resolution is needed to
/// trust it. `axios.` is the de-facto standard HTTP client, called the
/// same statically-qualified way (`axios.get(url)`).
///
/// No calibration corpus was measured for TS/JS — unlike
/// `io_signatures::csharp`'s eShopOnWeb run (see `csharp.rs`'s doc
/// comment), no reference project was walked before freezing this table.
/// This list is frozen-then-to-be-measured: a future architect-scoped
/// calibration (mirroring ADR-0022's C# follow-up) should run this
/// classifier against a real TS/JS corpus and report precision, the same
/// way T4.4 did for C#.
pub const IO_PREFIXES: &[&str] = &["fs.", "fsPromises.", "axios."];

/// Markers suspicious enough that an unproven TS/JS receiver is reported
/// `Unknown` rather than a fabricated `NotIo` (US17 T1, human-approved Q2,
/// mirrors C#'s `SUSPICIOUS_RECEIVER_MARKERS` — ADR-0016 §3 split). TS/JS
/// has no `type_env`-style receiver resolution any more than C# does, so
/// these markers name text patterns that commonly appear in the raw call
/// text of an unprovable I/O-shaped call:
///
/// - `fetch` — the WHATWG fetch API, a bare global function call with no
///   qualifying receiver to prove — never a confident prefix match.
/// - `readFile`/`writeFile` — Node `fs` members called WITHOUT the `fs.`/
///   `fsPromises.` qualifier this table's confident half requires (e.g.
///   destructured `import { readFile } from 'fs/promises'`).
/// - `XMLHttpRequest` — the legacy browser HTTP API, always instantiated
///   (`new XMLHttpRequest()`) rather than called through a static module
///   qualifier.
/// - `prisma.` / `knex(` — ORM/query-builder markers, the TS/JS analog of
///   C#'s EF Core `DbSet`/`AsQueryable` abstention markers (the N+1
///   query-in-loop shape).
///
/// `await import(...)` is deliberately ABSENT (human-approved Q2, deferred
/// to issue #120): it parses as an `import_expression`, not a
/// `call_expression`, so `ecmascript.scm`'s `@call` capture never sees it —
/// an entry for it here could never fire, so it is not written down as one
/// (cc-yagni: no code for a case the query cannot reach).
///
/// A call matching NONE of these AND no confident prefix is an honest
/// negative (`NotIo`) — flooding `Unknown` with every unresolved receiver
/// would drown the signal the same way an unbounded suspicious-name list
/// would (ADR-0016 §3).
pub const SUSPICIOUS_RECEIVER_MARKERS: &[&str] = &[
    "fetch",
    "readFile",
    "writeFile",
    "XMLHttpRequest",
    "prisma.",
    "knex(",
    // Retry (Security MEDIUM #2, fold-in 6): network/process markers were
    // absent from BOTH tables, so `http.get`/`cp.execSync`/etc. landed in
    // `NotIo` — an ASSERTED negative, not an abstention — silently
    // dropping real I/O from "Appels en boucle non classifiables". Never
    // promoted to the confident table above: none of these prove the
    // receiver's type syntactically (ADR-0016 §1), so abstention
    // (`Unknown`), not assertion, is the only honest call.
    "http.",
    "https.",
    "net.",
    "dns.",
    "child_process",
    "exec",
    "spawn",
];
