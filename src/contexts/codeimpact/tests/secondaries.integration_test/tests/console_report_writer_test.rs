use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;

#[test]
fn write_console_does_not_panic() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_zero_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(0);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_high_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(50);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_economic_impact() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(18.5, 12600, 19.7, "moderate");
    let metrics = CodeMetrics::new(27).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_memory_mb() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(50.0, 2_000_000, 50.2, "high");
    let metrics = CodeMetrics::new(30).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_ecological_impact() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(6000.0, 0, 6000.0, "low");
    let ecological = EcologicalImpact::new(2.4, 21600.0, EfficiencyClass::B);
    let metrics = CodeMetrics::new(27)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_ecological_zero_co2() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(0.0, 0, 0.0, "low");
    let ecological = EcologicalImpact::new(0.0, 0.0, EfficiencyClass::A);
    let metrics = CodeMetrics::new(1)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}
