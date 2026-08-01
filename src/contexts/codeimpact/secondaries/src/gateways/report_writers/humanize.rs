use codeimpact_hexagon::analysis::BreachedMetric;
use codeimpact_hexagon::analysis::EcologicalImpactEstimator;
use codeimpact_hexagon::analysis::ThresholdReport;

/// Formats a micro-dollar amount as a display string (US7 T2 slice R).
///
/// Extracted from `console_report_writer.rs` (previously a single, already
/// non-duplicated helper) so `html::view_model` can share the exact same
/// formatting instead of carrying its own temporary copy (S1).
pub fn format_dollars(microdollars: f64) -> String {
    let dollars = microdollars / 1_000_000.0;
    if dollars < 0.0001 {
        format!("${:.6}", dollars)
    } else if dollars < 1.0 {
        format!("${:.4}", dollars)
    } else {
        format!("${:.2}", dollars)
    }
}

/// Formats a byte count as a KB/MB display string. Extracted from the
/// branch duplicated verbatim in `write_console_to` and
/// `write_project_report_to` (console_report_writer.rs lines 77-97 / 262-279
/// pre-extraction).
pub fn format_memory(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    if kb >= 1024.0 {
        format!("{:.1} MB", kb / 1024.0)
    } else {
        format!("{:.1} KB", kb)
    }
}

/// Formats a joule count as a J/kJ (+ kWh) display string. Extracted
/// alongside `format_memory` from the same duplicated branch.
pub fn format_energy(joules: f64) -> String {
    let kwh = joules / EcologicalImpactEstimator::KWH_TO_JOULES;
    if joules >= 1000.0 {
        format!("{:.1} kJ ({:.4} kWh)", joules / 1000.0, kwh)
    } else {
        format!("{:.1} J ({:.6} kWh)", joules, kwh)
    }
}

/// Formats a kWh amount as a display string, for the energy threshold
/// (US8, change request on issue #8: energy replaces CPU cost as the
/// gate's first metric). Realistic values are tiny — a single trivial file
/// measures on the order of 6.5e-6 kWh — so the low tier keeps 8 decimals;
/// a project-scale aggregate can climb into the 1e-3+ range, where 4
/// decimals stay readable. Same tiered-precision shape as `format_dollars`.
pub fn format_kwh(kwh: f64) -> String {
    if kwh < 0.001 {
        format!("{:.8} kWh", kwh)
    } else {
        format!("{:.4} kWh", kwh)
    }
}

/// Renders a human-readable threshold-breach warning (US8, AD-3): the ONE
/// shared source of the "which threshold(s), by how much" phrasing —
/// console, JSON's embedded message, HTML's banner and the CLI's `--strict`
/// exit message (main.rs) all call this instead of re-deriving their own
/// text. Returns an empty string when there is nothing to report — callers
/// are expected to only print/embed a non-empty result.
pub fn render_threshold_warning(report: &ThresholdReport) -> String {
    if !report.has_breach() {
        return String::new();
    }
    let mut lines = vec!["=== Alertes de seuils ===".to_string()];
    for breach in report.breaches() {
        lines.push(format!(
            "[SEUIL DÉPASSÉ] {} — limite: {}, mesuré: {}, dépassement: {}",
            breach.metric().label(),
            format_metric_value(breach.metric(), breach.limit()),
            format_metric_value(breach.metric(), breach.actual()),
            format_metric_value(breach.metric(), breach.excess()),
        ));
    }
    lines.join("\n")
}

/// Formats one threshold value (limit/actual/excess) per its metric's own
/// unit — kWh for energy (reusing `format_kwh`), grams for CO2. Shared by
/// `render_threshold_warning` and the HTML view-model, which needs the
/// same per-value formatting for its structured banner (AD-3).
pub fn format_metric_value(metric: BreachedMetric, value: f64) -> String {
    match metric {
        BreachedMetric::Energy => format_kwh(value),
        BreachedMetric::Co2 => format!("{:.1} g", value),
    }
}

