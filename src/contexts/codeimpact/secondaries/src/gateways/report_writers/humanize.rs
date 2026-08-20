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
        // QA LOW / Security LOW + self-flagged (#128 retry 1): the "(s)"
        // shorthand left the VERB plural even at count 1 ("1 fichier(s)
        // n'ONT pas pu être mesuré(s)") — grammatically wrong French, not
        // just informal. Proper singular/plural agreement instead of a
        // shorthand that only ever covers the noun.
        // #128 retry 2 (Security HIGH): `unmeasurable_files` and
        // `unexplored_subtree` name TWO DIFFERENT absences — a count of
        // named files vs. an unquantified "at least one subtree was never
        // enumerated" — and must stay two separate, truthful sentences.
        // Merging them (e.g. "N+1 fichiers") would fabricate a count for
        // the subtree case exactly as ADR-0010 forbids.
        GateCoverage::Partial {
            unmeasurable_files,
            unexplored_subtree,
        } => {
            let mut clauses = Vec::new();
            if unmeasurable_files > 0 {
                let (noun, verb) = if unmeasurable_files == 1 {
                    ("fichier", "n'a pas pu être mesuré")
                } else {
                    ("fichiers", "n'ont pas pu être mesurés")
                };
                clauses.push(format!(
                    "{unmeasurable_files} {noun} {verb} — consultez la liste des fichiers non \
                     mesurés dans le rapport"
                ));
            }
            if unexplored_subtree {
                clauses.push(
                    "au moins une arborescence de fichiers n'a pas pu être explorée \
                     entièrement (profondeur maximale atteinte ou accès refusé) — son \
                     contenu n'est comptabilisé nulle part"
                        .to_string(),
                );
            }
            format!(
                "=== Couverture du seuil incomplète ===\n\
                 [SEUIL NON ÉVALUABLE EN TOTALITÉ] le seuil n'a donc pas pu s'appliquer à \
                 l'ensemble du projet : {}.",
                clauses.join(" ; ")
            )
        }
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

/// Console-output sanitization. Moved to the hexagon in #147 (Volet C):
/// `RunAnalysis` (zero-dep hexagon, ADR-0001) prints `eprintln!` warnings
/// carrying file names and cannot import a secondaries function — the
/// sanitizer now lives in `codeimpact_hexagon::analysis::console_sanitize`
/// (full doc + tests there) and is re-exported here so every adapter
/// writer call site keeps working unchanged (one implementation, cc-kiss).
pub use codeimpact_hexagon::analysis::sanitize_console_text;

#[cfg(test)]
mod tests {
    use super::*;

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
            unexplored_subtree: false,
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
            unexplored_subtree: false,
        });
        assert!(
            warning.contains("3 fichiers n'ont pas pu être mesurés"),
            "count > 1 must use plural grammar, got: {warning}"
        );
    }

    // Security HIGH (retry 2, #128) — `unexplored_subtree` names a
    // DIFFERENT absence than `unmeasurable_files`: "N files could not be
    // measured" and "at least one subtree could not be explored" must stay
    // two separate, truthful sentences — never merged into a fabricated
    // count (ADR-0010, same discipline as `Absent` above).
    //
    // Test List:
    // 1. unexplored_subtree alone (0 unmeasurable files) -> names the
    //    subtree, never mentions a file count
    // 2. both unmeasurable_files > 0 AND unexplored_subtree -> both
    //    sentences present, neither one swallows the other

    #[test]
    fn render_incomplete_coverage_warning_of_unexplored_subtree_alone_names_it_not_a_file_count() {
        let warning = render_incomplete_coverage_warning(GateCoverage::Partial {
            unmeasurable_files: 0,
            unexplored_subtree: true,
        });
        assert!(
            warning.contains("arborescence") && warning.contains("explor"),
            "must name that a subtree could not be explored, got: {warning}"
        );
        assert!(
            !warning.contains("0 fichier"),
            "must never fabricate a '0 fichier(s)' count for an absence it never quantified, \
             got: {warning}"
        );
    }

    #[test]
    fn render_incomplete_coverage_warning_of_files_and_unexplored_subtree_names_both() {
        let warning = render_incomplete_coverage_warning(GateCoverage::Partial {
            unmeasurable_files: 2,
            unexplored_subtree: true,
        });
        assert!(
            warning.contains("2 fichiers n'ont pas pu être mesurés"),
            "the named-files sentence must still be present, got: {warning}"
        );
        assert!(
            warning.contains("arborescence") && warning.contains("explor"),
            "the unexplored-subtree sentence must ALSO be present — neither absence may \
             swallow the other, got: {warning}"
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
