use std::sync::{Arc, Mutex};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;

#[derive(Clone)]
pub struct SharedReportWriterStub {
    pub last_metrics: Arc<Mutex<Option<CodeMetrics>>>,
    pub last_graph: Arc<Mutex<Option<FileConsumptionGraph>>>,
    pub last_stress_run: Arc<Mutex<Option<StressTestRun>>>,
    pub last_stress_impact: Arc<Mutex<Option<Measurement<EconomicImpact>>>>,
    pub last_json: Arc<Mutex<Option<String>>>,
    pub last_html: Arc<Mutex<Option<String>>>,
}

impl SharedReportWriterStub {
    pub fn new() -> Self {
        Self {
            last_metrics: Arc::new(Mutex::new(None)),
            last_graph: Arc::new(Mutex::new(None)),
            last_stress_run: Arc::new(Mutex::new(None)),
            last_stress_impact: Arc::new(Mutex::new(None)),
            last_json: Arc::new(Mutex::new(None)),
            last_html: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for SharedReportWriterStub {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportWriter for SharedReportWriterStub {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        *self.last_metrics.lock().unwrap() = Some(metrics.clone());
        Ok(())
    }

    fn write_json(
        &self,
        metrics: &CodeMetrics,
        target: &str,
        target_type: &str,
    ) -> Result<String, AnalysisError> {
        let json = format!(
            r#"{{"tool":{{"name":"codeimpact","version":"0.1.0"}},"timestamp":"2026-07-11T15:30:00Z","target":"{}","target_type":"{}","metrics":{{"cyclomatic_complexity":{}}}}}"#,
            target,
            target_type,
            metrics.cyclomatic_complexity()
        );
        *self.last_json.lock().unwrap() = Some(json.clone());
        Ok(json)
    }

    fn write_project_report(&self, graph: &FileConsumptionGraph) -> Result<(), AnalysisError> {
        *self.last_graph.lock().unwrap() = Some(graph.clone());
        Ok(())
    }

    fn write_project_json(
        &self,
        graph: &FileConsumptionGraph,
        target: &str,
    ) -> Result<String, AnalysisError> {
        *self.last_graph.lock().unwrap() = Some(graph.clone());
        let aggregated = graph.aggregated_metrics();
        let json = format!(
            r#"{{"tool":{{"name":"codeimpact","version":"0.1.0"}},"timestamp":"2026-07-11T15:30:00Z","target":"{}","target_type":"project","metrics":{{"cyclomatic_complexity":{}}}}}"#,
            target, aggregated.total_cyclomatic_complexity
        );
        *self.last_json.lock().unwrap() = Some(json.clone());
        Ok(json)
    }

    fn write_stress_test(
        &self,
        run: &StressTestRun,
        impact: &Measurement<EconomicImpact>,
    ) -> Result<(), AnalysisError> {
        *self.last_stress_run.lock().unwrap() = Some(run.clone());
        *self.last_stress_impact.lock().unwrap() = Some(impact.clone());
        Ok(())
    }

    fn write_html(
        &self,
        graph: &FileConsumptionGraph,
        target: &str,
    ) -> Result<String, AnalysisError> {
        *self.last_graph.lock().unwrap() = Some(graph.clone());
        let html = format!(
            "<!DOCTYPE html><html><body>stub html for {} ({} files)</body></html>",
            target,
            graph.files().len()
        );
        *self.last_html.lock().unwrap() = Some(html.clone());
        Ok(html)
    }
}
