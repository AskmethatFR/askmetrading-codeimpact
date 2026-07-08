/// Enum — règle d'analyse applicable.
/// Pour l'instant, seule la complexité cyclomatique est supportée.
#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisRule {
    CyclomaticComplexity,
}
