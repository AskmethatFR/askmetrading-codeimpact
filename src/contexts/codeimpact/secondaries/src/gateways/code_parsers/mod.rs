pub mod code_parser_stub;

#[cfg(feature = "syn-parser")]
pub mod syn_code_parser;

#[cfg(any(feature = "lang-csharp", feature = "lang-typescript"))]
pub mod tree_sitter;
