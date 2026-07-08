use crate::domain_model::{
    AnalysisRule, AnalysisTarget, CodeMetrics, CodeLocation, AnalysisError,
};

pub struct ProactiveAnalyzer;

impl ProactiveAnalyzer {
    pub fn analyze(source: &str, target: &AnalysisTarget, rules: &[AnalysisRule]) -> Result<CodeMetrics, AnalysisError> {
        let mut complexity = 0u32;
        let mut io_loops = Vec::new();
        let mut depth = 0u32;
        let mut allocs = Vec::new();

        for rule in rules {
            match rule {
                AnalysisRule::CyclomaticComplexity => {
                    complexity = count_cyclomatic(source);
                }
                AnalysisRule::IoInLoops => {
                    io_loops = detect_io_in_loops(source, target);
                }
                AnalysisRule::NestedDepth => {
                    depth = max_nested_depth(source);
                }
                AnalysisRule::AllocationHotspots => {
                    allocs = detect_allocation_hotspots(source, target);
                }
            }
        }

        Ok(CodeMetrics::new(complexity, io_loops, depth, allocs))
    }
}

fn count_cyclomatic(source: &str) -> u32 {
    let mut count = 1u32;
    for line in source.lines() {
        let t = line.trim();
        if t.starts_with("if ") || t.starts_with("else if ") || t.starts_with("while ")
            || t.starts_with("for ") || t.starts_with("match ") || t.starts_with("catch ")
            || t.contains("&&") || t.contains("||")
        {
            count += 1;
        }
    }
    count
}

fn detect_io_in_loops(source: &str, target: &AnalysisTarget) -> Vec<CodeLocation> {
    let mut results = Vec::new();
    let mut in_loop = false;

    for (line_num, line) in source.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("for ") || t.starts_with("while ") || t.starts_with("loop ") {
            in_loop = true;
            continue;
        }
        if in_loop && (t.contains("std::fs::") || t.contains("tokio::fs::")
            || t.contains("std::net::") || t.contains("reqwest::")
            || t.contains("std::io::"))
        {
            if let Ok(loc) = CodeLocation::new(
                target.path().clone(),
                line_num + 1,
                t.find(|c: char| !c.is_whitespace()).unwrap_or(0) + 1,
            ) {
                results.push(loc);
            }
        }
        if t == "}" {
            in_loop = false;
        }
    }
    results
}

fn max_nested_depth(source: &str) -> u32 {
    let mut max_depth = 0u32;
    let mut current = 0u32;
    for line in source.lines() {
        let t = line.trim();
        if t.starts_with("for ") || t.starts_with("while ") || t.starts_with("if ")
            || t.starts_with("match ") || t.starts_with("loop ")
        {
            current += 1;
            max_depth = max_depth.max(current);
        }
        if t == "}" {
            current = current.saturating_sub(1);
        }
    }
    max_depth
}

fn detect_allocation_hotspots(source: &str, target: &AnalysisTarget) -> Vec<CodeLocation> {
    let mut results = Vec::new();
    for (line_num, line) in source.lines().enumerate() {
        let t = line.trim();
        if t.contains("Box::new") || t.contains("vec!") || t.contains("HashMap::new")
            || t.contains("String::new") || t.contains("format!")
        {
            if let Ok(loc) = CodeLocation::new(
                target.path().clone(),
                line_num + 1,
                t.find(|c: char| !c.is_whitespace()).unwrap_or(0) + 1,
            ) {
                results.push(loc);
            }
        }
    }
    results
}