/// Neutralizes control characters before a value derived from analyzed
/// SOURCE CODE reaches a terminal (US17 T1 retry, Security MEDIUM,
/// CWE-117/CWE-150). Until the TS/JS tree-sitter adapter landed, every
/// producer of a `ParsedFunction`/`FunctionDetail` name was a Rust or C#
/// identifier — a closed character set with no control bytes possible. A
/// JS/TS `method_definition` key can be an arbitrary STRING LITERAL, so a
/// hostile name can now carry raw ANSI escape sequences (`ESC[2J` clears
/// the screen, `ESC[1;31m` recolors) and forge or hide what the operator
/// reads in `console_report_writer.rs` — under this tool's threat model
/// the report IS the product.
///
/// Every Unicode control character (`char::is_control` — C0 0x00-0x1F, DEL
/// 0x7F, C1 0x80-0x9F) is replaced by its `\xHH` textual escape: visible
/// and forensic (the operator can still see WHAT was there) but never
/// interpreted by the terminal. Every other character, including non-ASCII
/// UTF-8, passes through untouched — EXCEPT the widened class below.
///
/// Retry 2 (BLOCKING 2, Dev-B + Security convergent): `char::is_control`
/// covers Unicode category Cc only. Bidi-override FORMATTING characters
/// (category Cf) are a different category entirely and pass through
/// untouched by that check alone — yet U+202E (RIGHT-TO-LEFT OVERRIDE) is
/// the exact "Trojan Source" primitive (CVE-2021-42574): it visually
/// reorders every character after it on the same terminal line, the same
/// "forge what the operator reads" threat the ESC-sequence fix already
/// closed for Cc. `is_neutralized_char` below additionally catches the
/// full bidi-control set (`U+200E`/`U+200F`, `U+202A`-`U+202E`,
/// `U+2066`-`U+2069`, `U+061C`) and the line/paragraph separators
/// (`U+2028`/`U+2029`, which can forge extra report lines much like a raw
/// newline). Ruling D2 is respected: the STRATEGY is unchanged (escape,
/// don't truncate; console-writer only; `field_text`/JSON keep the real
/// name) — only the character CLASS is widened.
///
/// **Console-writer only.** The JSON writer already escapes control
/// characters (`serde_json`'s own string encoding) and the HTML writer's
/// `json_island_escape` (`html_report_writer.rs`) plus its `textContent`
/// -only renderer already close the `<script>`-breakout class for JS's
/// much wider character set — this function must NEVER be applied there,
/// nor inside `field_text` (`tree_sitter_code_parser.rs`): a downstream
/// tool consuming the JSON payload needs the REAL symbol name, unmodified.
pub fn sanitize_console_text(input: &str) -> String {
    let mut sanitized = String::with_capacity(input.len());
    for c in input.chars() {
        if is_neutralized_char(c) {
            sanitized.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            sanitized.push(c);
        }
    }
    sanitized
}

