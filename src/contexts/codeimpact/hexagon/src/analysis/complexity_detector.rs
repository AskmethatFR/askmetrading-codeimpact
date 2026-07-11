use super::call_graph::CallGraph;
use super::code_location::CodeLocation;
use super::code_parser::ParsedFunction;

/// Pattern category for a complexity warning.
#[derive(Clone, Debug, PartialEq)]
pub enum WarningPattern {
    QuadraticLoop,
    NestedLoops,
    DeepCallChain,
    HiddenComplexity,
    Recursion,
    LargeMatch,
    DeepConditional,
}

/// Severity level for a complexity warning.
#[derive(Clone, Debug, PartialEq)]
pub enum WarningSeverity {
    Warning,
    Critical,
}

/// A single complexity warning produced by the detector.
#[derive(Clone, Debug, PartialEq)]
pub struct ComplexityWarning {
    pub pattern: WarningPattern,
    pub severity: WarningSeverity,
    pub function: String,
    pub location: CodeLocation,
    pub message: String,
    pub suggestion: String,
}

/// Configuration thresholds for the complexity detector.
#[derive(Clone, Debug)]
pub struct DetectionConfig {
    pub max_call_depth: usize,
    pub complexity_ratio: f64,
    pub max_match_arms: usize,
    pub max_conditional_depth: usize,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            max_call_depth: 5,
            complexity_ratio: 5.0,
            max_match_arms: 10,
            max_conditional_depth: 5,
        }
    }
}

/// Domain service that detects problematic complexity patterns.
pub struct ComplexityDetector;

impl ComplexityDetector {
    pub fn detect(
        functions: &[ParsedFunction],
        call_graph: &CallGraph,
        config: &DetectionConfig,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        warnings.extend(Self::detect_quadratic_loops(functions));
        warnings.extend(Self::detect_nested_loops(functions));
        warnings.extend(Self::detect_deep_call_chains(functions, call_graph, config));
        warnings.extend(Self::detect_hidden_complexity(
            functions, call_graph, config,
        ));
        warnings.extend(Self::detect_recursion(functions, call_graph));
        warnings.extend(Self::detect_large_match(functions, config));
        warnings.extend(Self::detect_deep_conditional(functions, config));
        warnings
    }

    fn detect_quadratic_loops(functions: &[ParsedFunction]) -> Vec<ComplexityWarning> {
        let loop_fns: std::collections::HashSet<&str> = functions
            .iter()
            .filter(|f| f.has_loop)
            .map(|f| f.name.as_str())
            .collect();

        let mut warnings = Vec::new();
        for f in functions {
            if !f.has_loop {
                continue;
            }
            for callee in &f.calls {
                if loop_fns.contains(callee.as_str()) {
                    warnings.push(ComplexityWarning {
                        pattern: WarningPattern::QuadraticLoop,
                        severity: WarningSeverity::Critical,
                        function: f.name.clone(),
                        location: CodeLocation::new(String::new(), f.start_line, 1),
                        message: format!(
                            "O(n²) probable: appelle {} qui contient une boucle",
                            callee
                        ),
                        suggestion: "Envisager de restructurer pour éviter la boucle externe \
                                     ou l'appel à la fonction contenant une boucle"
                            .to_string(),
                    });
                    break;
                }
            }
        }
        warnings
    }

