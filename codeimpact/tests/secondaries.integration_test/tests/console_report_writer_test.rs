use codeimpact_hexagon::domain_model::code_metrics::CodeMetrics;
use codeimpact_hexagon::gateways_secondary_ports::report_writer_port::ReportWriterPort;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;

#[test]
fn console_report_writer_writes_to_stdout() {
    let metrics = CodeMetrics::new(12);
    let writer = ConsoleReportWriter::new();

    // This test just verifies it doesn't panic and returns Ok
    let result = writer.write_console(&metrics);
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
}

#[test]
fn console_report_writer_handles_zero_complexity() {
    let metrics = CodeMetrics::new(0);
    let writer = ConsoleReportWriter::new();

    let result = writer.write_console(&metrics);
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
}

#[test]
fn console_report_writer_handles_high_complexity() {
    let metrics = CodeMetrics::new(42);
    let writer = ConsoleReportWriter::new();

    let result = writer.write_console(&metrics);
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
}