/// Whether `c` must be neutralized before reaching a terminal (retry 2,
/// BLOCKING 2): the original Cc class (`char::is_control`) PLUS the bidi
/// Cf controls and the two Unicode line/paragraph separators — see
/// `sanitize_console_text`'s doc for the full threat rationale.
fn is_neutralized_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{2028}'
                | '\u{2029}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List (sanitize_console_text — US17 T1 retry, Security MEDIUM):
    // 1. an ANSI escape sequence (ESC[2J) is neutralized to a visible,
    //    non-interpretable \xHH escape — the exploit shape itself.
    // 2. plain ASCII text passes through unchanged.
    // 3. non-ASCII UTF-8 (accented / non-Latin) passes through unchanged —
    //    only CONTROL characters are touched, not "anything non-ASCII".
    // 4. newline/tab are ALSO neutralized (line-forging is the same class
    //    of attack as color/clear — not just the ESC byte itself).
    // 5. the empty string is untouched (vacuous case).

    #[test]
    fn sanitize_console_text_neutralizes_an_ansi_escape_sequence() {
        assert_eq!(
            sanitize_console_text("\x1b[2J\x1b[1;31mCRITICAL\x1b[0m"),
            "\\x1b[2J\\x1b[1;31mCRITICAL\\x1b[0m"
        );
    }

    #[test]
    fn sanitize_console_text_leaves_plain_ascii_untouched() {
        assert_eq!(sanitize_console_text("compute"), "compute");
    }

    #[test]
    fn sanitize_console_text_leaves_non_ascii_utf8_untouched() {
        assert_eq!(
            sanitize_console_text("café_naïve_日本語"),
            "café_naïve_日本語"
        );
    }

    #[test]
    fn sanitize_console_text_neutralizes_newline_and_tab() {
        assert_eq!(sanitize_console_text("a\nb\tc"), "a\\x0ab\\x09c");
    }

    #[test]
    fn sanitize_console_text_of_empty_string_is_empty() {
        assert_eq!(sanitize_console_text(""), "");
    }

    // Retry 2 (BLOCKING 2, Dev-B + Security convergent) — `char::is_control`
    // is Unicode category Cc ONLY (C0/DEL/C1). Bidi-override formatting
    // characters (Cf) are a DIFFERENT category and pass through untouched
    // by that check alone — but U+202E (RIGHT-TO-LEFT OVERRIDE) is the
    // exact "Trojan Source" (CVE-2021-42574) primitive: it visually
    // reorders everything after it on the same terminal line, the same
    // "forge what the operator reads" class as the ESC payload BLOCKING 2
    // (round 1) already closed for Cc. Line/paragraph separators
    // (U+2028/U+2029) are not Cc either and can forge extra report lines.
    //
    // Test List: 6. U+202E (RLO) is neutralized. 7. every other bidi
    // control in the widened class (U+200E/U+200F, U+202A-202D, U+2066-
    // 2069, U+061C) is neutralized. 8. U+2028/U+2029 (line/paragraph
    // separator) are neutralized. 9. the original Cc class (ESC) still
    // works — the widening must not regress round 1.

    #[test]
    fn sanitize_console_text_neutralizes_right_to_left_override() {
        let input = "safe\u{202E}evil";
        let output = sanitize_console_text(input);
        assert!(
            !output.contains('\u{202E}'),
            "U+202E (Trojan Source RLO) must be neutralized, got: {:?}",
            output
        );
        assert_eq!(output, "safe\\x202eevil");
    }

    #[test]
    fn sanitize_console_text_neutralizes_every_bidi_control_in_the_widened_class() {
        for bidi in [
            '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
            '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{061C}',
        ] {
            let output = sanitize_console_text(&format!("a{}b", bidi));
            assert!(
                !output.contains(bidi),
                "bidi control {:?} (U+{:04X}) must be neutralized, got: {:?}",
                bidi,
                bidi as u32,
                output
            );
        }
    }

    #[test]
    fn sanitize_console_text_neutralizes_line_and_paragraph_separators() {
        for separator in ['\u{2028}', '\u{2029}'] {
            let output = sanitize_console_text(&format!("a{}b", separator));
            assert!(
                !output.contains(separator),
                "separator {:?} must be neutralized, got: {:?}",
                separator,
                output
            );
        }
    }

    #[test]
    fn sanitize_console_text_still_neutralizes_the_original_cc_class() {
        // Round-1 regression guard: widening the class must not narrow it.
        assert_eq!(sanitize_console_text("\x1b[2J"), "\\x1b[2J");
    }

    // Test List (format_dollars):
    // 1. amount < $0.0001 -> 6 decimals
    // 2. amount exactly at the $0.0001 boundary -> NOT the 6-decimal branch (4 decimals)
    // 3. amount between $0.0001 and $1 -> 4 decimals
    // 4. amount exactly at the $1 boundary -> NOT the 4-decimal branch (2 decimals)
    // 5. amount >= $1 -> 2 decimals

    #[test]
    fn format_dollars_below_one_ten_thousandth_uses_six_decimals() {
        assert_eq!(format_dollars(50.0), "$0.000050");
    }

    #[test]
    fn format_dollars_at_the_six_decimal_boundary_uses_four_decimals() {
        // 100 microdollars == exactly $0.0001: `< 0.0001` is false at the boundary.
        assert_eq!(format_dollars(100.0), "$0.0001");
    }

    #[test]
    fn format_dollars_between_boundaries_uses_four_decimals() {
        assert_eq!(format_dollars(123_400.0), "$0.1234");
    }

    #[test]
    fn format_dollars_at_the_four_decimal_boundary_uses_two_decimals() {
        // 1_000_000 microdollars == exactly $1: `< 1.0` is false at the boundary.
        assert_eq!(format_dollars(1_000_000.0), "$1.00");
    }

    #[test]
    fn format_dollars_at_or_above_one_uses_two_decimals() {
        assert_eq!(format_dollars(2_500_000.0), "$2.50");
    }

    // Test List (format_memory):
    // 1. small byte count -> KB
    // 2. exactly at the 1024 KB boundary -> MB (not KB)
    // 3. large byte count -> MB

    #[test]
    fn format_memory_below_one_mb_uses_kb() {
        assert_eq!(format_memory(2048), "2.0 KB");
    }

    #[test]
    fn format_memory_at_the_mb_boundary_uses_mb() {
        // 1024 * 1024 bytes == exactly 1024 KB: `>= 1024.0` is true at the boundary.
        assert_eq!(format_memory(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn format_memory_above_one_mb_uses_mb() {
        assert_eq!(format_memory(3 * 1024 * 1024), "3.0 MB");
    }

    // Test List (format_energy):
    // 1. small joule count -> J (6-decimal kWh)
    // 2. exactly at the 1000 J boundary -> kJ (4-decimal kWh), not J
    // 3. large joule count -> kJ

    #[test]
    fn format_energy_below_one_kj_uses_joules() {
        let kwh = 500.0 / EcologicalImpactEstimator::KWH_TO_JOULES;
        assert_eq!(format_energy(500.0), format!("500.0 J ({:.6} kWh)", kwh));
    }

    #[test]
    fn format_energy_at_the_kj_boundary_uses_kilojoules() {
        // 1000 J is exactly the boundary: `>= 1000.0` is true at the boundary.
        let kwh = 1000.0 / EcologicalImpactEstimator::KWH_TO_JOULES;
        assert_eq!(format_energy(1000.0), format!("1.0 kJ ({:.4} kWh)", kwh));
    }

    #[test]
    fn format_energy_above_one_kj_uses_kilojoules() {
        let kwh = 12_300.0 / EcologicalImpactEstimator::KWH_TO_JOULES;
        assert_eq!(format_energy(12_300.0), format!("12.3 kJ ({:.4} kWh)", kwh));
    }

    // Test List (format_kwh) — US8 change request (issue #8): energy
    // replaces CPU cost as the gate's first metric.
    // 1. a realistic tiny value (a single file's real measured energy,
    //    6.5e-6 kWh from sample.rs) -> 8 decimals
    // 2. exactly at the 0.001 kWh boundary -> NOT the 8-decimal branch (4 decimals)
    // 3. a project-scale value above the boundary -> 4 decimals

    #[test]
    fn format_kwh_realistic_tiny_value_uses_eight_decimals() {
        assert_eq!(format_kwh(0.0000065), "0.00000650 kWh");
    }

    #[test]
    fn format_kwh_at_the_boundary_uses_four_decimals() {
        // 0.001 kWh is exactly the boundary: `< 0.001` is false at the boundary.
        assert_eq!(format_kwh(0.001), "0.0010 kWh");
    }

    #[test]
    fn format_kwh_project_scale_value_uses_four_decimals() {
        assert_eq!(format_kwh(0.0228), "0.0228 kWh");
    }
}
