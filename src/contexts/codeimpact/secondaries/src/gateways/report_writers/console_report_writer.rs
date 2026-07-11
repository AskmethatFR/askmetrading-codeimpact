use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EcologicalImpactEstimator;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::WarningSeverity;

const MICRODOLLARS_TO_DOLLARS: f64 = 1_000_000.0;
const KB_TO_MB: f64 = 1024.0;
const MB_TO_GB: f64 = 1024.0;

fn format_dollars(microdollars: f64) -> String {
    let dollars = microdollars / MICRODOLLARS_TO_DOLLARS;
    if dollars < 0.0001 {
        format!("${:.6}", dollars)
    } else if dollars < 1.0 {
        format!("${:.4}", dollars)
    } else {
        format!("${:.2}", dollars)
    }
}

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
            println!("Coût CPU: {}", format_dollars(economic.cpu_cost_microdollars()));
            let memory_kb = economic.memory_bytes() as f64 / 1024.0;
            if memory_kb >= 1024.0 {
                println!("Mémoire: {:.1} MB", memory_kb / 1024.0);
            } else {
                println!("Mémoire: {:.1} KB", memory_kb);
            }
            println!("Coût total: {}", format_dollars(economic.total_cost_microdollars()));
            println!("Niveau: {}", economic.level());
        }

        if let Some(ecological) = metrics.ecological_impact() {
            println!();
            println!("=== Impact écologique estimé ===");
            println!("CO₂: {:.1} g", ecological.co2_grams());
            let energy_joules = ecological.energy_joules();
            let energy_kwh = energy_joules / EcologicalImpactEstimator::KWH_TO_JOULES;
            if energy_joules >= 1000.0 {
                println!("Énergie: {:.1} kJ ({:.4} kWh)", energy_joules / 1000.0, energy_kwh);
            } else {
                println!("Énergie: {:.1} J ({:.6} kWh)", energy_joules, energy_kwh);
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

        let io_in_loops = metrics.io_in_loops();
        if !io_in_loops.is_empty() {
            println!();
            println!("=== I/O dans boucles ===");
            for w in io_in_loops {
                let location_str = if w.location.file_path().is_empty() {
                    format!("{}:{}", w.location.line(), w.location.col())
                } else {
                    w.location.to_string()
                };
                println!(
                    "[CRITICAL] {} → I/O dans boucle: {} ({})",
                    w.function, w.io_call, location_str
                );
            }
            println!("========================");
        }

        Ok(())
    }

    fn write_json(
        &self,
        metrics: &CodeMetrics,
        target: &str,
        target_type: &str,
    ) -> Result<String, AnalysisError> {
        // ConsoleReportWriter uses same DTOs as JsonReportWriter for consistent JSON output (ADR-4.4)
        use super::json_report_writer;
        json_report_writer::serialize_metrics(metrics, target, target_type)
    }

    fn write_project_report(
        &self,
        graph: &FileConsumptionGraph,
    ) -> Result<(), AnalysisError> {
        let aggregated = graph.aggregated_metrics();

        println!("=== Métriques par fichier ===");
        let per_file = graph.per_file_metrics();
        if per_file.is_empty() {
            println!("(aucun fichier analysé)");
            return Ok(());
        }

        // Sort files for deterministic output
        let mut sorted_files: Vec<&PathBuf> = per_file.keys().collect();
        sorted_files.sort();

        for path in &sorted_files {
            if let Some(metrics) = per_file.get(*path) {
                println!(
                    "{} — complexité directe: {}, complexité transitive: {}, niveau: {}",
                    path.display(),
                    metrics.cyclomatic_complexity(),
                    metrics.transitive_complexity(),
                    metrics.complexity_level(),
                );
            }
        }
        println!();

        println!("=== Chaînes de consommation ===");
        for path in &sorted_files {
            let chain = graph.consumption_chain(path);
            if chain.len() > 1 {
                let chain_str: Vec<String> = chain
                    .iter()
                    .map(|p| p.file_stem().unwrap().to_str().unwrap().to_string())
                    .collect();
                println!("  {} → {}", path.display(), chain_str.join(" → "));
            }
        }
        println!();

        println!("=== Cycles ===");
        let cycles = graph.files_with_cycles();
        if cycles.is_empty() {
            println!("  (aucun cycle détecté)");
        } else {
            for path in &cycles {
                println!("  {} fait partie d'un cycle de dépendances", path.display());
            }
        }
        println!();

        println!("=== Résumé du projet ===");
        println!("Fichiers analysés: {}", aggregated.total_files);
        println!("Dépendances totales: {}", graph.total_dependencies());
        println!("Complexité directe totale: {}", aggregated.total_cyclomatic_complexity);
        println!("Complexité transitive totale: {}", aggregated.total_transitive_complexity);
        println!("Profondeur max de chaîne: {}", aggregated.max_call_depth);
        println!("Fichiers en cycle: {}", aggregated.files_with_cycles.len());

        if let Some(economic) = &aggregated.total_economic_impact {
            println!();
            println!("=== Impact économique total ===");
            println!("Coût CPU: {}", format_dollars(economic.cpu_cost_microdollars()));
            let memory_kb = economic.memory_bytes() as f64 / 1024.0;
            if memory_kb >= 1024.0 {
                println!("Mémoire: {:.1} MB", memory_kb / 1024.0);
            } else {
                println!("Mémoire: {:.1} KB", memory_kb);
            }
            println!("Coût total: {}", format_dollars(economic.total_cost_microdollars()));
            println!("Niveau: {}", economic.level());
        }

        if let Some(ecological) = &aggregated.total_ecological_impact {
            println!();
            println!("=== Impact écologique total ===");
            println!("CO₂: {:.1} g", ecological.co2_grams());
            let energy_joules = ecological.energy_joules();
            let energy_kwh = energy_joules / EcologicalImpactEstimator::KWH_TO_JOULES;
            if energy_joules >= 1000.0 {
                println!("Énergie: {:.1} kJ ({:.4} kWh)", energy_joules / 1000.0, energy_kwh);
            } else {
                println!("Énergie: {:.1} J ({:.6} kWh)", energy_joules, energy_kwh);
            }
            println!("Classe: {}", ecological.efficiency_class().label());
        }

        println!("==============================");

        Ok(())
    }

    fn write_stress_test(
        &self,
        run: &StressTestRun,
        impact: &EconomicImpact,
    ) -> Result<(), AnalysisError> {
        println!("=== Stress Test ===");
        let filter_label = run
            .filter()
            .map(|f| format!(" (filtre: {})", f))
            .unwrap_or_default();
        println!("Tests: {}/{} passés{}", run.tests_passed(), run.tests_total(), filter_label);
        println!("Durée: {} ms", run.duration_ms());
        println!("Temps CPU: {} ms", run.cpu_time_ms());
        let memory_mb = run.memory_kb() as f64 / KB_TO_MB;
        if memory_mb >= MB_TO_GB {
            println!("Mémoire: {:.1} GB", memory_mb / MB_TO_GB);
        } else {
            println!("Mémoire: {:.1} MB", memory_mb);
        }
        println!();
        println!("=== Impact économique réel ===");
        println!("Coût CPU: {}", format_dollars(impact.cpu_cost_microdollars()));
        let memory_mb = impact.memory_bytes() as f64 / KB_TO_MB / KB_TO_MB;
        if memory_mb >= MB_TO_GB {
            println!("Mémoire: {:.1} GB", memory_mb / MB_TO_GB);
        } else {
            println!("Mémoire: {:.1} MB", memory_mb);
        }
        println!("Coût total: {}", format_dollars(impact.total_cost_microdollars()));
        println!("Niveau: {}", impact.level());
        println!("==============================");
        Ok(())
    }
}
