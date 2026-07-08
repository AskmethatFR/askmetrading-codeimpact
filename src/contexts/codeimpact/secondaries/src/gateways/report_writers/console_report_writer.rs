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
        println!("Complexité directe: {}", metrics.cyclomatic_complexity());
        println!(
            "Complexité transitive: {} (dont {} cachée dans les appels)",
            metrics.transitive_complexity(),
            metrics.hidden_complexity(),
        );
        println!("Profondeur d'appels max: {}", metrics.max_call_depth());
        let cycle_count = metrics.functions_with_cycles().len();
        println!("Fonctions avec cycle: {}", cycle_count);
        println!("Niveau: {}", metrics.complexity_level());
        println!("========================");
        Ok(())
    }
}
