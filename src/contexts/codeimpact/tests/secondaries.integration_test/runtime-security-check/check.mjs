// #28 (ADR-8.10 hardening) — runs the REAL emitted report document (built by
// the Rust integration test from the actual `assets::JS` + `write_html`
// pipeline, then patched with an adversarial data island) through jsdom, a
// genuine HTML parser + DOM implementation. This is the one thing a Rust
// string-matching gate (html_renderer_gate_hardening_test.rs) structurally
// cannot prove: that a payload landing in the DOM as `textContent` truly
// never becomes markup, that the closed-whitelist `cls()` helper truly
// falls back on a hostile key, and that the two numeric `.style` sinks
// truly clamp — under a spec-conformant parser, not a hand-rolled one that
// would only ever validate its own assumptions.
//
// Usage: node check.mjs <path-to-html-file>
// Prints one JSON object to stdout; the Rust test does all assertions.

import { readFileSync } from "node:fs";
import { JSDOM } from "jsdom";

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error("usage: node check.mjs <path-to-html-file>");
  process.exit(2);
}

const html = readFileSync(htmlPath, "utf8");
const payloadProbe = process.env.CODEIMPACT_PAYLOAD_PROBE || null;

// The ECMAScript-standard own property names of Object.prototype (any name
// NOT in this list, found on the jsdom window's OWN Object.prototype after
// the renderer runs, is evidence of pollution).
const STANDARD_OBJECT_PROTOTYPE_KEYS = [
  "constructor",
  "hasOwnProperty",
  "isPrototypeOf",
  "propertyIsEnumerable",
  "toLocaleString",
  "toString",
  "valueOf",
  "__defineGetter__",
  "__defineSetter__",
  "__lookupGetter__",
  "__lookupSetter__",
  "__proto__",
];

const result = { run_error: null };

try {
  // `runScripts: "dangerously"` is what makes jsdom actually EXECUTE the
  // inline <script> — without it this would only prove the document
  // parses, not that the renderer runs safely against hostile data.
  const dom = new JSDOM(html, {
    runScripts: "dangerously",
    url: "http://localhost/report.html",
  });
  const { window } = dom;
  const { document } = window;

  result.img_tag_count = document.querySelectorAll("img").length;
  result.script_tag_count = document.querySelectorAll("script").length;
  result.iframe_tag_count = document.querySelectorAll("iframe").length;

  // Prototype pollution must be checked in the jsdom window's OWN realm —
  // it has its own Object distinct from this outer Node process's Object,
  // so the report JS (which runs inside that realm) could only ever
  // pollute window.Object.prototype, never this process's.
  const extraProtoKeys = window.Object.getOwnPropertyNames(
    window.Object.prototype
  ).filter((k) => !STANDARD_OBJECT_PROTOTYPE_KEYS.includes(k));
  result.prototype_polluted = extraProtoKeys.length > 0;
  result.extra_prototype_keys = extraProtoKeys;

  // `Object.keys` only lists a plain object's OWN properties, so it would
  // never see pollution living on the prototype chain — `for...in` DOES
  // walk inherited enumerable properties, which is exactly the symptom a
  // classic `Object.prototype.polluted = true` assignment produces.
  const strayEnumerableKeys = [];
  for (const key in new window.Object()) {
    strayEnumerableKeys.push(key);
  }
  result.plain_object_has_stray_keys = strayEnumerableKeys.length > 0;
  result.stray_enumerable_keys = strayEnumerableKeys;

  // Root is always the first row `renderTree()` appends (renderTreeRow("",
  // 0, list) runs before it recurses into children).
  const swatches = Array.from(document.querySelectorAll(".swatch"));
  result.first_swatch_class = swatches.length > 0 ? swatches[0].className : null;

  // Only the crafted hostile-support metric has support !== "supported", so
  // exactly one .tag should exist inside .metrics-grid.
  const supportTags = Array.from(
    document.querySelectorAll(".metrics-grid .tag")
  );
  result.metrics_grid_tag_count = supportTags.length;
  result.support_tag_class = supportTags.length > 0 ? supportTags[0].className : null;

  const fills = Array.from(document.querySelectorAll(".metric-fill"));
  result.metric_fill_widths = fills.map((el) => el.style.width);

  const indents = Array.from(document.querySelectorAll(".tree-indent"));
  result.max_indent_padding_left_px = indents
    .map((el) => parseFloat(el.style.paddingLeft) || 0)
    .reduce((max, v) => Math.max(max, v), 0);

  result.body_text_contains_payload_literally = payloadProbe
    ? document.body.textContent.includes(payloadProbe)
    : null;

  result.pwned = Boolean(window.__pwned__);
} catch (err) {
  result.run_error = String(err && err.stack ? err.stack : err);
}

process.stdout.write(JSON.stringify(result));
