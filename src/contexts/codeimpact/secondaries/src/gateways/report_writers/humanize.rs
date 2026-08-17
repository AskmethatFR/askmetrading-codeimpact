use codeimpact_hexagon::analysis::BreachedMetric;
use codeimpact_hexagon::analysis::EcologicalImpactEstimator;
use codeimpact_hexagon::analysis::GateCoverage;
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

/// Renders a human-readable warning naming why the gate that decides
/// `--strict`'s exit code (US128, issue #128) could not apply in full —
/// either some files went unmeasured (`Partial`) or the run's single
/// measurement itself was never taken (`Absent`, ADR-0032 AD-5: an absence
/// is named as an absence, never disguised as a count). Sibling of
/// `render_threshold_warning` (AD-3) — same "one shared renderer" shape,
/// stderr-only. Returns an empty string for `GateCoverage::Complete` —
/// callers are expected to only print a non-empty result. Never emits a raw
/// file path (ADR-0006/#132 discipline) — only a count, the full list
/// already lives in the report's own `unmeasurable_files` section.
pub fn render_incomplete_coverage_warning(coverage: GateCoverage) -> String {
    match coverage {
        GateCoverage::Complete => String::new(),
        GateCoverage::Partial { unmeasurable_files } => format!(
            "=== Couverture du seuil incomplète ===\n\
             [SEUIL NON ÉVALUABLE EN TOTALITÉ] {} fichier(s) n'ont pas pu être mesuré(s) — \
             le seuil n'a donc pas pu s'appliquer à l'ensemble du projet. Consultez la liste \
             des fichiers non mesurés dans le rapport.",
            unmeasurable_files
        ),
        GateCoverage::Absent => "=== Couverture du seuil incomplète ===\n\
             [SEUIL NON ÉVALUABLE] la mesure n'a pas pu être prise — aucun résultat \
             exploitable pour évaluer le seuil."
            .to_string(),
    }
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
/// 0x7F, C1 0x80-0x9F) is replaced by a brace-delimited `\u{HH}` textual
/// escape: visible and forensic (the operator can still see WHAT was
/// there) but never interpreted by the terminal. Every other character,
/// including non-ASCII UTF-8, passes through untouched — EXCEPT the
/// widened class below, and a literal backslash (see the injectivity
/// paragraph).
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
/// **Sweep (Dev-B MINOR A, Security LOW, both lanes) — the escape is
/// INJECTIVE.** Two distinct source problems made two DIFFERENT source
/// names render byte-identically before this: (1) a literal backslash was
/// never itself escaped, so a source text that literally spelled out an
/// escape marker (four printable characters: `\`, `x`, `1`, `b`) rendered
/// the same as a REAL control byte escaped by this function; (2) the
/// prior `\xHH` form used `{:02x}` — a MINIMUM width, not a fixed one —
/// so a wide codepoint like U+202E (`\x202e`, four hex digits) collided
/// with a literal `\x20` immediately followed by the plain text `2e`. Both
/// are closed together: a literal `\` is escaped to `\\` (so a single
/// un-doubled `\` in the output can only ever be this function's own
/// marker, never raw input text), and the escape itself is the
/// brace-delimited `\u{HH}` form — the closing `}` makes the boundary
/// between the escape and any following text unambiguous regardless of
/// how many hex digits the codepoint needs.
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
        if c == '\\' {
            sanitized.push_str("\\\\");
        } else if is_neutralized_char(c) {
            sanitized.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            sanitized.push(c);
        }
    }
    sanitized
}

