use super::analysis_rule::AnalysisRule;
use super::code_metrics::CodeMetrics;
use super::errors::AnalysisError;

/// Service domaine — analyse proactive du code source.
/// Calcule la complexité cyclomatique en comptant les points de décision.
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

/// Compte les points de décision dans le code source.
fn count_decision_points(source: &str) -> u32 {
    let mut count = 0u32;

    for line in source.lines() {
        let trimmed = line.trim();

        // Évite les commentaires et chaînes vides
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        // Compter chaque mot-clé de décision
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

/// Compte les occurrences d'un mot-clé dans une ligne de code.
/// Évite les faux positifs en vérifiant les limites de mots.
fn count_keyword(line: &str, keyword: &str) -> u32 {
    let mut count = 0u32;
    let mut start = 0;

    while let Some(pos) = line[start..].find(keyword) {
        let abs_pos = start + pos;
        // Pour 'if', on vérifie que ce n'est pas déjà compté dans 'else if'
        if keyword == "if" && abs_pos >= 5 && line[abs_pos - 5..abs_pos].trim_end() == "else" {
            start = abs_pos + keyword.len();
            continue;
        }
        // Vérification: le mot-clé est bien un mot entier (pas un sous-mot)
        if is_whole_word(line, abs_pos, keyword.len()) {
            count += 1;
        }
        start = abs_pos + keyword.len();
    }

    count
}

/// Vérifie que le mot-clé est un mot entier (entouré de non-alphanumériques).
fn is_whole_word(line: &str, pos: usize, len: usize) -> bool {
    // Vérifier le caractère avant
    if pos > 0 {
        let prev = line.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return false;
        }
    }
    // Vérifier le caractère après
    let after = pos + len;
    if after < line.len() {
        let next = line.as_bytes()[after];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_decision_points_none() {
        assert_eq!(count_decision_points("fn test() { let x = 1; }"), 0);
    }

    #[test]
    fn count_decision_points_one_if() {
        assert_eq!(count_decision_points("fn test() { if x > 0 { } }"), 1);
    }

    #[test]
    fn count_decision_points_else_if() {
        assert_eq!(count_decision_points("if x > 0 { } else if x < 0 { }"), 2);
    }

    #[test]
    fn count_decision_points_and() {
        assert_eq!(count_decision_points("if x > 0 && y > 0 { }"), 2);
    }
}
