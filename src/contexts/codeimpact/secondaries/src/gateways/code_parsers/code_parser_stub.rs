use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::ParsedFunction;

pub struct CodeParserStub {
    result: Result<Vec<ParsedFunction>, AnalysisError>,
    deps_result: Option<Result<Vec<String>, AnalysisError>>,
}

impl CodeParserStub {
    pub fn new(result: Result<Vec<ParsedFunction>, AnalysisError>) -> Self {
        Self {
            result,
            deps_result: None,
        }
    }

    pub fn with_functions(functions: Vec<ParsedFunction>) -> Self {
        Self {
            result: Ok(functions),
            deps_result: None,
        }
    }

    pub fn with_deps(mut self, deps: Result<Vec<String>, AnalysisError>) -> Self {
        self.deps_result = Some(deps);
        self
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

    fn parse_file_dependencies(
        &self,
        _source: &str,
    ) -> Result<Vec<String>, AnalysisError> {
        match &self.deps_result {
            Some(result) => match result {
                Ok(deps) => Ok(deps.clone()),
                Err(e) => match e {
                    AnalysisError::IoError(msg) => Err(AnalysisError::IoError(msg.clone())),
                    AnalysisError::AnalysisFailed(msg) => {
                        Err(AnalysisError::AnalysisFailed(msg.clone()))
                    }
                    AnalysisError::UnsupportedTarget(msg) => {
                        Err(AnalysisError::UnsupportedTarget(msg.clone()))
                    }
                },
            },
            None => Ok(vec![]),
        }
    }
}
