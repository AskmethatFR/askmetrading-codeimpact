use codeimpact_hexagon::analysis::CodeMetrics;
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
