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

fn format_metric_value(metric: BreachedMetric, value: f64) -> String {
    match metric {
        BreachedMetric::Cpu => format_dollars(value),
        BreachedMetric::Co2 => format!("{:.1} g", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
