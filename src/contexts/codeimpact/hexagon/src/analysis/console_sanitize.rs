/// Console-output sanitization (#147, Volet C — LOW): neutralizes control
/// characters in text derived from ANALYZED SOURCE CODE before it reaches a
/// terminal. Moved here from `secondaries/.../report_writers/humanize.rs`
/// in #147 because the `run_analysis` use case (the hexagon) prints
/// `eprintln!` warnings carrying file names, and the hexagon is zero-dep
/// (ADR-0001): it cannot import a secondaries function, so the sanitizer
/// lives in the domain and `humanize.rs` re-exports it for the adapter
/// writers (one implementation, cc-kiss).
///
/// Threat (US17 T1 retry, Security MEDIUM, CWE-117/CWE-150): until the
/// TS/JS tree-sitter adapter landed, every producer of a
/// `ParsedFunction`/`FunctionDetail` name was a Rust or C# identifier — a
/// closed character set with no control bytes possible. A JS/TS
/// `method_definition` key can be an arbitrary STRING LITERAL, and a FILE
/// NAME on disk can carry anything, so hostile input can now carry raw ANSI
/// escape sequences (`ESC[2J` clears the screen, `ESC[1;31m` recolors,
/// `ESC[8m` conceals) and forge or hide what the operator reads — under
/// this tool's threat model the report IS the product, and a warning
/// printed immediately before the coverage warning can leave an active SGR
/// attribute that hides the `[SEUIL NON ÉVALUABLE EN TOTALITÉ]` line
/// (#146/#147 Volet C, measured by Security: `\x1b[8m z.rs` conceals the
/// next line).
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
}