    fn detect_nested_loops(functions: &[ParsedFunction]) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            if f.has_nested_loop {
                warnings.push(ComplexityWarning {
                    pattern: WarningPattern::NestedLoops,
                    severity: WarningSeverity::Warning,
                    function: f.name.clone(),
                    location: CodeLocation::new(String::new(), f.start_line, 1),
                    message: "boucles imbriquées détectées".to_string(),
                    suggestion: "Extraire la boucle interne dans une fonction séparée".to_string(),
                });
            }
        }
        warnings
    }

    fn detect_deep_call_chains(
        functions: &[ParsedFunction],
        call_graph: &CallGraph,
        config: &DetectionConfig,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            let depth = call_graph.call_chain_depth(&f.name);
            if depth > config.max_call_depth {
                warnings.push(ComplexityWarning {
                    pattern: WarningPattern::DeepCallChain,
                    severity: WarningSeverity::Warning,
                    function: f.name.clone(),
                    location: CodeLocation::new(String::new(), f.start_line, 1),
                    message: format!(
                        "chaîne d'appels de {} niveaux (seuil: {})",
                        depth, config.max_call_depth
                    ),
                    suggestion: "Réduire les niveaux d'indirection ou simplifier la hiérarchie \
                                 d'appels"
                        .to_string(),
                });
            }
        }
        warnings
    }

    fn detect_hidden_complexity(
        functions: &[ParsedFunction],
        call_graph: &CallGraph,
        config: &DetectionConfig,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            let caller_direct = call_graph.direct_of(&f.name) as f64;
            if caller_direct == 0.0 {
                continue;
            }
            for callee in &f.calls {
                let callee_transitive = call_graph.transitive_of(callee) as f64;
                if callee_transitive == 0.0 {
                    continue;
                }
                let ratio = callee_transitive / caller_direct;
                if ratio >= config.complexity_ratio {
                    warnings.push(ComplexityWarning {
                        pattern: WarningPattern::HiddenComplexity,
                        severity: WarningSeverity::Warning,
                        function: f.name.clone(),
                        location: CodeLocation::new(String::new(), f.start_line, 1),
                        message: format!("appelle {} qui est {:.1}x plus complexe", callee, ratio),
                        suggestion: "Extraire la logique complexe ou simplifier le callee"
                            .to_string(),
                    });
                    break;
                }
            }
        }
        warnings
    }

    fn detect_recursion(
        functions: &[ParsedFunction],
        call_graph: &CallGraph,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            if call_graph.has_cycle(&f.name) {
                warnings.push(ComplexityWarning {
                    pattern: WarningPattern::Recursion,
                    severity: WarningSeverity::Critical,
                    function: f.name.clone(),
                    location: CodeLocation::new(String::new(), f.start_line, 1),
                    message: "récursion détectée".to_string(),
                    suggestion: "Remplacer la récursion par une approche itérative".to_string(),
                });
            }
        }
        warnings
    }

    fn detect_large_match(
        functions: &[ParsedFunction],
        config: &DetectionConfig,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            if f.match_arms as usize > config.max_match_arms {
                warnings.push(ComplexityWarning {
                    pattern: WarningPattern::LargeMatch,
                    severity: WarningSeverity::Warning,
                    function: f.name.clone(),
                    location: CodeLocation::new(String::new(), f.start_line, 1),
                    message: format!(
                        "{} arms match (seuil: {})",
                        f.match_arms, config.max_match_arms
                    ),
                    suggestion: "Remplacer le grand match par une table de dispatch ou un \
                                 pattern visitor"
                        .to_string(),
                });
            }
        }
        warnings
    }

    fn detect_deep_conditional(
        functions: &[ParsedFunction],
        config: &DetectionConfig,
    ) -> Vec<ComplexityWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            if f.depth as usize > config.max_conditional_depth {
                warnings.push(ComplexityWarning {
                    pattern: WarningPattern::DeepConditional,
                    severity: WarningSeverity::Warning,
                    function: f.name.clone(),
                    location: CodeLocation::new(String::new(), f.start_line, 1),
                    message: format!(
                        "{} niveaux d'imbrication conditionnelle (seuil: {})",
                        f.depth, config.max_conditional_depth
                    ),
                    suggestion: "Extraire les blocs conditionnels dans des fonctions séparées"
                        .to_string(),
                });
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::super::call_graph::CallGraph;
    use super::super::code_parser::ParsedFunction;
    use super::*;

    fn make_fn(
        name: &str,
        decision_points: u32,
        calls: Vec<&str>,
        has_loop: bool,
        has_nested_loop: bool,
        depth: u32,
        match_arms: u32,
    ) -> ParsedFunction {
        ParsedFunction {
            name: name.to_string(),
            start_line: 1,
            calls: calls.into_iter().map(String::from).collect(),
            has_loop,
            has_nested_loop,
            decision_points,
            depth,
            match_arms,
            calls_in_loops: vec![],
        }
    }

    fn make_warning(
        function: &str,
        pattern: WarningPattern,
        severity: WarningSeverity,
        message: &str,
        suggestion: &str,
    ) -> ComplexityWarning {
        ComplexityWarning {
            pattern,
            severity,
            function: function.to_string(),
            location: super::CodeLocation::new(String::new(), 1, 1),
            message: message.to_string(),
            suggestion: suggestion.to_string(),
        }
    }

    #[test]
    fn quadratic_loop_detected() {
        let fns = vec![
            make_fn("process_items", 1, vec!["validate"], true, false, 0, 0),
            make_fn("validate", 1, vec![], true, false, 0, 0),
        ];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let quad: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::QuadraticLoop))
            .collect();
        assert_eq!(quad.len(), 1);
        assert_eq!(quad[0].function, "process_items");
        assert_eq!(quad[0].severity, WarningSeverity::Critical);
        assert!(quad[0].message.contains("validate"));
    }

    #[test]
    fn nested_loops_detected() {
        let fns = vec![make_fn("nested", 2, vec![], true, true, 0, 0)];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let nested: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::NestedLoops))
            .collect();
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].function, "nested");
        assert_eq!(nested[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn deep_call_chain_detected() {
        let fns = vec![
            make_fn("a", 1, vec!["b"], false, false, 0, 0),
            make_fn("b", 1, vec!["c"], false, false, 0, 0),
            make_fn("c", 1, vec!["d"], false, false, 0, 0),
            make_fn("d", 1, vec!["e"], false, false, 0, 0),
            make_fn("e", 1, vec!["f"], false, false, 0, 0),
            make_fn("f", 1, vec![], false, false, 0, 0),
        ];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig {
            max_call_depth: 5,
            ..DetectionConfig::default()
        };
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let deep: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::DeepCallChain))
            .collect();
        assert_eq!(deep.len(), 1);
        assert_eq!(deep[0].function, "a");
        assert_eq!(deep[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn hidden_complexity_detected() {
        let fns = vec![
            make_fn("simple", 1, vec!["complex"], false, false, 0, 0),
            make_fn("complex", 10, vec!["very_complex"], false, false, 0, 0),
            make_fn("very_complex", 10, vec![], false, false, 0, 0),
        ];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig {
            complexity_ratio: 5.0,
            ..DetectionConfig::default()
        };
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let hidden: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::HiddenComplexity))
            .collect();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].function, "simple");
        assert_eq!(hidden[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn recursion_direct_detected() {
        let fns = vec![make_fn(
            "self_call",
            1,
            vec!["self_call"],
            false,
            false,
            0,
            0,
        )];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let rec: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::Recursion))
            .collect();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].function, "self_call");
        assert_eq!(rec[0].severity, WarningSeverity::Critical);
    }

    #[test]
    fn recursion_indirect_detected() {
        let fns = vec![
            make_fn("a", 1, vec!["b"], false, false, 0, 0),
            make_fn("b", 1, vec!["a"], false, false, 0, 0),
        ];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let rec: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::Recursion))
            .collect();
        assert_eq!(rec.len(), 2);
        assert!(rec.iter().any(|w| w.function == "a"));
        assert!(rec.iter().any(|w| w.function == "b"));
    }

    #[test]
    fn large_match_detected() {
        let fns = vec![make_fn("handler", 1, vec![], false, false, 0, 15)];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig {
            max_match_arms: 10,
            ..DetectionConfig::default()
        };
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let large: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::LargeMatch))
            .collect();
        assert_eq!(large.len(), 1);
        assert_eq!(large[0].function, "handler");
        assert_eq!(large[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn deep_conditional_detected() {
        let fns = vec![make_fn("deep_cond", 1, vec![], false, false, 7, 0)];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig {
            max_conditional_depth: 5,
            ..DetectionConfig::default()
        };
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let deep: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::DeepConditional))
            .collect();
        assert_eq!(deep.len(), 1);
        assert_eq!(deep[0].function, "deep_cond");
        assert_eq!(deep[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn clean_code_no_warnings() {
        let fns = vec![make_fn("clean", 1, vec![], false, false, 2, 3)];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn detection_config_defaults() {
        let config = DetectionConfig::default();
        assert_eq!(config.max_call_depth, 5);
        assert!((config.complexity_ratio - 5.0).abs() < 1e-9);
        assert_eq!(config.max_match_arms, 10);
        assert_eq!(config.max_conditional_depth, 5);
    }

    #[test]
    fn quadratic_loop_skipped_when_callee_has_no_loop() {
        let fns = vec![
            make_fn("process_items", 1, vec!["validate"], true, false, 0, 0),
            make_fn("validate", 1, vec![], false, false, 0, 0),
        ];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig::default();
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        let quad: Vec<&ComplexityWarning> = warnings
            .iter()
            .filter(|w| matches!(w.pattern, WarningPattern::QuadraticLoop))
            .collect();
        assert!(quad.is_empty());
    }

    #[test]
    fn multiple_warnings_on_same_function() {
        let fns = vec![make_fn("messy", 1, vec![], true, true, 7, 15)];
        let graph = CallGraph::build(&fns);
        let config = DetectionConfig {
            max_match_arms: 10,
            max_conditional_depth: 5,
            ..DetectionConfig::default()
        };
        let warnings = ComplexityDetector::detect(&fns, &graph, &config);

        assert!(warnings
            .iter()
            .any(|w| matches!(w.pattern, WarningPattern::NestedLoops)));
        assert!(warnings
            .iter()
            .any(|w| matches!(w.pattern, WarningPattern::LargeMatch)));
        assert!(warnings
            .iter()
            .any(|w| matches!(w.pattern, WarningPattern::DeepConditional)));
    }
}
