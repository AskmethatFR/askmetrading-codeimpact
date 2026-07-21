// ── "Industry" design-system tokens, pruned to what the 1a (Inspector)
// layout uses (spec §4c) — steel/mono palette, Barlow / Barlow Condensed.

use std::sync::OnceLock;

use super::base64;

const BARLOW_REGULAR_WOFF2: &[u8] = include_bytes!("fonts/Barlow-Regular.latin.woff2");
const BARLOW_CONDENSED_SEMIBOLD_WOFF2: &[u8] =
    include_bytes!("fonts/BarlowCondensed-SemiBold.latin.woff2");

/// Unicode range shared by both faces (Latin + Latin-1 Supplement + the
/// handful of typographic punctuation/symbol codepoints the report itself
/// emits — arrows, em-dash range, middle dot, section punctuation).
const LATIN_UNICODE_RANGE: &str = "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,U+2074,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD";

fn barlow_regular_base64() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| base64::encode(BARLOW_REGULAR_WOFF2))
}

fn barlow_condensed_semibold_base64() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| base64::encode(BARLOW_CONDENSED_SEMIBOLD_WOFF2))
}

/// Builds the full CSS document: two embedded base64 `@font-face` blocks
/// (ADR-8.11 — Barlow 400 body, Barlow Condensed 600 heading, ~40 KB of
/// base64 added to every report) ahead of the static design-token/component
/// CSS. `include_bytes!` + a hand-rolled RFC-4648 encoder keep this at zero
/// new dependencies; `OnceLock` means each face is base64-encoded once per
/// process even across multiple `write_html` calls.
pub fn css() -> String {
    format!(
        "@font-face{{font-family:\"Barlow\";font-style:normal;font-weight:400;font-display:swap;\
         src:url(data:font/woff2;base64,{barlow}) format(\"woff2\");unicode-range:{range};}}\n\
         @font-face{{font-family:\"Barlow Condensed\";font-style:normal;font-weight:600;font-display:swap;\
         src:url(data:font/woff2;base64,{barlow_condensed}) format(\"woff2\");unicode-range:{range};}}\n\
         {css_base}",
        barlow = barlow_regular_base64(),
        barlow_condensed = barlow_condensed_semibold_base64(),
        range = LATIN_UNICODE_RANGE,
        css_base = CSS_BASE,
    )
}

const CSS_BASE: &str = r#"
:root {
  --color-bg: #f2f2f3;
  --color-surface: #e9e9ea;
  --color-text: #1d1f20;
  --color-accent: #5980a6;
  --color-divider: color-mix(in srgb, #1d1f20 16%, transparent);

  --color-neutral-100: #f5f5f8;
  --color-neutral-800: #424244;

  --color-accent-100: #eef6ff;
  --color-accent-200: #d6ebff;
  --color-accent-300: #b5d9fd;
  --color-accent-400: #94bce3;
  --color-accent-500: #749dc4;
  --color-accent-600: #597ea3;
  --color-accent-700: #416180;
  --color-accent-800: #2c455d;
  --color-accent-900: #1d2d3d;

  --font-heading: "Barlow Condensed", system-ui, sans-serif;
  --font-heading-weight: 600;
  --font-body: "Barlow", system-ui, sans-serif;

  --space-1: 3.4px;
  --space-2: 6.8px;
  --space-3: 10.2px;
  --space-4: 13.6px;
  --space-6: 20.4px;
  --space-8: 27.2px;

  --radius-sm: 2px;
  --radius-md: 4px;

  --shadow-sm: 0 1px 2px color-mix(in srgb, #2b2b2d 14%, transparent);
}

* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--color-bg);
  color: var(--color-text);
  font-family: var(--font-body);
  font-size: 15px;
  line-height: 1.55;
  padding: var(--space-8);
}
h1, h2, h3, h4 { font-family: var(--font-heading); font-weight: var(--font-heading-weight); }

