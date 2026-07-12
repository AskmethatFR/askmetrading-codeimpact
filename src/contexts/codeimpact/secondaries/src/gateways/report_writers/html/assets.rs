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
.path-cell { font-family: ui-monospace, Menlo, monospace; word-break: break-all; }
.bar-track { display: inline-block; height: 6px; width: 100px; background: color-mix(in srgb, var(--color-text) 10%, transparent); vertical-align: middle; }
.bar-fill { display: block; height: 100%; background: var(--color-accent-600); }
.score-value { display: inline-block; margin-left: 8px; font-variant-numeric: tabular-nums; }
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

  var raw = document.getElementById("ci-data").textContent;
  var data = JSON.parse(raw);
  var root = document.getElementById("ci-root");

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

  function renderFileTable() {
    var wrap = el("div", "blueprint");
    var table = el("table", "table");
    var thead = el("thead");
    var headRow = el("tr");
    ["File", "Score", "Level"].forEach(function (label) {
      headRow.appendChild(el("th", null, label));
    });
    thead.appendChild(headRow);
    table.appendChild(thead);

    var tbody = el("tbody");
    data.files.forEach(function (file) {
      var row = el("tr");

      row.appendChild(el("td", "path-cell", file.path));

      var scoreCell = el("td");
      var track = el("span", "bar-track");
      var fill = el("span", "bar-fill");
      setPct(fill, file.score_pct);
      track.appendChild(fill);
      scoreCell.appendChild(track);
      scoreCell.appendChild(el("span", "score-value", String(file.score)));
      row.appendChild(scoreCell);

      var levelCell = el("td");
      levelCell.appendChild(el("span", "tag " + cls(LVL, file.level_label, "lvl-low"), file.level_label.toUpperCase()));
      row.appendChild(levelCell);

      tbody.appendChild(row);
    });
    table.appendChild(tbody);
    wrap.appendChild(table);
    return wrap;
  }

  root.appendChild(renderBanner());
  root.appendChild(renderStats());
  root.appendChild(renderFileTable());
})();
"#;
