use std::io::Write;
use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::MetricSupport;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::WarningSeverity;

use super::humanize::{format_dollars, format_energy, format_memory, render_threshold_warning};

const KB_TO_MB: f64 = 1024.0;
const MB_TO_GB: f64 = 1024.0;

#[derive(Default)]
pub struct ConsoleReportWriter;

impl ConsoleReportWriter {
    pub fn new() -> Self {
        Self
    }

    /// Write console report to a custom writer (used for testing).
    pub fn write_console_to(&self, writer: &mut dyn Write, metrics: &CodeMetrics) {
        // T3 (US16, #33): a language whose io_in_loops capability is
        // Unsupported (C#, Q1 human-approved — nothing measured until T4)
        // must never render a `0` that reads as "measured, nothing found".
        // `None` (Rust, or no capabilities attached at all) keeps today's
        // behavior exactly — zero behavior change.
        let io_na_language = metrics.capabilities().and_then(|c| {
            matches!(c.io_in_loops(), MetricSupport::Unsupported)
                .then(|| c.language().display_name())
        });

        writeln!(writer, "=== Rapport d'analyse ===").unwrap();
        writeln!(
            writer,
            "Complexité directe: {}",
            metrics.cyclomatic_complexity()
        )
        .unwrap();
        let call_graph_note = match metrics.capabilities().map(|c| c.call_graph()) {
            Some(MetricSupport::Degraded(reason)) => format!(" [dégradé: {}]", reason),
            _ => String::new(),
        };
        writeln!(
            writer,
            "Complexité transitive: {} (dont {} cachée dans les appels){}",
            metrics.transitive_complexity(),
            metrics.hidden_complexity(),
            call_graph_note,
        )
        .unwrap();
        writeln!(
            writer,
            "Profondeur d'appels max: {}",
            metrics.max_call_depth()
        )
        .unwrap();
        let cycle_count = metrics.functions_with_cycles().len();
        writeln!(writer, "Fonctions avec cycle: {}", cycle_count).unwrap();
        writeln!(writer, "Niveau: {}", metrics.complexity_level()).unwrap();
        // #56 T2 — abstention (ADR-0010/ADR-0014 §4): one synthesis line,
        // always printed (a measured `0` is legitimate and honest, never
        // omitted) — never per-line detail, which would turn an
        // abstention into a pseudo-warning. T3: an Unsupported io_in_loops
        // capability overrides this with an explicit n/a — the count
        // itself carries no honest meaning when nothing was measured.
        match io_na_language {
            Some(language) => writeln!(
                writer,
                "Appels en boucle non classifiables: n/a — non supporté pour {}",
                language
            )
            .unwrap(),
            None => writeln!(
                writer,
                "Appels en boucle non classifiables: {}",
                metrics.unclassifiable_io_in_loops_count()
            )
            .unwrap(),
        }

        let details = metrics.function_details();
        if !details.is_empty() {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Détails par fonction ===").unwrap();
            for d in details {
                let loc = if d.location().file_path().is_empty() {
                    format!(":{}", d.location().line())
                } else {
                    d.location().to_string()
                };
                let cycle = if d.in_cycle() { " [cycle]" } else { "" };
                writeln!(
                    writer,
                    "  {} — directe: {}, transitive: {}, profondeur: {}{} ({})",
                    d.name(),
                    d.direct(),
                    d.transitive(),
                    d.call_depth(),
                    cycle,
                    loc
                )
                .unwrap();
            }
        }

        if let Some(economic) = metrics.economic_impact() {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Impact économique estimé ===").unwrap();
            writeln!(
                writer,
                "Coût CPU: {}",
                format_dollars(economic.cpu_cost_microdollars())
            )
            .unwrap();
            writeln!(
                writer,
                "Mémoire: {}",
                format_memory(economic.memory_bytes())
            )
            .unwrap();
            writeln!(
                writer,
                "Coût total: {}",
                format_dollars(economic.total_cost_microdollars())
            )
            .unwrap();
            writeln!(writer, "Niveau: {}", economic.level()).unwrap();
        }

        if let Some(ecological) = metrics.ecological_impact() {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Impact écologique estimé ===").unwrap();
            writeln!(writer, "CO₂: {:.1} g", ecological.co2_grams()).unwrap();
            writeln!(
                writer,
                "Énergie: {}",
                format_energy(ecological.energy_joules())
            )
            .unwrap();
            writeln!(writer, "Classe: {}", ecological.efficiency_class().label()).unwrap();
        }

        let warnings = metrics.warnings();
        if !warnings.is_empty() {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Avertissements ===").unwrap();
            for w in warnings {
                let label = match w.severity {
                    WarningSeverity::Warning => "WARNING",
                    WarningSeverity::Critical => "CRITICAL",
                };
                let loc = if w.location.file_path().is_empty() {
                    format!(":{}", w.location.line())
                } else {
                    w.location.to_string()
                };
                writeln!(
                    writer,
                    "[{}][{:?}] {} → {} ({})",
                    label, w.pattern, w.function, w.message, loc
                )
                .unwrap();
            }
            writeln!(writer, "========================").unwrap();
        } else {
            writeln!(writer, "========================").unwrap();
        }

        let io_in_loops = metrics.io_in_loops();
        if let Some(language) = io_na_language {
            // T3: an explicit n/a, never the silent-omit an empty Vec would
            // otherwise produce below — silence here would read as
            // "measured, nothing found", not "not supported".
            writeln!(writer).unwrap();
            writeln!(writer, "=== I/O dans boucles ===").unwrap();
            writeln!(writer, "n/a — non supporté pour {}", language).unwrap();
            writeln!(writer, "========================").unwrap();
        } else if !io_in_loops.is_empty() {
            writeln!(writer).unwrap();
            writeln!(writer, "=== I/O dans boucles ===").unwrap();
            for w in io_in_loops {
                let location_str = if w.location.file_path().is_empty() {
                    format!("{}:{}", w.location.line(), w.location.col())
                } else {
                    w.location.to_string()
                };
                writeln!(
                    writer,
                    "[CRITICAL] {} → I/O dans boucle: {} ({})",
                    w.function, w.io_call, location_str
                )
                .unwrap();
            }
            writeln!(writer, "========================").unwrap();
        }

        // US8 — AD-3: same shared renderer as the project surface, single-
        // file twin (found while writing the e2e single-file test: this
        // wiring was missing here even though write_project_report_to had
        // it).
        if let Some(report) = metrics.threshold_report() {
            if report.has_breach() {
                writeln!(writer).unwrap();
                writeln!(writer, "{}", render_threshold_warning(report)).unwrap();
            }
        }
    }

