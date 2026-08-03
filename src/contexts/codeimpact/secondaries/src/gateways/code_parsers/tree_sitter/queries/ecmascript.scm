; TypeScript/JavaScript metric-extraction query (US17 T1) — shared by both
; grammars (Q8: one file, split into typescript.scm/javascript.scm only if
; a node-kind divergence is ever proven; the compile test in
; tree_sitter_code_parser.rs's test suite is the arbiter). Mirrors
; csharp.scm's structure and comment discipline: this file only says WHAT
; to find, the range-containment post-processor in
; tree_sitter_code_parser.rs still tells constructs apart by node kind
; where one capture name groups several (e.g. @branch.arm covers
; switch_case/switch_default/if_statement — one feeds branch_arms AND
; decision_points, if_statement only feeds decision_points).

(function_declaration) @function
(function_expression) @function
(generator_function_declaration) @function
(generator_function) @function
(arrow_function) @function
(method_definition) @function

(for_statement) @loop
(for_in_statement) @loop
(while_statement) @loop
(do_statement) @loop

(if_statement) @branch.arm
(switch_case) @branch.arm
(switch_default) @branch.arm

(binary_expression
  operator: ["&&" "||" "??"]) @conditional
(ternary_expression) @conditional

; `?.` (optional chaining) is deliberately NOT captured here (human-approved
; Q6, deferred to issue #117, which will add it to C# AND TS/JS
; simultaneously). Counting it in TS/JS only, ahead of C#, would break the
; cross-language comparability invariant ADR-0020 D4 relies on and shift
; the meaning of ADR-0017's thresholds — this omission is a decision, not
; an oversight.

(call_expression) @call
