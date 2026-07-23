// #28 (ADR-8.10 hardening) — the ADR-8.10 structural gates
// (`rendered_js_contains_no_html_sink`, `rendered_js_has_only_two_style_sinks`
// in `html_report_writer_test.rs`) originally matched literal substrings
// only. A sink reached via string-literal concatenation
// (`"inner" + "HTML"`) or bracket-notation / bulk-setter property access
// (`node["style"][computedProp]`, `Object.assign(node.style, …)`, a bare
// `node.style = …`) slipped through undetected — demonstrated by the
// Security audit on #27. The shipped JS uses none of these patterns (Do NOT
// weaken the real emitted JS); this file proves the GATE itself now catches
// them, by appending a fixture bypass to a copy of the real emitted JS and
// asserting the gate fails — then proving the same gate stays clean on the
// unmodified original, in the same test, so every case discriminates in
// both directions.
//
// Test List:
// 1. html_sink gate catches "inner"+"HTML" built via string concatenation
//    (ticket's exact demonstrated bypass #2)
// 2. style_sink gate catches ["style"][styleProp] where styleProp is built
//    via "css"+"Text" concatenation (ticket's exact demonstrated bypass #1);
//    the html_sink gate also catches it independently (cssText reconstructed
//    via concatenation)
// 3. style_sink gate catches `.style[dynamicProp]` bracket notation with NO
//    banned literal anywhere in the snippet — proves the catch is genuinely
//    structural, not an accidental literal match
// 4. style_sink gate catches `Object.assign(el.style, {...})` bulk-set, no
//    banned literal
// 5. style_sink gate catches a bare `el.style = value` reassignment, no
//    banned literal

use codeimpact_secondaries::gateways::report_writers::html::assets;
use codeimpact_secondaries_integration_test::rendering_gate::{
    html_sink_violations, style_sink_violations,
};

#[test]
fn html_sink_gate_catches_the_demonstrated_inner_html_concatenation_bypass() {
    let bypass = "\nvar sinkName = \"inner\" + \"HTML\"; document.getElementById(\"x\")[sinkName] = hostileMarkup;\n";
    let bypassed_js = format!("{}{}", assets::JS, bypass);

    assert!(
        html_sink_violations(assets::JS).is_empty(),
        "the real emitted JS must stay clean before the bypass is appended"
    );

    let violations = html_sink_violations(&bypassed_js);
    assert!(
        !violations.is_empty(),
        "expected the html-sink gate to catch 'inner' + 'HTML' concatenation, got no violations"
    );
    assert!(
        violations.iter().any(|v| v.contains("innerHTML")),
        "expected a violation mentioning innerHTML, got: {:?}",
        violations
    );
}

#[test]
fn style_sink_gate_catches_the_demonstrated_css_text_bracket_and_concatenation_bypass() {
    let bypass = "\nvar styleProp = \"css\" + \"Text\"; document.getElementById(\"x\")[\"style\"][styleProp] = hostileCss;\n";
    let bypassed_js = format!("{}{}", assets::JS, bypass);

    assert!(
        style_sink_violations(assets::JS).is_empty(),
        "the real emitted JS must stay clean before the bypass is appended"
    );

    let style_violations = style_sink_violations(&bypassed_js);
    assert!(
        !style_violations.is_empty(),
        "expected the style-sink gate to catch [\"style\"][...] bracket notation, got no violations"
    );

    let html_violations = html_sink_violations(&bypassed_js);
    assert!(
        html_violations.iter().any(|v| v.contains("cssText")),
        "expected the html-sink gate to independently catch 'css' + 'Text' concatenation, got: {:?}",
        html_violations
    );
}

#[test]
fn style_sink_gate_catches_dot_bracket_notation_with_no_banned_literal_present() {
    let bypass = "\nvar prop = pickHostileProp(); document.getElementById(\"x\").style[prop] = hostileCss;\n";
    let bypassed_js = format!("{}{}", assets::JS, bypass);

    assert!(
        style_sink_violations(assets::JS).is_empty(),
        "the real emitted JS must stay clean before the bypass is appended"
    );
    assert!(
        !style_sink_violations(&bypassed_js).is_empty(),
        "expected the style-sink gate to catch bare '.style[' bracket notation"
    );
    assert!(
        html_sink_violations(&bypassed_js).is_empty(),
        "this bypass carries no banned literal or reconstructible identifier — the html-sink \
         gate must stay quiet, proving the catch above is genuinely structural, not an \
         accidental literal match"
    );
}

#[test]
fn style_sink_gate_catches_object_assign_bulk_style_set() {
    let bypass =
        "\nObject.assign(document.getElementById(\"x\").style, { width: dynamicWidth });\n";
    let bypassed_js = format!("{}{}", assets::JS, bypass);

    assert!(
        style_sink_violations(assets::JS).is_empty(),
        "the real emitted JS must stay clean before the bypass is appended"
    );
    assert!(
        !style_sink_violations(&bypassed_js).is_empty(),
        "expected the style-sink gate to catch Object.assign( bulk-setting .style"
    );
}

#[test]
fn style_sink_gate_catches_bare_style_reassignment() {
    let bypass = "\ndocument.getElementById(\"x\").style = dynamicStyleValue;\n";
    let bypassed_js = format!("{}{}", assets::JS, bypass);

    assert!(
        style_sink_violations(assets::JS).is_empty(),
        "the real emitted JS must stay clean before the bypass is appended"
    );
    assert!(
        !style_sink_violations(&bypassed_js).is_empty(),
        "expected the style-sink gate to catch a bare `.style =` reassignment"
    );
}