    /// Write project report to a custom writer (used for testing).
    pub fn write_project_report_to(&self, writer: &mut dyn Write, graph: &FileConsumptionGraph) {
        let aggregated = graph.aggregated_metrics();

        writeln!(writer, "=== Métriques par fichier ===").unwrap();
        let per_file = graph.per_file_metrics();
        if per_file.is_empty() {
            writeln!(writer, "(aucun fichier analysé)").unwrap();
            return;
        }

        // Sort files for deterministic output
        let mut sorted_files: Vec<&PathBuf> = per_file.keys().collect();
        sorted_files.sort();

        for path in &sorted_files {
            if let Some(metrics) = per_file.get(*path) {
                writeln!(
                    writer,
                    "{} — complexité directe: {}, complexité transitive: {}, niveau: {}",
                    path.display(),
                    metrics.cyclomatic_complexity(),
                    metrics.transitive_complexity(),
                    metrics.complexity_level(),
                )
                .unwrap();
                for d in metrics.function_details() {
                    let loc = d.location().to_string();
                    let cycle = if d.in_cycle() { " [cycle]" } else { "" };
                    writeln!(
                        writer,
                        "    {} — directe: {}, transitive: {}, profondeur: {}{} ({})",
                        d.name(),
                        d.direct(),
                        d.transitive(),
                        d.call_depth(),
                        cycle,
                        loc
                    )
                    .unwrap();
                }
                // Hidden complexity per file
                writeln!(
                    writer,
                    "    complexité cachée dans les appels: {}",
                    metrics.hidden_complexity(),
                )
                .unwrap();
                // Warnings per file
                let warnings = metrics.warnings();
                if !warnings.is_empty() {
                    writeln!(writer, "    avertissements:").unwrap();
                    for w in warnings {
                        let label = match w.severity {
                            WarningSeverity::Warning => "WARNING",
                            WarningSeverity::Critical => "CRITICAL",
                        };
                        let loc_str = w.location.to_string();
                        writeln!(
                            writer,
                            "      [{}][{:?}] {} → {} ({})",
                            label, w.pattern, w.function, w.message, loc_str
                        )
                        .unwrap();
                    }
                }
                // I/O in loops per file
                let io_warnings = metrics.io_in_loops();
                if !io_warnings.is_empty() {
                    writeln!(writer, "    I/O dans boucles:").unwrap();
                    for w in io_warnings {
                        let loc_str = w.location.to_string();
                        writeln!(
                            writer,
                            "      [CRITICAL] {} → I/O dans boucle: {} ({})",
                            w.function, w.io_call, loc_str,
                        )
                        .unwrap();
                    }
                }
            }
        }
        writeln!(writer).unwrap();

        writeln!(writer, "=== Chaînes de consommation ===").unwrap();
        for path in &sorted_files {
            let chain = graph.consumption_chain(path);
            if chain.len() > 1 {
                let chain_str: Vec<String> = chain
                    .iter()
                    .map(|p| p.file_stem().unwrap().to_str().unwrap().to_string())
                    .collect();
                writeln!(writer, "  {} → {}", path.display(), chain_str.join(" → ")).unwrap();
            }
        }
        writeln!(writer).unwrap();

        writeln!(writer, "=== Cycles ===").unwrap();
        let cycles = graph.files_with_cycles();
        if cycles.is_empty() {
            writeln!(writer, "  (aucun cycle détecté)").unwrap();
        } else {
            for path in &cycles {
                writeln!(
                    writer,
                    "  {} fait partie d'un cycle de dépendances",
                    path.display()
                )
                .unwrap();
            }
        }
        writeln!(writer).unwrap();

        let unmeasurable = graph.unmeasurable_files();
        if !unmeasurable.is_empty() {
            writeln!(
                writer,
                "=== Fichiers NON MESURÉS ({}) ===",
                unmeasurable.len()
            )
            .unwrap();
            for f in unmeasurable {
                writeln!(writer, "  {} — {}", f.path.display(), f.reason).unwrap();
            }
            writeln!(writer).unwrap();
        }

        writeln!(writer, "=== Résumé du projet ===").unwrap();
        writeln!(writer, "Fichiers analysés: {}", aggregated.total_files).unwrap();
        writeln!(
            writer,
            "Dépendances totales: {}",
            graph.total_dependencies()
        )
        .unwrap();
        writeln!(
            writer,
            "Complexité directe totale: {}",
            aggregated.total_cyclomatic_complexity
        )
        .unwrap();
        writeln!(
            writer,
            "Complexité transitive totale: {}",
            aggregated.total_transitive_complexity
        )
        .unwrap();
        writeln!(
            writer,
            "Complexité cachée totale: {}",
            aggregated.total_hidden_complexity
        )
        .unwrap();
        writeln!(
            writer,
            "Profondeur max de chaîne: {}",
            aggregated.max_call_depth
        )
        .unwrap();
        writeln!(
            writer,
            "Fichiers en cycle: {}",
            aggregated.files_with_cycles.len()
        )
        .unwrap();
        // #56 T2 — project-total synthesis line, same convention as the
        // per-file one above: always printed, aggregate only.
        writeln!(
            writer,
            "Appels en boucle non classifiables (total): {}",
            aggregated.total_unclassifiable_io_in_loops
        )
        .unwrap();

        if let Some(economic) = &aggregated.total_economic_impact {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Impact économique total ===").unwrap();
            writeln!(
                writer,
                "Coût CPU: {}",
                format_dollars(economic.cpu_cost_microdollars())
            )
            .unwrap();
            writeln!(
                writer,
                "Mémoire: {}",
                format_memory(economic.memory_bytes())
            )
            .unwrap();
            writeln!(
                writer,
                "Coût total: {}",
                format_dollars(economic.total_cost_microdollars())
            )
            .unwrap();
            writeln!(writer, "Niveau: {}", economic.level()).unwrap();
        }

        if let Some(ecological) = &aggregated.total_ecological_impact {
            writeln!(writer).unwrap();
            writeln!(writer, "=== Impact écologique total ===").unwrap();
            writeln!(writer, "CO₂: {:.1} g", ecological.co2_grams()).unwrap();
            writeln!(
                writer,
                "Énergie: {}",
                format_energy(ecological.energy_joules())
            )
            .unwrap();
            writeln!(writer, "Classe: {}", ecological.efficiency_class().label()).unwrap();
        }

        // US8 — AD-3: the ONE shared renderer, only when there IS a breach
        // to report (render_threshold_warning returns "" otherwise, and a
        // graph that never had thresholds evaluated carries None at all).
        if let Some(report) = graph.threshold_report() {
            if report.has_breach() {
                writeln!(writer).unwrap();
                writeln!(writer, "{}", render_threshold_warning(report)).unwrap();
            }
        }

        writeln!(writer, "==============================").unwrap();
    }

