use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::WarningSeverity;

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

        if let Some(economic) = metrics.economic_impact() {
            println!();
            println!("=== Impact économique estimé ===");
            println!("Coût CPU: {:.1} μ$", economic.cpu_cost_microdollars());
            let memory_kb = economic.memory_bytes() as f64 / 1024.0;
            if memory_kb >= 1024.0 {
                println!("Mémoire: {:.1} MB", memory_kb / 1024.0);
            } else {
                println!("Mémoire: {:.1} KB", memory_kb);
            }
            println!("Coût total: {:.1} μ$", economic.total_cost_microdollars());
            println!("Niveau: {}", economic.level());
        }

        if let Some(ecological) = metrics.ecological_impact() {
            println!();
            println!("=== Impact écologique estimé ===");
            println!("CO₂: {:.1} g", ecological.co2_grams());
            let energy_joules = ecological.energy_joules();
            if energy_joules >= 1000.0 {
                println!("Énergie: {:.1} kJ", energy_joules / 1000.0);
            } else {
                println!("Énergie: {:.1} J", energy_joules);
            }
            println!("Classe: {}", ecological.efficiency_class().label());
        }

        let warnings = metrics.warnings();
        if !warnings.is_empty() {
            println!();
            println!("=== Avertissements ===");
            for w in warnings {
                let label = match w.severity {
                    WarningSeverity::Warning => "WARNING",
                    WarningSeverity::Critical => "CRITICAL",
                };
                println!("[{}] {} → {}", label, w.function, w.message);
            }
            println!("========================");
        } else {
            println!("========================");
        }
        Ok(())
    }
}
