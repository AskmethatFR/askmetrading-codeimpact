/// ADR-8.10 test-only structural gate, hardened (#28).
///
/// `rendered_js_contains_no_html_sink` / `rendered_js_has_only_two_style_sinks`
/// (`html_report_writer_test.rs`) originally matched LITERAL substrings
/// only, so a sink reached through string-literal concatenation
/// (`"inner" + "HTML"`) or bracket-notation / bulk-setter property access
/// (`node["style"][computedProp]`, `Object.assign(node.style, …)`, a bare
/// `node.style = …`) never tripped them — demonstrated by the Security audit
/// on #27. Extracted here so the "real JS must pass" tests and the "a known
/// bypass must fail" tests (`html_renderer_gate_hardening_test.rs`) share a
/// single implementation — the two families can never silently drift apart.
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

/// ADR-8.10 rule 4 — total ban on HTML/script sinks, hardened (#28) to also
/// catch a banned identifier reconstructed via string-literal concatenation.
/// `Object.assign(` (a bulk property setter that can write `innerHTML`/
/// `cssText` in one call, sidestepping a single-property literal scan) is
/// banned outright alongside the identifier list. Returns one message per
/// violation; an empty `Vec` means the source is clean.
pub fn html_sink_violations(source: &str) -> Vec<String> {
    let mut violations: Vec<String> = BANNED_HTML_SINK_TOKENS
        .iter()
        .filter(|token| source.contains(*token))
        .map(|token| format!("banned sink '{token}' found as a literal substring"))
        .collect();

    if source.contains("Object.assign(") {
        violations.push("banned bulk property setter 'Object.assign(' found".to_string());
    }

    for joined in concatenated_string_literals(source) {
        for token in BANNED_HTML_SINK_TOKENS {
            if joined.contains(token) {
                violations.push(format!(
                    "banned sink '{token}' built via string-literal concatenation (joined value: \"{joined}\")"
                ));
            }
        }
    }

    violations
}

const LEGITIMATE_DOTTED_STYLE_SINKS: [&str; 2] = [".style.width", ".style.paddingLeft"];

/// ADR-8.10 rule 3 — exactly two clamped numeric `.style` sinks. Hardened
/// (#28) against bracket-notation (`.style[`, `["style"]`, `['style']`),
/// `Object.assign` bulk-set, and a bare `.style =` reassignment of the whole
/// style object/string — none of which the dotted-property literal count
/// alone would catch. Returns one message per violation; an empty `Vec`
/// means the source is clean.
pub fn style_sink_violations(source: &str) -> Vec<String> {
    let mut violations = Vec::new();

    let dotted_total = source.matches(".style.").count();
    let accounted_for: usize = LEGITIMATE_DOTTED_STYLE_SINKS
        .iter()
        .map(|sink| source.matches(sink).count())
        .sum();
    if dotted_total != accounted_for {
        violations.push(format!(
            "expected only the two clamped `.style.` sinks {LEGITIMATE_DOTTED_STYLE_SINKS:?}, found {dotted_total} total `.style.` accesses"
        ));
    }

    for pattern in [".style[", "[\"style\"]", "['style']"] {
        if source.contains(pattern) {
            violations.push(format!(
                "banned bracket-notation style access '{pattern}' found"
            ));
        }
    }

    if source.contains("Object.assign(") {
        violations.push("banned bulk property setter 'Object.assign(' found".to_string());
    }

    if bare_style_assignment(source) {
        violations.push(
            "banned bare `.style =` reassignment found (bypasses the two-sink budget)".to_string(),
        );
    }

    violations
}

/// True when `.style` appears followed by `=` (not `==`) with no `.` or `[`
/// in between — i.e. the whole style object/string is reassigned, rather
/// than one clamped numeric property being set (`.style.width = …`, which
/// continues with `.`, is correctly left alone).
fn bare_style_assignment(source: &str) -> bool {
    let needle = ".style";
    let mut search_from = 0usize;
    while let Some(offset) = source[search_from..].find(needle) {
        let match_start = search_from + offset;
        let after = source[match_start + needle.len()..].trim_start();
        let continues_as_property_or_index = after.starts_with('.') || after.starts_with('[');
        if !continues_as_property_or_index && after.starts_with('=') && !after.starts_with("==") {
            return true;
        }
        search_from = match_start + needle.len();
    }
    false
}

/// Finds maximal chains of `"lit1" + "lit2" (+ "lit3")*` and returns each
/// chain's joined content — the shape a "banned identifier reconstructed via
/// concatenation" bypass produces. A lone literal (no `+` to another
/// literal) is not a chain and is not returned: literal presence alone is
/// not the obfuscation under test. Operates on `char`s (not bytes) so
/// non-ASCII content elsewhere in the source (e.g. the renderer's own "→ "
/// arrow) can never desync the scan.
fn concatenated_string_literals(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let mut chains = Vec::new();

    while i < chars.len() {
        if chars[i] == '"' {
            let (first, mut cursor) = read_string_literal(&chars, i);
            let mut chain = vec![first];
            loop {
                let after_ws = skip_ws(&chars, cursor);
                if chars.get(after_ws) != Some(&'+') {
                    break;
                }
                let after_plus = skip_ws(&chars, after_ws + 1);
                if chars.get(after_plus) != Some(&'"') {
                    break;
                }
                let (next_literal, next_cursor) = read_string_literal(&chars, after_plus);
                chain.push(next_literal);
                cursor = next_cursor;
            }
            if chain.len() > 1 {
                chains.push(chain.concat());
            }
            i = cursor;
        } else {
            i += 1;
        }
    }

    chains
}

fn skip_ws(chars: &[char], mut idx: usize) -> usize {
    while idx < chars.len() && chars[idx].is_whitespace() {
        idx += 1;
    }
    idx
}

/// Reads a double-quoted JS string literal starting at `start` (the opening
/// `"`), honoring `\"` escapes. Returns the literal's raw content (escapes
/// left as-is — callers only ever substring-match it against ASCII banned
/// tokens, so de-escaping would add complexity with no observable effect)
/// and the index just past the closing quote.
fn read_string_literal(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut content = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                content.push(chars[i]);
                content.push(chars[i + 1]);
                i += 2;
            }
            '"' => {
                i += 1;
                break;
            }
            c => {
                content.push(c);
                i += 1;
            }
        }
    }
    (content, i)
}
