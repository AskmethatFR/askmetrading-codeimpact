use super::analysis_rule::AnalysisRule;
use super::call_graph::CallGraph;
use super::code_metrics::{CodeMetrics, FunctionDetail};
use super::code_parser::CodeParser;
use super::complexity_detector::{ComplexityDetector, DetectionConfig};
use super::errors::AnalysisError;

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
        }
    }

    let call_graph = CallGraph::build(&functions);

    let function_details: Vec<FunctionDetail> = functions
        .iter()
        .map(|f| {
            let direct = call_graph.direct_of(&f.name);
            let transitive = call_graph.transitive_of(&f.name);
            let call_depth = call_graph.call_chain_depth(&f.name);
            let in_cycle = call_graph.has_cycle(&f.name);
            FunctionDetail {
                name: f.name.clone(),
                direct,
                transitive,
                call_depth,
                in_cycle,
            }
        })
        .collect();

    let transitive_complexity = call_graph.transitive_total();
    let max_call_depth = call_graph.max_call_depth();
    let functions_with_cycles = call_graph.functions_with_cycles();

    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&functions, &call_graph, &config);

    Ok(CodeMetrics::with_call_graph(
        complexity,
        transitive_complexity,
        max_call_depth,
        functions_with_cycles,
        function_details,
    )
    .with_warnings(warnings))
}
