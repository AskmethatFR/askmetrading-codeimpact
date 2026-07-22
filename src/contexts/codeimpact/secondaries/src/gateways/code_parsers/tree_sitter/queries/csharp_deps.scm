; C# dependency-extraction query (US16 T5) — captures the constructs the
; namespace-index builder (tree_sitter_code_parser.rs) turns into a
; `namespace -> declaring-files` index, and every file's own `using`
; directives resolve through. Two capture names only:
;   @namespace — a namespace this file DECLARES (`namespace X { ... }` or
;                 the C# 10 file-scoped form `namespace X;`).
;   @using     — a namespace this file DEPENDS ON via a `using` directive.
;                 The captured node is the WHOLE using_directive; its
;                 actual namespace text is extracted by the adapter (the
;                 grammar gives the target path no field name of its own —
;                 see `using_target_text`).

(namespace_declaration) @namespace
(file_scoped_namespace_declaration) @namespace

(using_directive) @using
