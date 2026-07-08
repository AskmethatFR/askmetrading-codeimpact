use super::analysis_rule::AnalysisRule;
use super::code_metrics::CodeMetrics;
use super::errors::AnalysisError;

pub fn analyze(source: &str, rules: &[AnalysisRule]) -> Result<CodeMetrics, AnalysisError> {
    let mut complexity = 1u32;

    for rule in rules {
        match rule {
            AnalysisRule::CyclomaticComplexity => {
                complexity += count_decision_points(source);
            }
        }
    }

    Ok(CodeMetrics::new(complexity))
}

fn count_decision_points(source: &str) -> u32 {
    let mut count = 0u32;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        count += count_keyword(trimmed, "else if");
        count += count_keyword(trimmed, "if");
        count += count_keyword(trimmed, "while");
        count += count_keyword(trimmed, "for");
        count += count_keyword(trimmed, "match");
        count += count_keyword(trimmed, "catch");
        count += count_keyword(trimmed, "&&");
        count += count_keyword(trimmed, "||");
    }

    count
}

fn count_keyword(line: &str, keyword: &str) -> u32 {
    let mut count = 0u32;
    let mut start = 0;

    while let Some(pos) = line[start..].find(keyword) {
        let abs_pos = start + pos;
        if keyword == "if" && abs_pos >= 5 && line[abs_pos - 5..abs_pos].trim_end() == "else" {
            start = abs_pos + keyword.len();
            continue;
        }
        if is_whole_word(line, abs_pos, keyword.len()) {
            count += 1;
        }
        start = abs_pos + keyword.len();
    }

    count
}

fn is_whole_word(line: &str, pos: usize, len: usize) -> bool {
    if pos > 0 {
        let prev = line.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return false;
        }
    }
    let after = pos + len;
    if after < line.len() {
        let next = line.as_bytes()[after];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return false;
        }
    }
    true
}
