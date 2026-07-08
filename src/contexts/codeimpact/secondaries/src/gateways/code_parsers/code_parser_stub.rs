use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::ParsedFunction;

pub struct CodeParserStub {
    result: Result<Vec<ParsedFunction>, AnalysisError>,
}

impl CodeParserStub {
    pub fn new(result: Result<Vec<ParsedFunction>, AnalysisError>) -> Self {
        Self { result }
    }

    pub fn with_functions(functions: Vec<ParsedFunction>) -> Self {
        Self {
            result: Ok(functions),
        }
    }
}

impl CodeParser for CodeParserStub {
    fn parse(&self, _source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        match &self.result {
            Ok(functions) => Ok(functions.clone()),
            Err(e) => match e {
                AnalysisError::IoError(msg) => Err(AnalysisError::IoError(msg.clone())),
                AnalysisError::AnalysisFailed(msg) => {
                    Err(AnalysisError::AnalysisFailed(msg.clone()))
                }
                AnalysisError::UnsupportedTarget(msg) => {
                    Err(AnalysisError::UnsupportedTarget(msg.clone()))
                }
            },
        }
    }
}
