use codeimpact_hexagon::analysis::IoClassification;

fn is_suspicious_receiver(call_name: &str, suspicious_markers: &[String]) -> bool {
    suspicious_markers
        .iter()
        .any(|marker| call_name.contains(marker.as_str()))
}

/// Call classification shared by every tree-sitter-backed language (US16
/// T4.1/T4.2, generalized US17 T1 retry-free refactor — renamed from
/// `classify_csharp_call`, made language-agnostic by taking
/// `suspicious_markers` as a parameter instead of reading a C#-only
/// constant). `call_name` is the raw source text of a call node's callee
/// field (e.g. `"File.ReadAllText"`, `"_context.Users.Where"`,
/// `"fs.readFile"`, `"list.Add"`) — no type resolution is available for
/// any tree-sitter-backed language (unlike `SynCodeParser`'s `type_env`),
/// so the ONLY assertion this function is entitled to make is a syntactic
/// one (ADR-0016 §1): the receiver's type is proven when — and only when —
/// the call text itself starts with a known confident prefix.
///
/// `confident_prefixes` is the language's own `LanguageProfile::io_table`
/// (base table plus any user-configured additions, US16 T4.3) — always
/// matched by `starts_with`, never `contains` (T4.1 mutation-bite: a call
/// whose text merely CONTAINS a prefix without starting with it must not
/// match). `suspicious_markers` is the language's own
/// `LanguageProfile::suspicious_markers` (US16 T4.2's C# list, US17 Q2's
/// TS/JS list) — text patterns that abstain (`Unknown`) rather than assert
/// (`Io`) on an unprovable receiver. A call matching neither is an honest
/// negative (`NotIo`).
pub fn classify_call(
    call_name: &str,
    confident_prefixes: &[String],
    suspicious_markers: &[String],
) -> IoClassification {
    if confident_prefixes
        .iter()
        .any(|prefix| call_name.starts_with(prefix.as_str()))
    {
        return IoClassification::Io;
    }

    if is_suspicious_receiver(call_name, suspicious_markers) {
        return IoClassification::Unknown;
    }

    IoClassification::NotIo
}
