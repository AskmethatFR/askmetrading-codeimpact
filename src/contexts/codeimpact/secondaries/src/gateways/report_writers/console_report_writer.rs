use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ReportWriter;

#[derive(Default)]
pub struct ConsoleReportWriter;

impl ConsoleReportWriter {
    pub fn new() -> Self {
        Self
    }
}

impl ReportWriter for ConsoleReportWriter {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        println!("=== Rapport d'analyse ===");
        println!("Complexité: {}", metrics.cyclomatic_complexity());
        println!("Niveau: {}", metrics.complexity_level());
        println!("========================");
        Ok(())
    }
}
