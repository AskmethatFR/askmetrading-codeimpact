; TypeScript/JavaScript dependency-extraction query (US17 T4.3) — the
; ecmascript-family counterpart to `csharp_deps.scm`. One capture name only:
;   @import — a specifier this file DEPENDS ON, captured from three literal-
;             only sites: `import ... from '<spec>'`, `export ... from
;             '<spec>'`, and `require('<spec>')`. The Rust side
;             (`resolve_dependencies` / `extract_deps`) extracts the
;             literal's text from its `string_fragment` child — never the
;             raw `string` node text, which still carries the surrounding
;             quotes — and abstains (no edge, no error) on anything it
;             cannot read safely: zero or multiple `string_fragment`
;             children (an `escape_sequence` is a SIBLING of the fragment,
;             never nested inside it, and splits or shortens the fragment
;             count/span the same way), or a single fragment that does not
;             span the whole quoted content (AD-8 — abstain, never guess,
;             never fail).
;
; `import x = require('./y')` (the legacy TypeScript form, whose target
; string sits on the nested `import_require_clause` node, NOT on
; `import_statement` itself) is deliberately NOT captured here:
; `import_require_clause` exists ONLY in the TypeScript grammar, and this
; single query file is shared by both grammars (AD-7) — referencing it would
; make `Query::new` panic the moment this query runs against the plain
; JavaScript grammar. Out of scope for T4.3.
;
; `require(...)` is a plain function call, not a declaration: its site is a
; `call_expression` whose callee is the identifier `require` (guarded by the
; `#eq?` predicate below) and whose FIRST argument is a string literal — the
; leading `.` anchor forces `arguments`' first child to be the string, so
; `require(cfg, './x')` does not match. A computed argument (`require(name)`,
; a template string, string concatenation, or any other non-literal
; expression) simply matches none of these patterns — there is no guard to
; write here, only a query shape not to widen (AD-8). A leading COMMENT
; before the string argument (`require(/* c */ './x')`) also fails the `.`
; anchor and abstains: this grammar's `comment` is a named "extra" node that
; can appear before the real first argument (confirmed against the same
; grammar quirk the IIFE-attribution tests below already document), so it
; counts as `arguments`' actual first named child, not the string.

(import_statement source: (string) @import)

(export_statement source: (string) @import)

(call_expression
  function: (identifier) @_callee
  arguments: (arguments . (string) @import)
  (#eq? @_callee "require"))
