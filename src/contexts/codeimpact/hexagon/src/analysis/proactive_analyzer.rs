use super::analysis_rule::AnalysisRule;
use super::call_graph::CallGraph;
use super::code_location::CodeLocation;
use super::code_metrics::{CodeMetrics, FunctionDetail};
use super::code_parser::CodeParser;
use super::complexity_detector::{ComplexityDetector, DetectionConfig};
use super::ecological_impact::EcologicalImpactEstimator;
use super::economic_impact::EconomicImpactEstimator;
use super::errors::AnalysisError;
use super::io_in_loops_detector::IoInLoopsDetector;

pub fn analyze(
    source: &str,
    rules: &[AnalysisRule],
    parser: &dyn CodeParser,
) -> Result<CodeMetrics, AnalysisError> {
    let functions = parser.parse(source)?;
    let mut complexity = 1u32;

    for rule in rules {
        match rule {
            AnalysisRule::CyclomaticComplexity => {
                complexity += functions.iter().map(|f| f.decision_points).sum::<u32>();
            }
            AnalysisRule::IoInLoops => {
                // IoInLoops does not contribute to cyclomatic complexity;
                // it is handled below via IoInLoopsDetector.
            }
        }
    }

    let call_graph = CallGraph::build(&functions);

    let function_details: Vec<FunctionDetail> = functions
        .iter()
        .map(|f| {
            let direct = call_graph.direct_of(&f.name);
            let hidden = call_graph.hidden_of(&f.name);
            let call_depth = call_graph.call_chain_depth(&f.name);
            let in_cycle = call_graph.has_cycle(&f.name);
            FunctionDetail::new(
                f.name.clone(),
                CodeLocation::new(String::new(), f.start_line, 1),
                direct,
                hidden,
                call_depth,
                in_cycle,
            )
        })
        .collect();

    let transitive_complexity = call_graph.transitive_total();
    let max_call_depth = call_graph.max_call_depth();
    let functions_with_cycles = call_graph.functions_with_cycles();

    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&functions, &call_graph, &config);

    let mut metrics = CodeMetrics::with_call_graph(
        complexity,
        transitive_complexity,
        max_call_depth,
        functions_with_cycles,
        function_details,
    )
    .with_warnings(warnings);

    if rules.contains(&AnalysisRule::IoInLoops) {
        let io_warnings = IoInLoopsDetector::detect(&functions);
        metrics = metrics.with_io_in_loops(io_warnings);
        let unclassifiable_count = IoInLoopsDetector::count_unclassifiable(&functions);
        metrics = metrics.with_unclassifiable_io_in_loops_count(unclassifiable_count);
    }

    let economic = EconomicImpactEstimator::estimate(&metrics, &functions, &call_graph);
    metrics = metrics.with_economic_impact(economic);

    if let Some(economic) = metrics.economic_impact() {
        let ecological = EcologicalImpactEstimator::estimate(
            economic,
            EcologicalImpactEstimator::DEFAULT_CO2_G_PER_KWH,
        );
        metrics = metrics.with_ecological_impact(ecological);
    }

    Ok(metrics)
}
