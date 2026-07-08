use super::analysis_rule::AnalysisRule;
use super::code_metrics::CodeMetrics;
use super::code_parser::CodeParser;
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

    Ok(CodeMetrics::new(complexity))
}
