use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::ParsedFunction;

pub struct CodeParserStub {
    result: Result<Vec<ParsedFunction>, AnalysisError>,
    deps_result: Option<Result<Vec<String>, AnalysisError>>,
    failing_when_source_contains: Option<(String, AnalysisError)>,
}

impl CodeParserStub {
    pub fn new(result: Result<Vec<ParsedFunction>, AnalysisError>) -> Self {
        Self {
            result,
            deps_result: None,
            failing_when_source_contains: None,
        }
    }

    pub fn with_functions(functions: Vec<ParsedFunction>) -> Self {
        Self {
            result: Ok(functions),
            deps_result: None,
            failing_when_source_contains: None,
        }
    }

    pub fn with_deps(mut self, deps: Result<Vec<String>, AnalysisError>) -> Self {
        self.deps_result = Some(deps);
        self
    }

    /// Makes `parse` fail with `err` for any source containing `marker`,
    /// otherwise returning the stub's normal result — lets a single stub
    /// distinguish a "good" file from a "bad" one by content, the way a
    /// real parser would (used to test per-file parse-failure handling in a
    /// project with a mix of valid and invalid files).
    pub fn failing_when_source_contains(mut self, marker: &str, err: AnalysisError) -> Self {
        self.failing_when_source_contains = Some((marker.to_string(), err));
        self
    }
}

impl CodeParser for CodeParserStub {
    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        if let Some((marker, err)) = &self.failing_when_source_contains {
            if source.contains(marker.as_str()) {
                return Err(err.clone());
            }
        }
        self.result.clone()
    }

    fn parse_file_dependencies(&self, _source: &str) -> Result<Vec<String>, AnalysisError> {
        match &self.deps_result {
            Some(result) => result.clone(),
            None => Ok(vec![]),
        }
    }
}
