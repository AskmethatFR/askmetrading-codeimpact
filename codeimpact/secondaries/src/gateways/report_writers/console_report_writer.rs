use codeimpact_hexagon::domain_model::code_metrics::CodeMetrics;
use codeimpact_hexagon::domain_model::errors::AnalysisError;
use codeimpact_hexagon::gateways_secondary_ports::report_writer_port::ReportWriterPort;

/// Adaptateur — écrit le rapport d'analyse dans la console.
#[derive(Default)]
pub struct ConsoleReportWriter;

impl ConsoleReportWriter {
    pub fn new() -> Self {
        Self
    }
}

impl ReportWriterPort for ConsoleReportWriter {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        println!("=== Rapport d'Analyse ===");
        println!("Complexité: {}", metrics.cyclomatic_complexity());
        println!("Niveau: {}", metrics.complexity_level());
        println!("========================");
        Ok(())
    }
}