    /// Write a stress-test report to a custom writer (used for testing).
    pub fn write_stress_test_to(
        &self,
        writer: &mut dyn Write,
        run: &StressTestRun,
        impact: &Measurement<EconomicImpact>,
    ) {
        writeln!(writer, "=== Stress Test ===").unwrap();
        let filter_label = run
            .filter()
            .map(|f| format!(" (filtre: {})", f))
            .unwrap_or_default();
        writeln!(
            writer,
            "Tests: {}/{} passés{}",
            run.tests_passed(),
            run.tests_total(),
            filter_label
        )
        .unwrap();
        writeln!(writer, "Durée: {} ms", run.duration_ms()).unwrap();
        match run.cpu_time_ms() {
            Measurement::Available(ms) => {
                writeln!(writer, "Temps CPU: {} ms", ms).unwrap();
            }
            Measurement::Unmeasurable(reason) => {
                writeln!(writer, "Temps CPU: n/a ({})", reason).unwrap();
            }
        }
        match run.memory_kb() {
            Measurement::Available(kb) => {
                let memory_mb = kb as f64 / KB_TO_MB;
                if memory_mb >= MB_TO_GB {
                    writeln!(writer, "Mémoire: {:.1} GB", memory_mb / MB_TO_GB).unwrap();
                } else {
                    writeln!(writer, "Mémoire: {:.1} MB", memory_mb).unwrap();
                }
            }
            Measurement::Unmeasurable(reason) => {
                writeln!(writer, "Mémoire: n/a ({})", reason).unwrap();
            }
        }
        writeln!(writer).unwrap();
        writeln!(writer, "=== Impact économique réel ===").unwrap();
        match impact {
            Measurement::Available(impact) => {
                writeln!(
                    writer,
                    "Coût CPU: {}",
                    format_dollars(impact.cpu_cost_microdollars())
                )
                .unwrap();
                let memory_mb = impact.memory_bytes() as f64 / KB_TO_MB / KB_TO_MB;
                if memory_mb >= MB_TO_GB {
                    writeln!(writer, "Mémoire: {:.1} GB", memory_mb / MB_TO_GB).unwrap();
                } else {
                    writeln!(writer, "Mémoire: {:.1} MB", memory_mb).unwrap();
                }
                writeln!(
                    writer,
                    "Coût total: {}",
                    format_dollars(impact.total_cost_microdollars())
                )
                .unwrap();
                writeln!(writer, "Niveau: {}", impact.level()).unwrap();
            }
            Measurement::Unmeasurable(reason) => {
                writeln!(writer, "Coût CPU: n/a ({})", reason).unwrap();
                writeln!(writer, "Mémoire: n/a ({})", reason).unwrap();
                writeln!(writer, "Coût total: n/a ({})", reason).unwrap();
                writeln!(writer, "Niveau: n/a ({})", reason).unwrap();
            }
        }
        writeln!(writer, "==============================").unwrap();
    }
}

impl ReportWriter for ConsoleReportWriter {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        self.write_console_to(&mut std::io::stdout().lock(), metrics);
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

    fn write_project_report(&self, graph: &FileConsumptionGraph) -> Result<(), AnalysisError> {
        self.write_project_report_to(&mut std::io::stdout().lock(), graph);
        Ok(())
    }

    fn write_project_json(
        &self,
        graph: &FileConsumptionGraph,
        target: &str,
    ) -> Result<String, AnalysisError> {
        // ConsoleReportWriter uses the same DTOs as JsonReportWriter (ADR-4.4)
        use super::json_report_writer;
        json_report_writer::serialize_project_metrics(graph, target)
    }

    fn write_stress_test(
        &self,
        run: &StressTestRun,
        impact: &Measurement<EconomicImpact>,
    ) -> Result<(), AnalysisError> {
        self.write_stress_test_to(&mut std::io::stdout().lock(), run, impact);
        Ok(())
    }

    fn write_html(
        &self,
        _graph: &FileConsumptionGraph,
        _target: &str,
    ) -> Result<String, AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "console writer does not support html output".into(),
        ))
    }
}
