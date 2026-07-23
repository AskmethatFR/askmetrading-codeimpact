/// ADR-8.10 test-only structural gate.
///
/// `rendered_js_contains_no_html_sink` / `rendered_js_has_only_two_style_sinks`
/// (`html_report_writer_test.rs`) originally inlined this literal-substring
/// matching directly in each test. Extracted here (#28) so the "real JS must
/// pass" tests and the new "a known bypass must fail" tests
/// (`html_renderer_gate_hardening_test.rs`) share a single implementation —
/// the two families can never silently drift apart.
///
/// Lives in the integration test crate, not `secondaries`: this is a
/// build-time verification tool exercised only from `tests/*.rs`, never
/// shipped or called at runtime (ca-layering — a test concern does not
/// belong in a production adapter crate).
const BANNED_HTML_SINK_TOKENS: [&str; 10] = [
    "innerHTML",
    "outerHTML",
    "insertAdjacentHTML",
    "document.write",
    "setAttribute",
    "eval(",
    "new Function",
    "javascript:",
    "srcdoc",
    "cssText",
];

/// ADR-8.10 rule 4 — total ban on HTML/script sinks. Returns one message per
/// violation; an empty `Vec` means the source is clean.
pub fn html_sink_violations(source: &str) -> Vec<String> {
    BANNED_HTML_SINK_TOKENS
        .iter()
        .filter(|token| source.contains(*token))
        .map(|token| format!("banned sink '{token}' found as a literal substring"))
        .collect()
}

const LEGITIMATE_DOTTED_STYLE_SINKS: [&str; 2] = [".style.width", ".style.paddingLeft"];

/// ADR-8.10 rule 3 — exactly two clamped numeric `.style` sinks. Returns one
/// message per violation; an empty `Vec` means the source is clean.
pub fn style_sink_violations(source: &str) -> Vec<String> {
    let dotted_total = source.matches(".style.").count();
    let accounted_for: usize = LEGITIMATE_DOTTED_STYLE_SINKS
        .iter()
        .map(|sink| source.matches(sink).count())
        .sum();

    if dotted_total == accounted_for {
        Vec::new()
    } else {
        vec![format!(
            "expected only the two clamped `.style.` sinks {LEGITIMATE_DOTTED_STYLE_SINKS:?}, found {dotted_total} total `.style.` accesses"
        )]
    }
}
