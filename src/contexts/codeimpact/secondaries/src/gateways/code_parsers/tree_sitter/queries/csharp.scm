; C# metric-extraction query (US16 T2) — captures the constructs the
; range-containment post-processor (tree_sitter_code_parser.rs) assigns to
; their innermost enclosing @function by byte range. Capture NAMES group
; constructs the post-processor still tells apart by node kind (e.g.
; @branch.arm covers both a switch's `switch_section` and an `if_statement`
; — one feeds `branch_arms` AND `decision_points`, the other only
; `decision_points`); this file only says WHAT to find, never how to score
; it.

(method_declaration) @function
(local_function_statement) @function
(constructor_declaration) @function

(for_statement) @loop
(foreach_statement) @loop
(while_statement) @loop
(do_statement) @loop

(switch_section) @branch.arm
(if_statement) @branch.arm

(binary_expression
  operator: ["&&" "||"]) @conditional
(conditional_expression) @conditional

(invocation_expression) @call
