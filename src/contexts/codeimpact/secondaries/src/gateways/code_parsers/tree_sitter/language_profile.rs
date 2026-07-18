/// Everything `TreeSitterCodeParser` needs to parse one grammar: the
/// compiled `tree-sitter` language, the `.scm` query that captures the
/// constructs the range-containment post-processor turns into
/// `ParsedFunction`s, and the (currently unused — T4 seam) I/O signature
/// table. One profile per language keeps the parser itself grammar-
/// agnostic — a future TypeScript grammar is a second `LanguageProfile`,
/// not a second parser type.
pub struct LanguageProfile {
    pub grammar: tree_sitter::Language,
    pub scm: &'static str,
    pub io_table: &'static [&'static str],
}
