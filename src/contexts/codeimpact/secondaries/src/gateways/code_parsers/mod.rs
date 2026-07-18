pub mod code_parser_stub;

#[cfg(feature = "syn-parser")]
pub mod syn_code_parser;

#[cfg(feature = "lang-csharp")]
pub mod tree_sitter;