/// Whether `c` must be neutralized before reaching a terminal (retry 2,
/// BLOCKING 2; widened, sweep, Dev-B MINOR B + Security MEDIUM): the
/// original Cc class (`char::is_control`) PLUS `is_cf` (category Cf,
/// FORMAT characters) PLUS the two Unicode line/paragraph separators —
/// see `sanitize_console_text`'s doc for the full threat rationale.
fn is_neutralized_char(c: char) -> bool {
    c.is_control() || is_cf(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Unicode general category Cf (FORMAT) — characters with no visible glyph
/// of their own that alter the presentation of surrounding text (bidi
/// overrides/isolates, zero-width joiners, the BOM, soft hyphen, the tag
/// block). Sweep (Dev-B MINOR B, Security MEDIUM, both lanes, independent
/// convergence): round 1 enumerated only the BIDI subset of this category
/// (`U+200E`/`U+200F`, `U+202A`-`U+202E`, `U+2066`-`U+2069`, `U+061C`) —
/// real, but not the whole category. Security verified in real project
/// output that U+200B (ZERO WIDTH SPACE), U+FEFF (ZERO WIDTH NO-BREAK
/// SPACE / BOM) and U+00AD (SOFT HYPHEN) each reached the terminal raw,
/// making two DIFFERENT function names render as the SAME visible report
/// line — the "forge what the operator reads" class, without the
/// reordering/control-sequence teeth of RLO/ESC but just as able to make
/// two distinct functions indistinguishable. A category PREDICATE (rather
/// than one enumerated arm per newly-found codepoint) closes the whole
/// class at once: U+200C (ZWNJ), U+200D (ZWJ), U+2060 (WORD JOINER) and
/// the U+E0000-U+E007F tag block Dev-B additionally listed are Cf too,
/// and need no separate arm here.
///
/// Neither lane asked for full Unicode-category-database precision here
/// (rustc's own std has no `char::is_format`, and pulling in a Unicode
/// database crate for one predicate was judged out of proportion for
/// this sweep — see cc-yagni/cc-kiss) — this is the practically-relevant
/// Cf set: every codepoint either lane named, plus the two adjacent
/// invisible-math-operator/interlinear-annotation ranges that share the
/// exact same "zero-width, alters presentation" shape as the ones named.
/// Both lanes note explicitly that even a COMPLETE Cf predicate does not
/// close the wider spoofing class (a Cyrillic homoglyph `а` achieves the
/// same with no control character involved at all, and no character-class
/// check can reach it) — general-Cf is the proportionate stop, not a step
/// toward homoglyph detection.
fn is_cf(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
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
            "\\u{1b}[2J\\u{1b}[1;31mCRITICAL\\u{1b}[0m"
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
        assert_eq!(sanitize_console_text("a\nb\tc"), "a\\u{a}b\\u{9}c");
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
        assert_eq!(output, "safe\\u{202e}evil");
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
        assert_eq!(sanitize_console_text("\x1b[2J"), "\\u{1b}[2J");
    }

    // Sweep (Dev-B MINOR A, Security LOW, both lanes) — the escape was not
    // INJECTIVE: a literal source text spelling out the escape marker
    // (`\x1b`, four printable characters) rendered byte-identically to a
    // REAL control byte (0x1b) escaped by this very function, because (a)
    // a literal backslash was never itself escaped, and (b) `\xHH` is a
    // MINIMUM-width format — U+202E emits four hex digits, so a literal
    // `\x20` followed by the plain text `28` collides with a real U+2028.
    // Two independent lanes converged on this without seeing each other's
    // report. Fix: escape the backslash itself (`\` -> `\\`), and use the
    // brace-delimited `\u{HH}` form, whose closing `}` makes the boundary
    // unambiguous regardless of how many hex digits follow.
    //
    // Test List: 10. the two example sources from the report render
    // DIFFERENTLY. 11. a literal backslash is escaped to `\\`, so it can
    // never be mistaken for the start of an escape marker. 12. the
    // brace-delimited form removes the variable-width collision
    // (`\x20`+`28` no longer equals a real U+2028).

    #[test]
    fn sanitize_console_text_is_injective_literal_backslash_x_vs_real_escape_byte() {
        let literal_backslash_text = "\\x1b[2J-LITERAL";
        let real_escape_byte_text = "\x1b[2J-LITERAL";
        assert_ne!(
            sanitize_console_text(literal_backslash_text),
            sanitize_console_text(real_escape_byte_text),
            "a literal '\\x1b' text and a REAL ESC byte must render \
             differently — the operator must be able to tell them apart"
        );
    }

    #[test]
    fn sanitize_console_text_escapes_a_literal_backslash() {
        assert_eq!(sanitize_console_text("a\\b"), "a\\\\b");
    }

    #[test]
    fn sanitize_console_text_brace_delimited_form_has_no_variable_width_collision() {
        // A literal `\x20` followed by the plain characters '2','8' must
        // NOT collide with a real U+2028 (line separator) once escaped.
        let literal_x20_then_28 = "\\x2028";
        let real_u2028 = "\u{2028}";
        assert_ne!(
            sanitize_console_text(literal_x20_then_28),
            sanitize_console_text(real_u2028)
        );
    }

    // Sweep (Dev-B MINOR B, Security MEDIUM, both lanes) — the previous
    // class enumerated only the BIDI subset of category Cf. Security
    // verified in real project output that U+200B (ZWSP), U+FEFF and
    // U+00AD (SHY) each reach the terminal raw, making two functions named
    // e.g. "authenticate" and "auth<ZWSP>enticate" render as VISUALLY
    // IDENTICAL report lines — the same "forge what the operator reads"
    // class as the RLO vector, just without terminal reordering. Widened
    // to the general Cf (format character) predicate `is_cf`, which also
    // covers ZWNJ/ZWJ/WORD JOINER/the tag block Dev-B listed without
    // naming each one individually — that is the point of a category
    // predicate over an enumerated subset.
    //
    // Test List: 13. ZWSP renders visibly distinct from a clean twin. 14.
    // FEFF (BOM) and SHY are each neutralized. 15. ZWNJ/ZWJ/WORD JOINER
    // are each neutralized. 16. a tag-block character is neutralized.

    #[test]
    fn sanitize_console_text_makes_a_zwsp_name_visibly_distinct_from_its_clean_twin() {
        let clean = "authenticate";
        let hostile = "auth\u{200B}enticate";
        let sanitized_clean = sanitize_console_text(clean);
        let sanitized_hostile = sanitize_console_text(hostile);
        assert_ne!(
            sanitized_clean, sanitized_hostile,
            "a ZWSP-carrying name must render visibly distinct from its clean twin, \
             not collapse to the same report line"
        );
        // The mere presence of different UTF-8 bytes is not "visibly
        // distinct" to a human reading a terminal — ZWSP itself must be
        // neutralized to a VISIBLE marker, or the two lines print
        // identically to the eye despite differing at the byte level.
        assert!(
            !sanitized_hostile.contains('\u{200B}'),
            "the raw ZWSP must not reach the rendered output: {:?}",
            sanitized_hostile
        );
    }

    #[test]
    fn sanitize_console_text_neutralizes_bom_and_soft_hyphen() {
        for c in ['\u{FEFF}', '\u{00AD}'] {
            let output = sanitize_console_text(&format!("a{}b", c));
            assert!(
                !output.contains(c),
                "{:?} (U+{:04X}) must be neutralized, got: {:?}",
                c,
                c as u32,
                output
            );
        }
    }

    #[test]
    fn sanitize_console_text_neutralizes_zwnj_zwj_and_word_joiner() {
        for c in ['\u{200C}', '\u{200D}', '\u{2060}'] {
            let output = sanitize_console_text(&format!("a{}b", c));
            assert!(
                !output.contains(c),
                "{:?} (U+{:04X}) must be neutralized, got: {:?}",
                c,
                c as u32,
                output
            );
        }
    }

    #[test]
    fn sanitize_console_text_neutralizes_a_tag_block_character() {
        // U+E0020 TAG SPACE — part of the U+E0000-U+E007F tag block.
        let output = sanitize_console_text("a\u{E0020}b");
        assert!(
            !output.contains('\u{E0020}'),
            "a tag-block character must be neutralized, got: {:?}",
            output
        );
    }

    // Test List (render_incomplete_coverage_warning — QA LOW/Security LOW,
    // #128 retry 1): the function had NO direct unit test anywhere — only
    // an e2e substring check (`stderr.contains("1 fichier")`), which the
    // singular/plural grammar bug survived undetected.
    // 1. Complete -> empty string
    // 2. Partial{1} -> singular grammar ("1 fichier n'a pas pu être
    //    mesuré", not "fichier(s) ... mesuré(s)" — the verb stayed plural
    //    at count 1 before this fix)
    // 3. Partial{N>1} -> plural grammar
    // 4. Absent -> the absence message, naming no count at all

    #[test]
    fn render_incomplete_coverage_warning_of_complete_is_empty() {
        assert_eq!(
            render_incomplete_coverage_warning(GateCoverage::Complete),
            ""
        );
    }

    #[test]
    fn render_incomplete_coverage_warning_of_one_unmeasurable_file_uses_singular_grammar() {
        let warning = render_incomplete_coverage_warning(GateCoverage::Partial {
            unmeasurable_files: 1,
        });
        assert!(
            warning.contains("1 fichier n'a pas pu être mesuré"),
            "count 1 must use singular grammar (\"n'a pas pu être mesuré\"), got: {warning}"
        );
    }

    #[test]
    fn render_incomplete_coverage_warning_of_several_unmeasurable_files_uses_plural_grammar() {
        let warning = render_incomplete_coverage_warning(GateCoverage::Partial {
            unmeasurable_files: 3,
        });
        assert!(
            warning.contains("3 fichiers n'ont pas pu être mesurés"),
            "count > 1 must use plural grammar, got: {warning}"
        );
    }

    #[test]
    fn render_incomplete_coverage_warning_of_absent_names_the_absence_not_a_count() {
        let warning = render_incomplete_coverage_warning(GateCoverage::Absent);
        assert!(
            warning.contains("la mesure n'a pas pu être prise"),
            "Absent must name the absence itself, not a fabricated count, got: {warning}"
        );
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
