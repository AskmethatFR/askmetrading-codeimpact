/// Everything `TreeSitterCodeParser` needs to parse one grammar: the
/// compiled `tree-sitter` language, the `.scm` query that captures the
/// constructs the range-containment post-processor turns into
/// `ParsedFunction`s, and the confident I/O prefixes (US16 T4.1) fed to
/// `classify_csharp_call`. Owned (`Vec<String>`), not `&'static`, because
/// T4.3 appends user-configured prefixes at construction time — a runtime
/// list, not a compile-time constant. One profile per language keeps the
/// parser itself grammar-agnostic — a future TypeScript grammar is a second
/// `LanguageProfile`, not a second parser type.
pub struct LanguageProfile {
    pub grammar: tree_sitter::Language,
    pub scm: &'static str,
    pub io_table: Vec<String>,
}