.tag {
  display: inline-flex; align-items: center; font-size: 11px;
  letter-spacing: 0.02em; padding: 3px 10px; border-radius: calc(var(--radius-md) * 0.75);
}
.tag-neutral { background: var(--color-neutral-100); color: var(--color-neutral-800); }
.lvl-none { background: var(--color-neutral-100); color: var(--color-neutral-800); }
.lvl-low { background: var(--color-accent-100); color: var(--color-accent-800); }
.lvl-moderate { background: var(--color-accent-200); color: var(--color-accent-800); }
.lvl-high { background: var(--color-accent-700); color: #f2f2f3; }
.lvl-critical { background: var(--color-accent-900); color: #f2f2f3; }

.banner { display: flex; flex-direction: column; gap: var(--space-4); margin-bottom: var(--space-6); }
.banner-row { display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
.banner-title { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 23px; letter-spacing: -0.01em; }

.stat-grid {
  display: grid; grid-template-columns: repeat(9, 1fr); gap: 1px;
  background: var(--color-divider); border: 1px solid var(--color-divider);
  margin-bottom: var(--space-6);
}
.tile { background: var(--color-bg); padding: 12px 13px; }
.tile-label { font-size: 9.5px; letter-spacing: 0.08em; text-transform: uppercase; color: color-mix(in srgb, var(--color-text) 55%, transparent); }
.tile-value { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 23px; line-height: 1.15; }
.tile-sub { font-size: 10px; color: var(--color-accent); }

.blueprint { border: 1px solid var(--color-divider); }
.table { width: 100%; border-collapse: collapse; font-size: 14px; }
.table th {
  text-align: left; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase;
  color: color-mix(in srgb, var(--color-text) 60%, transparent);
  padding: var(--space-2); border-bottom: 1px solid var(--color-divider);
}
.table td { padding: var(--space-2); border-bottom: 1px solid color-mix(in srgb, var(--color-text) 8%, transparent); }
.table tbody tr:hover { background: color-mix(in srgb, var(--color-text) 4%, transparent); }

.split { display: grid; grid-template-columns: 300px 1fr; border: 1px solid var(--color-divider); }
.tree-pane {
  border-right: 1px solid var(--color-divider); padding: var(--space-3) var(--space-2);
  height: 720px; overflow: auto; background: color-mix(in srgb, var(--color-text) 2.5%, transparent);
}
.tree-heading {
  font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase;
  color: color-mix(in srgb, var(--color-text) 55%, transparent);
  margin: 2px 0 var(--space-2) 6px;
}
.tree-row {
  display: flex; align-items: center; gap: 6px; padding: 5px 6px; cursor: pointer;
}
.tree-row:hover { background: color-mix(in srgb, var(--color-text) 5%, transparent); }
.tree-row-selected { background: var(--color-accent-100); border-left: 2px solid var(--color-accent); }
.tree-indent { flex: none; display: inline-block; }
.tree-caret { width: 12px; flex: none; font-size: 11px; line-height: 1; color: var(--color-accent); }
.tree-name {
  flex: 1; min-width: 0; font-family: ui-monospace, Menlo, monospace; font-size: 12.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.tree-score { width: 22px; text-align: right; font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 12px; flex: none; }
.swatch { width: 8px; height: 8px; flex: none; }

.detail-pane { padding: var(--space-6) var(--space-6); height: 720px; overflow: auto; }
.detail-header { display: flex; align-items: flex-start; gap: var(--space-4); justify-content: space-between; margin-bottom: var(--space-6); }
.detail-kind { font-size: 10px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--color-accent); margin-bottom: 2px; }
.detail-name { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 27px; line-height: 1.08; letter-spacing: -0.015em; }
.detail-path { font-size: 12px; color: color-mix(in srgb, var(--color-text) 52%, transparent); font-family: ui-monospace, Menlo, monospace; word-break: break-all; margin-top: 2px; }
.detail-score-block { display: flex; align-items: center; gap: var(--space-4); flex: none; }
.detail-score { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 36px; line-height: 0.95; }

.metrics-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 28px; margin-bottom: var(--space-6); }
.metric-top { display: flex; justify-content: space-between; font-size: 12px; margin-bottom: 5px; }
.metric-label { color: color-mix(in srgb, var(--color-text) 66%, transparent); }
.metric-value { font-family: var(--font-heading); font-weight: var(--font-heading-weight); }
.metric-track { height: 6px; background: color-mix(in srgb, var(--color-text) 10%, transparent); }
.metric-fill { height: 100%; background: var(--color-accent-600); }

.section { margin-bottom: var(--space-6); }
.section-heading {
  font-size: 11px; letter-spacing: 0.1em; text-transform: uppercase;
  color: color-mix(in srgb, var(--color-text) 55%, transparent); margin-bottom: var(--space-2);
}

.children-list { display: flex; flex-direction: column; }
.child-row { display: flex; align-items: center; gap: 12px; padding: 8px 0; border-bottom: 1px solid color-mix(in srgb, var(--color-text) 8%, transparent); }
.child-kind { font-size: 10px; width: 46px; color: var(--color-accent); text-transform: uppercase; letter-spacing: 0.08em; flex: none; }
.child-name { flex: 1; min-width: 0; font-family: ui-monospace, Menlo, monospace; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.child-score { width: 26px; text-align: right; font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 15px; flex: none; }

.num-cell { text-align: right; font-variant-numeric: tabular-nums; }
.loc-cell { text-align: right; font-family: ui-monospace, Menlo, monospace; font-size: 11px; color: color-mix(in srgb, var(--color-text) 55%, transparent); }
.cycle-tag { margin-left: 8px; font-size: 9px; padding: 1px 6px; }

.warning-list, .io-list { display: flex; flex-direction: column; gap: 10px; }
.warning-card { padding: 12px 14px; border-left: 3px solid var(--color-divider); }
.warning-card.sev-warning { border-left-color: var(--color-accent-500); }
.warning-card.sev-critical { border-left-color: var(--color-accent-900); }
.warning-head { display: flex; align-items: center; gap: 9px; margin-bottom: 5px; }
.warning-pattern { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 15px; }
.warning-meta { margin-left: auto; font-size: 11px; font-family: ui-monospace, Menlo, monospace; color: color-mix(in srgb, var(--color-text) 52%, transparent); }
.warning-message { font-size: 13.5px; line-height: 1.5; }
.warning-suggestion { font-size: 12.5px; color: var(--color-accent-700); margin-top: 5px; }
.sev-warning { background: var(--color-accent-200); color: var(--color-accent-800); }
.sev-critical { background: var(--color-accent-900); color: #f2f2f3; }

.io-card { padding: 10px 14px; border-left: 3px solid var(--color-accent-900); display: flex; align-items: center; gap: 10px; }
.io-function, .io-call { font-family: ui-monospace, Menlo, monospace; font-size: 13px; }
.io-call { color: var(--color-accent-700); }
.io-verb { font-size: 12px; color: color-mix(in srgb, var(--color-text) 60%, transparent); }
.io-loc { margin-left: auto; font-size: 11px; font-family: ui-monospace, Menlo, monospace; color: color-mix(in srgb, var(--color-text) 52%, transparent); }

.sup-ok { background: var(--color-neutral-100); color: var(--color-neutral-800); }
.sup-degraded { background: var(--color-accent-200); color: var(--color-accent-800); }
.sup-na { background: var(--color-neutral-100); color: color-mix(in srgb, var(--color-text) 55%, transparent); }
.metric-note { font-size: 11px; color: color-mix(in srgb, var(--color-text) 55%, transparent); margin-top: 3px; }

.unmeasurable-section { padding: 12px 14px; margin-bottom: var(--space-6); }
.unmeasurable-list { display: flex; flex-direction: column; gap: 6px; }
.unmeasurable-row { display: flex; align-items: center; gap: 10px; padding: 4px 0; }
.unmeasurable-path { font-family: ui-monospace, Menlo, monospace; font-size: 13px; }
.unmeasurable-reason { margin-left: auto; font-size: 12px; color: color-mix(in srgb, var(--color-text) 60%, transparent); }

.impact-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.impact-card { padding: 16px; }
.impact-title { font-size: 10px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--color-accent); margin-bottom: 10px; }
.impact-rows { display: flex; flex-direction: column; gap: 7px; font-size: 13px; }
.impact-row { display: flex; justify-content: space-between; }
.impact-label { color: color-mix(in srgb, var(--color-text) 60%, transparent); }
.impact-value { font-family: var(--font-heading); font-weight: var(--font-heading-weight); }
.eco-body { display: flex; align-items: center; gap: 16px; }
.env-badge {
  width: 60px; height: 60px; flex: none; display: grid; place-items: center;
  font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 34px; line-height: 1;
}
.env-a { background: var(--color-accent-100); color: var(--color-accent-900); }
.env-b { background: var(--color-accent-200); color: var(--color-accent-900); }
.env-c { background: var(--color-accent-300); color: var(--color-accent-900); }
.env-d { background: var(--color-accent-400); color: var(--color-accent-900); }
.env-e { background: var(--color-accent-500); color: #f2f2f3; }
.env-f { background: var(--color-accent-700); color: #f2f2f3; }
.env-g { background: var(--color-accent-900); color: #f2f2f3; }
"#;

// ── Renderer — the load-bearing part of ADR-8.5/8.6, formalised as ADR-8.10 ──
//
// Four rules (spec §4a), mechanically enforced by
// `rendered_js_contains_no_html_sink` / `rendered_js_has_only_two_style_sinks`:
//   1. Every element is created through `el()` — the only entry point for
//      code-derived data into the DOM is `el()`'s `textContent` assignment.
//   2. Colours never become a data-built style string — they resolve
//      through closed whitelist maps (`cls()`), defaulting on an unknown key.
//   3. Exactly two numeric style sinks in the whole file (`setPct`,
//      `setIndent`), both `Number()` + `isFinite()` + clamped.
//   4. `innerHTML` / `outerHTML` / `insertAdjacentHTML` / `document.write` /
//      `setAttribute` / `eval(` / `new Function` / `javascript:` / `srcdoc` /
//      `cssText` never appear anywhere in this file.
pub const JS: &str = r#"
(function () {
  "use strict";

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = String(text);
    return n;
  }

  var LVL = { none: "lvl-none", low: "lvl-low", moderate: "lvl-moderate", high: "lvl-high", critical: "lvl-critical" };
  var SEV = { warning: "sev-warning", critical: "sev-critical" };
  var ENV = { A: "env-a", B: "env-b", C: "env-c", D: "env-d", E: "env-e", F: "env-f", G: "env-g" };
  var SUP = { supported: "sup-ok", degraded: "sup-degraded", unsupported: "sup-na" };
  function cls(map, key, fallback) {
    return Object.prototype.hasOwnProperty.call(map, key) ? map[key] : fallback;
  }

  function setPct(node, v) {
    var n = Number(v);
    if (!isFinite(n) || n < 0) n = 0;
    if (n > 100) n = 100;
    node.style.width = n + "%";
  }

  function setIndent(node, depth) {
    var d = Number(depth);
    if (!isFinite(d) || d < 0) d = 0;
    if (d > 20) d = 20;
    node.style.paddingLeft = (d * 15) + "px";
  }

  var raw = document.getElementById("ci-data").textContent;
  var data = JSON.parse(raw);
  var root = document.getElementById("ci-root");

  var byId = {};
  data.nodes.forEach(function (n) {
    byId[n.id] = n;
  });

  var state = { expanded: {}, selected: "" };
  data.nodes.forEach(function (n) {
    if (n.kind !== "file") state.expanded[n.id] = true;
  });

  function renderBanner() {
    var banner = el("div", "banner");
    var row = el("div", "banner-row");
    row.appendChild(el("div", "banner-title", data.project.target));
    row.appendChild(el("span", "tag tag-neutral", data.project.tool));
    banner.appendChild(row);
    return banner;
  }

  function renderStats() {
    var grid = el("div", "stat-grid");
    data.stats.forEach(function (s) {
      var tile = el("div", "tile");
      tile.appendChild(el("div", "tile-label", s.label));
      tile.appendChild(el("div", "tile-value", s.value));
      tile.appendChild(el("div", "tile-sub", s.sub));
      grid.appendChild(tile);
    });
    return grid;
  }

  function renderThresholds() {
    if (!data.thresholds || !data.thresholds.has_breach) return null;
    var section = el("div", "section blueprint unmeasurable-section");
    section.appendChild(
      el("div", "section-heading", "Seuils dépassés · " + data.thresholds.breaches.length)
    );
    var list = el("div", "unmeasurable-list");
    data.thresholds.breaches.forEach(function (b) {
      var row = el("div", "unmeasurable-row");
      row.appendChild(el("span", "tag sev-critical", b.metric));
      row.appendChild(
        el("span", "unmeasurable-path", "limite: " + b.limit + " · mesuré: " + b.actual)
      );
      row.appendChild(el("span", "unmeasurable-reason", "dépassement: " + b.excess));
      list.appendChild(row);
    });
    section.appendChild(list);
    return section;
  }

  function renderUnmeasurable() {
    if (!data.unmeasurable_files || data.unmeasurable_files.length === 0) return null;
    var section = el("div", "section blueprint unmeasurable-section");
    section.appendChild(
      el("div", "section-heading", "Fichiers non mesurés · " + data.unmeasurable_files.length)
    );
    var list = el("div", "unmeasurable-list");
    data.unmeasurable_files.forEach(function (f) {
      var row = el("div", "unmeasurable-row");
      row.appendChild(el("span", "tag sev-warning", "NON MESURÉ"));
      row.appendChild(el("span", "unmeasurable-path", f.path));
      row.appendChild(el("span", "unmeasurable-reason", f.reason));
      list.appendChild(row);
    });
    section.appendChild(list);
    return section;
  }

  function renderTreeRow(id, depth, container) {
    var n = byId[id];
    var isFolder = n.kind !== "file";
    var rowCls = "tree-row" + (id === state.selected ? " tree-row-selected" : "");
    var row = el("div", rowCls);

    var indent = el("span", "tree-indent");
    setIndent(indent, depth);
    row.appendChild(indent);

    row.appendChild(el("span", "tree-caret", isFolder ? (state.expanded[id] ? "▾" : "▸") : ""));
    row.appendChild(el("span", "tree-name", n.name));
    row.appendChild(el("span", "tree-score", String(n.score)));
    row.appendChild(el("span", "swatch " + cls(LVL, n.level, "lvl-low")));

    row.addEventListener("click", function () {
      if (isFolder) state.expanded[id] = !state.expanded[id];
      state.selected = id;
      renderAll();
    });

    container.appendChild(row);

    if (isFolder && state.expanded[id]) {
      n.child_ids.forEach(function (childId) {
        renderTreeRow(childId, depth + 1, container);
      });
    }
  }

  function renderTree() {
    var pane = el("div", "tree-pane");
    pane.appendChild(el("div", "tree-heading", "File tree"));
    var list = el("div", "tree-list");
    renderTreeRow("", 0, list);
    pane.appendChild(list);
    return pane;
  }

  function renderMetrics(node) {
    var grid = el("div", "metrics-grid");
    node.metrics.forEach(function (m) {
      var row = el("div");
      var top = el("div", "metric-top");
      top.appendChild(el("span", "metric-label", m.label));
      if (m.support && m.support !== "supported") {
        top.appendChild(el("span", "tag " + cls(SUP, m.support, "sup-ok"), m.support.toUpperCase()));
      }
      top.appendChild(el("span", "metric-value", m.value));
      row.appendChild(top);
      var track = el("div", "metric-track");
      var fill = el("div", "metric-fill");
      setPct(fill, m.pct);
      track.appendChild(fill);
      row.appendChild(track);
      if (m.note) row.appendChild(el("div", "metric-note", m.note));
      grid.appendChild(row);
    });
    return grid;
  }

  var KIND_LABEL = { project: "Project", folder: "Folder", file: "File" };

  function renderChildren(node) {
    if (node.child_ids.length === 0) return null;
    var section = el("div", "section");
    section.appendChild(el("div", "section-heading", "Contents · " + node.child_ids.length));
    var list = el("div", "children-list");
    node.child_ids.forEach(function (childId) {
      var c = byId[childId];
      var row = el("div", "child-row");
      row.appendChild(el("span", "child-kind", c.kind === "file" ? "FILE" : "DIR"));
      row.appendChild(el("span", "child-name", c.name));
      row.appendChild(el("span", "child-score", String(c.score)));
      row.appendChild(el("span", "swatch " + cls(LVL, c.level, "lvl-low")));
      list.appendChild(row);
    });
    section.appendChild(list);
    return section;
  }

  function renderFunctions(node) {
    if (node.functions.length === 0) return null;
    var section = el("div", "section");
    section.appendChild(el("div", "section-heading", "Functions · " + node.functions.length));
    var table = el("table", "table");
    var thead = el("thead");
    var headRow = el("tr");
    ["Function", "Direct", "Transitive", "Depth", "Location"].forEach(function (label) {
      headRow.appendChild(el("th", null, label));
    });
    thead.appendChild(headRow);
    table.appendChild(thead);
    var tbody = el("tbody");
    node.functions.forEach(function (f) {
      var row = el("tr");
      var nameCell = el("td", null, f.name);
      if (f.in_cycle) nameCell.appendChild(el("span", "tag tag-outline cycle-tag", "cycle"));
      row.appendChild(nameCell);
      row.appendChild(el("td", "num-cell", String(f.direct)));
      row.appendChild(el("td", "num-cell", String(f.transitive)));
      row.appendChild(el("td", "num-cell", String(f.depth)));
      row.appendChild(el("td", "loc-cell", f.loc));
      tbody.appendChild(row);
    });
    table.appendChild(tbody);
    section.appendChild(table);
    return section;
  }

  function renderWarnings(node) {
    if (node.warnings.length === 0) return null;
    var section = el("div", "section");
    section.appendChild(el("div", "section-heading", "Pattern warnings · " + node.warnings.length));
    var list = el("div", "warning-list");
    node.warnings.forEach(function (w) {
      var card = el("div", "blueprint warning-card " + cls(SEV, w.severity, "sev-warning"));
      var head = el("div", "warning-head");
      head.appendChild(el("span", "tag " + cls(SEV, w.severity, "sev-warning"), w.sev_label));
      head.appendChild(el("span", "warning-pattern", w.pattern));
      head.appendChild(el("span", "warning-meta", w.function + " · " + w.loc));
      card.appendChild(head);
      card.appendChild(el("div", "warning-message", w.message));
      card.appendChild(el("div", "warning-suggestion", "→ " + w.suggestion));
      list.appendChild(card);
    });
    section.appendChild(list);
    return section;
  }

  function renderIo(node) {
    // T3 (US16, #33, amends ADR-0008): an Unsupported io_in_loops capability
    // carries a non-empty io_note — render an honest n/a badge row instead
    // of silently returning null, which would read as "measured, zero
    // instances found" rather than "not supported for this language".
    if (node.io_note) {
      var naSection = el("div", "section");
      naSection.appendChild(el("div", "section-heading", "I/O in loops"));
      var naList = el("div", "io-list");
      var naCard = el("div", "blueprint io-card");
      naCard.appendChild(el("span", "tag " + cls(SUP, "unsupported", "sup-na"), "N/A"));
      naCard.appendChild(el("span", "io-verb", node.io_note));
      naList.appendChild(naCard);
      naSection.appendChild(naList);
      return naSection;
    }
    if (node.ios.length === 0) return null;
    var section = el("div", "section");
    section.appendChild(el("div", "section-heading", "I/O in loops · " + node.ios.length));
    var list = el("div", "io-list");
    node.ios.forEach(function (io) {
      var card = el("div", "blueprint io-card");
      card.appendChild(el("span", "tag sev-critical", "CRITICAL"));
      card.appendChild(el("span", "io-function", io.function));
      card.appendChild(el("span", "io-verb", "calls"));
      card.appendChild(el("span", "io-call", io.io_call));
      card.appendChild(el("span", "io-loc", io.loc));
      list.appendChild(card);
    });
    section.appendChild(list);
    return section;
  }

  function impactRow(label, value) {
    var row = el("div", "impact-row");
    row.appendChild(el("span", "impact-label", label));
    row.appendChild(el("span", "impact-value", value));
    return row;
  }

  function renderImpact(node) {
    if (!node.economic && !node.ecological) return null;
    var grid = el("div", "impact-grid");

    if (node.economic) {
      var eco = node.economic;
      var card = el("div", "blueprint impact-card");
      card.appendChild(el("div", "impact-title", "Economic impact"));
      var rows = el("div", "impact-rows");
      rows.appendChild(impactRow("CPU cost", eco.cpu));
      rows.appendChild(impactRow("Memory", eco.memory));
      rows.appendChild(impactRow("Total cost", eco.total));
      card.appendChild(rows);
      card.appendChild(el("span", "tag " + cls(LVL, eco.level, "lvl-low"), eco.level.toUpperCase()));
      grid.appendChild(card);
    }

    if (node.ecological) {
      var env = node.ecological;
      var card2 = el("div", "blueprint impact-card");
      card2.appendChild(el("div", "impact-title", "Ecological impact"));
      var body = el("div", "eco-body");
      body.appendChild(el("div", "env-badge " + cls(ENV, env.class, "env-a"), env.class));
      var rows2 = el("div", "impact-rows");
      rows2.appendChild(impactRow("CO₂", env.co2));
      rows2.appendChild(impactRow("Energy", env.energy));
      body.appendChild(rows2);
      card2.appendChild(body);
      grid.appendChild(card2);
    }

    return grid;
  }

  function renderDetail() {
    var node = byId[state.selected] || byId[""];
    var pane = el("div", "detail-pane");

    var header = el("div", "detail-header");
    var identity = el("div");
    identity.appendChild(el("div", "detail-kind", cls(KIND_LABEL, node.kind, "File")));
    identity.appendChild(el("div", "detail-name", node.name));
    identity.appendChild(el("div", "detail-path", node.path));
    header.appendChild(identity);

    var scoreBlock = el("div", "detail-score-block");
    scoreBlock.appendChild(el("div", "detail-score", String(node.score)));
    scoreBlock.appendChild(el("span", "tag " + cls(LVL, node.level, "lvl-low"), node.level.toUpperCase()));
    header.appendChild(scoreBlock);

    pane.appendChild(header);
    pane.appendChild(renderMetrics(node));

    [renderChildren(node), renderFunctions(node), renderWarnings(node), renderIo(node), renderImpact(node)].forEach(
      function (section) {
        if (section) pane.appendChild(section);
      }
    );

    return pane;
  }

  function renderSplit() {
    var split = el("div", "split");
    split.appendChild(renderTree());
    split.appendChild(renderDetail());
    return split;
  }

  function renderAll() {
    root.textContent = "";
    root.appendChild(renderBanner());
    var thresholds = renderThresholds();
    if (thresholds) root.appendChild(thresholds);
    root.appendChild(renderStats());
    var unmeasurable = renderUnmeasurable();
    if (unmeasurable) root.appendChild(unmeasurable);
    root.appendChild(renderSplit());
  }

  renderAll();
})();
"#;
