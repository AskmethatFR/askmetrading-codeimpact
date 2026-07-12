// ── "Industry" design-system tokens, pruned to what the 1a (Inspector)
// layout uses (spec §4c) — steel/mono palette, Barlow / Barlow Condensed.

pub const CSS: &str = r#"
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
.lvl-low { background: var(--color-accent-100); color: var(--color-accent-800); }
.lvl-moderate { background: var(--color-accent-200); color: var(--color-accent-800); }
.lvl-high { background: var(--color-accent-700); color: #f2f2f3; }
.lvl-critical { background: var(--color-accent-900); color: #f2f2f3; }

.banner { display: flex; flex-direction: column; gap: var(--space-4); margin-bottom: var(--space-6); }
.banner-row { display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
.banner-title { font-family: var(--font-heading); font-weight: var(--font-heading-weight); font-size: 23px; letter-spacing: -0.01em; }

.stat-grid {
  display: grid; grid-template-columns: repeat(8, 1fr); gap: 1px;
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

.metrics-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 28px; }
.metric-top { display: flex; justify-content: space-between; font-size: 12px; margin-bottom: 5px; }
.metric-label { color: color-mix(in srgb, var(--color-text) 66%, transparent); }
.metric-value { font-family: var(--font-heading); font-weight: var(--font-heading-weight); }
.metric-track { height: 6px; background: color-mix(in srgb, var(--color-text) 10%, transparent); }
.metric-fill { height: 100%; background: var(--color-accent-600); }
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

  var LVL = { low: "lvl-low", moderate: "lvl-moderate", high: "lvl-high", critical: "lvl-critical" };
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
      top.appendChild(el("span", "metric-value", m.value));
      row.appendChild(top);
      var track = el("div", "metric-track");
      var fill = el("div", "metric-fill");
      setPct(fill, m.pct);
      track.appendChild(fill);
      row.appendChild(track);
      grid.appendChild(row);
    });
    return grid;
  }

  var KIND_LABEL = { project: "Project", folder: "Folder", file: "File" };

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
    root.appendChild(renderStats());
    root.appendChild(renderSplit());
  }

  renderAll();
})();
"#;
