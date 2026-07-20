/// C# I/O signature prefixes — a T4 seam (US16 T2 scope note): NOT consumed
/// by T2's metric computation. `TreeSitterCodeParser` classifies every
/// call-in-loop as `IoClassification::Unknown` in this slice (an honest
/// abstention, ADR-0010 — never a fabricated `NotIo`), the same way #56 T1
/// treated an unresolved Rust receiver before its own classifier existed.
/// Real I/O detection for C# (matching this table against a resolved
/// receiver/call type) is T4's job.
#[allow(dead_code)]
pub const IO_PREFIXES: &[&str] = &[
    "File.",
    "Directory.",
    "HttpClient.",
    "SqlCommand.",
    "Stream.",
    "DbContext.",
];
