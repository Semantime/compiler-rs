#!/usr/bin/env python3

from __future__ import annotations

import argparse
import html
import json
import math
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COMPILER_BIN = ROOT / "target" / "debug" / "compiler-rs"
DEFAULT_OUTPUT = ROOT / "tools" / "regression_viewer_py" / "generated" / "regression_viewer.html"

EVENT_COLORS = {
    "sustained_high": "#b42318",
    "sustained_low": "#1849a9",
    "spike": "#ea580c",
    "drop": "#0f766e",
    "regime_shift": "#7c3aed",
    "oscillation": "#c2410c",
    "increasing_trend": "#1d4ed8",
    "decreasing_trend": "#0f766e",
    "peer_imbalance": "#b45309",
}

SERIES_PALETTE = [
    "#2563eb",
    "#dc2626",
    "#16a34a",
    "#9333ea",
    "#ea580c",
    "#0891b2",
    "#475569",
    "#be185d",
]

QUICK_VIEWS = [
    ("all", "All"),
    ("spike", "Spike/Drop"),
    ("shift", "Regime Shift"),
    ("volatile", "Oscillation"),
    ("failures", "Failures"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description="Render regression viewer HTML from Rust viewer-json.")
    parser.add_argument(
        "--compiler-bin",
        type=Path,
        default=DEFAULT_COMPILER_BIN,
        help="Path to the built compiler-rs binary.",
    )
    parser.add_argument(
        "--cases-dir",
        type=Path,
        default=None,
        help="Optional regression cases directory.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="HTML output path.",
    )
    parser.add_argument(
        "--no-open",
        action="store_true",
        help="Do not open the generated HTML automatically.",
    )
    args = parser.parse_args()

    envelope = load_viewer_data(args.compiler_bin, args.cases_dir)
    output_path = args.output.resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(render_html(envelope), encoding="utf-8")

    print(output_path)
    if not args.no_open:
        subprocess.run(["open", str(output_path)], check=False)

    return 0


def load_viewer_data(compiler_bin: Path, cases_dir: Path | None) -> dict[str, Any]:
    command = [str(compiler_bin), "viewer-json"]
    if cases_dir is not None:
        command.append(str(cases_dir))
    completed = subprocess.run(command, capture_output=True, text=True, check=True, cwd=ROOT)
    return json.loads(completed.stdout)


def render_html(envelope: dict[str, Any], *, live_preview: bool = False, server_url: str = "") -> str:
    data = envelope.get("data", envelope)
    report = data["report"]
    cases = [build_case_view(index, case) for index, case in enumerate(data["cases"])]
    tag_rates = build_tag_rates(cases)
    generated_at = datetime.now(timezone.utc).astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
    viewer_model = {
        "cases": cases,
        "tag_options": sorted({tag for case in cases for tag in case["tags"]}),
    }

    quick_view_buttons = "".join(
        f'<button type="button" class="quick-view-button{" active" if value == "all" else ""}" '
        f'data-quick-view="{html.escape(value)}">{html.escape(label)}</button>'
        for value, label in QUICK_VIEWS
    )
    refresh_badge = (
        f"<p class=\"live-preview-note\">Live preview: polling every 2s from {html.escape(server_url or '/viewer.json')}.</p>"
        if live_preview
        else ""
    )
    live_script = (
        """
  <script>
    setInterval(() => {
      const detailLayer = document.getElementById("detail-layer");
      const drawerOpen = detailLayer && !detailLayer.classList.contains("hidden");
      if (drawerOpen || document.hidden) {
        return;
      }
      window.location.reload();
    }, 2000);
  </script>
"""
        if live_preview
        else ""
    )
    footer_text = (
        "Live preview entrypoint: make viewer. This page reloads automatically."
        if live_preview
        else "Static HTML export generated from Rust regression output."
    )

    html_template = """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Compiler Benchmark Console</title>
  <style>
    :root {
      --page-bg: #f6f8fb;
      --panel-bg: #ffffff;
      --panel-soft: #f8fafc;
      --ink: #0f172a;
      --muted: #64748b;
      --line: #e2e8f0;
      --accent: #2563eb;
      --accent-soft: #eff6ff;
      --pass: #166534;
      --pass-soft: #dcfce7;
      --fail: #b91c1c;
      --fail-soft: #fee2e2;
      --warn: #b45309;
      --warn-soft: #ffedd5;
      --shadow: 0 1px 2px rgba(15, 23, 42, 0.04), 0 8px 24px rgba(15, 23, 42, 0.04);
      --radius-lg: 18px;
      --radius-md: 12px;
      --radius-sm: 10px;
    }

    * { box-sizing: border-box; }
    html, body { margin: 0; padding: 0; }
    body {
      background:
        radial-gradient(circle at top left, rgba(37, 99, 235, 0.08), transparent 28%),
        radial-gradient(circle at top right, rgba(14, 165, 233, 0.08), transparent 24%),
        var(--page-bg);
      color: var(--ink);
      font-family: "Avenir Next", "Helvetica Neue", "Segoe UI", sans-serif;
    }

    .shell {
      width: min(1500px, calc(100vw - 32px));
      margin: 0 auto;
      padding: 28px 0 40px;
    }

    .hero {
      margin-bottom: 18px;
    }

    .hero-title {
      margin: 0 0 8px;
      font-size: 34px;
      line-height: 1.08;
      letter-spacing: -0.03em;
      font-weight: 750;
    }

    .hero-subtitle {
      margin: 0;
      color: var(--muted);
      font-size: 15px;
      line-height: 1.6;
      max-width: 960px;
    }

    .hero-subtitle code,
    .detail-title code,
    .results-table code {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.95em;
    }

    .live-preview-note {
      margin: 10px 0 0;
      color: var(--muted);
      font-size: 14px;
    }

    .summary-grid,
    .tag-rate-row {
      display: grid;
      gap: 14px;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      margin-bottom: 16px;
    }

    .metric-card,
    .tag-rate-card,
    .controls-card,
    .results-card,
    .section-card {
      background: var(--panel-bg);
      border: 1px solid var(--line);
      border-radius: var(--radius-lg);
      box-shadow: var(--shadow);
    }

    .metric-card,
    .tag-rate-card {
      padding: 18px 18px 16px;
      min-height: 110px;
    }

    .metric-label,
    .tag-rate-label,
    .section-label,
    .field-label {
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }

    .metric-value,
    .tag-rate-value {
      margin-top: 10px;
      font-size: 30px;
      line-height: 1.08;
      font-weight: 750;
      letter-spacing: -0.03em;
    }

    .metric-meta,
    .tag-rate-meta,
    .summary-path,
    .results-meta,
    .empty-note {
      margin-top: 8px;
      color: var(--muted);
      font-size: 13px;
      line-height: 1.45;
    }

    .controls-card {
      padding: 18px;
      margin-bottom: 16px;
    }

    .controls-header {
      display: flex;
      justify-content: space-between;
      gap: 16px;
      align-items: baseline;
      margin-bottom: 14px;
      flex-wrap: wrap;
    }

    .controls-title {
      margin: 0;
      font-size: 15px;
      font-weight: 700;
      color: #334155;
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }

    .controls-grid {
      display: grid;
      grid-template-columns: minmax(240px, 1.4fr) minmax(200px, 0.85fr) minmax(240px, 0.9fr) minmax(160px, 0.65fr);
      gap: 14px;
      align-items: start;
    }

    .control-field label {
      display: block;
      margin-bottom: 8px;
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }

    .control-field input,
    .control-field select {
      width: 100%;
      border: 1px solid #cbd5e1;
      border-radius: var(--radius-md);
      background: #ffffff;
      color: var(--ink);
      padding: 11px 12px;
      font-size: 14px;
      font-family: inherit;
    }

    .control-field select[multiple] {
      min-height: 132px;
      padding-block: 8px;
    }

    .quick-view-row {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      margin-bottom: 6px;
    }

    .quick-view-button {
      border: 1px solid #cbd5e1;
      border-radius: 999px;
      padding: 8px 12px;
      background: #ffffff;
      color: #334155;
      font-size: 13px;
      font-weight: 600;
      cursor: pointer;
      transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    }

    .quick-view-button:hover {
      border-color: #94a3b8;
    }

    .quick-view-button.active {
      background: var(--accent-soft);
      border-color: #bfdbfe;
      color: var(--accent);
    }

    .control-check {
      display: flex;
      gap: 10px;
      align-items: center;
      padding-top: 30px;
      color: #334155;
      font-size: 14px;
    }

    .control-check input {
      width: 16px;
      height: 16px;
      margin: 0;
    }

    .results-card {
      min-height: 560px;
      overflow: hidden;
    }

    .results-header {
      display: flex;
      justify-content: space-between;
      gap: 12px;
      align-items: baseline;
      padding: 18px 18px 10px;
      border-bottom: 1px solid var(--line);
      flex-wrap: wrap;
    }

    .results-title {
      margin: 0;
      font-size: 16px;
      font-weight: 700;
      color: #334155;
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }

    .results-table-wrap {
      overflow-x: auto;
    }

    .results-table {
      width: 100%;
      border-collapse: separate;
      border-spacing: 0;
      font-size: 14px;
    }

    .results-table thead th {
      position: sticky;
      top: 0;
      z-index: 2;
      text-align: left;
      padding: 13px 12px;
      background: var(--panel-soft);
      border-bottom: 1px solid var(--line);
      color: #475569;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }

    .results-table tbody td {
      padding: 12px;
      border-bottom: 1px solid var(--line);
      vertical-align: top;
    }

    .results-row {
      cursor: pointer;
      transition: background 120ms ease;
    }

    .results-row:hover {
      background: #f8fafc;
    }

    .result-row-selected {
      background: #eff6ff;
    }

    .status-pill,
    .signal-pill,
    .event-pill {
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
      padding: 5px 10px;
      font-size: 12px;
      font-weight: 700;
      border: 1px solid transparent;
      line-height: 1.2;
      white-space: nowrap;
    }

    .status-pill.pass {
      background: var(--pass-soft);
      color: var(--pass);
      border-color: rgba(22, 101, 52, 0.16);
    }

    .status-pill.fail {
      background: var(--fail-soft);
      color: var(--fail);
      border-color: rgba(185, 28, 28, 0.16);
    }

    .signal-pill,
    .event-pill {
      background: #f8fafc;
      color: #334155;
      border-color: #e2e8f0;
      font-weight: 600;
    }

    .event-pill.missing {
      background: var(--fail-soft);
      color: var(--fail);
      border-color: rgba(185, 28, 28, 0.16);
    }

    .event-pill.unexpected {
      background: var(--warn-soft);
      color: var(--warn);
      border-color: rgba(180, 83, 9, 0.16);
    }

    .chip-row {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-top: 8px;
    }

    .hover-cell {
      position: relative;
      max-width: 0;
    }

    .cell-truncate {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .cell-popover {
      display: none;
      position: absolute;
      left: 0;
      top: calc(100% + 6px);
      min-width: 260px;
      max-width: 520px;
      white-space: pre-wrap;
      background: #ffffff;
      color: #0f172a;
      border: 1px solid #cbd5e1;
      border-radius: var(--radius-sm);
      box-shadow: 0 10px 30px rgba(15, 23, 42, 0.18);
      padding: 12px;
      z-index: 30;
      pointer-events: none;
    }

    .hover-cell:hover .cell-popover {
      display: block;
    }

    .description-cell {
      min-width: 320px;
      max-width: 460px;
    }

    .highlight-cell {
      min-width: 220px;
      max-width: 320px;
    }

    .preview-cell {
      min-width: 220px;
    }

    .preview-cell svg {
      display: block;
    }

    .case-subject,
    .case-metric {
      margin-top: 4px;
      color: var(--muted);
      font-size: 12px;
      line-height: 1.45;
    }

    .drawer-layer.hidden {
      display: none;
    }

    .sheet-backdrop {
      position: fixed;
      inset: 0;
      background: rgba(15, 23, 42, 0.42);
      backdrop-filter: blur(3px);
      z-index: 1000;
    }

    .detail-sheet-panel {
      position: fixed;
      top: 0;
      right: 0;
      height: 100vh;
      width: min(46rem, 48vw);
      max-width: 100%;
      background: #ffffff;
      border-left: 1px solid var(--line);
      box-shadow: -12px 0 32px rgba(15, 23, 42, 0.18);
      padding: 16px 16px 24px;
      overflow-y: auto;
      z-index: 1001;
    }

    .detail-header {
      display: flex;
      justify-content: space-between;
      gap: 16px;
      align-items: flex-start;
      padding-bottom: 12px;
      margin-bottom: 14px;
      border-bottom: 1px solid var(--line);
    }

    .detail-eyebrow {
      margin-bottom: 4px;
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }

    .detail-title {
      margin: 0 0 6px;
      font-size: 16px;
      line-height: 1.35;
      font-weight: 750;
    }

    .detail-subtitle {
      margin: 0;
      color: var(--muted);
      font-size: 13px;
      line-height: 1.5;
    }

    .close-button {
      border: 1px solid #cbd5e1;
      border-radius: 999px;
      padding: 8px 12px;
      background: #ffffff;
      color: #334155;
      font-size: 13px;
      font-weight: 700;
      cursor: pointer;
    }

    .detail-grid {
      display: grid;
      gap: 12px;
    }

    .section-card {
      padding: 14px;
    }

    .section-card pre {
      margin: 0;
      font-size: 12px;
      line-height: 1.5;
      font-family: "SFMono-Regular", "Menlo", monospace;
      white-space: pre-wrap;
      word-break: break-word;
    }

    .summary-block {
      margin-top: 8px;
      white-space: pre-wrap;
      background: var(--panel-soft);
      border-radius: var(--radius-sm);
      padding: 12px;
      line-height: 1.55;
      font-size: 14px;
    }

    .mini-metric-grid {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 10px;
      margin-top: 10px;
    }

    .mini-metric {
      border-radius: var(--radius-md);
      background: var(--panel-soft);
      padding: 10px 12px;
    }

    .mini-metric-label {
      color: var(--muted);
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      margin-bottom: 4px;
    }

    .mini-metric-value {
      font-size: 18px;
      line-height: 1.1;
      font-weight: 750;
      letter-spacing: -0.02em;
    }

    .chart-legend {
      display: grid;
      gap: 6px;
      margin: 10px 0 0;
    }

    .chart-legend-item {
      display: flex;
      align-items: center;
      gap: 8px;
      color: #475569;
      font-size: 13px;
    }

    .chart-swatch {
      width: 10px;
      height: 10px;
      border-radius: 999px;
      display: inline-block;
      flex: 0 0 auto;
    }

    .table-scroll {
      overflow-x: auto;
    }

    .signal-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
      margin-top: 10px;
    }

    .signal-table th,
    .signal-table td {
      border-bottom: 1px solid var(--line);
      padding: 8px 6px;
      text-align: left;
      vertical-align: top;
    }

    .signal-table th {
      color: #475569;
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }

    .detail-section + .detail-section {
      margin-top: 14px;
    }

    .event-time-grid {
      display: grid;
      gap: 10px;
      margin-top: 10px;
    }

    .event-time-card {
      border: 1px solid var(--line);
      border-radius: var(--radius-md);
      background: var(--panel-soft);
      padding: 10px 12px;
    }

    .event-time-card-header {
      display: flex;
      justify-content: space-between;
      gap: 10px;
      align-items: baseline;
      flex-wrap: wrap;
    }

    .event-time-kind {
      font-size: 13px;
      font-weight: 700;
      color: #334155;
    }

    .event-time-range {
      color: var(--muted);
      font-size: 12px;
    }

    .event-time-points {
      margin-top: 8px;
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }

    .event-time-pill {
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
      padding: 5px 10px;
      font-size: 12px;
      font-weight: 700;
      color: #334155;
      background: #ffffff;
      border: 1px solid #dbe3ef;
      white-space: nowrap;
    }

    details.json-block {
      margin-top: 10px;
      border: 1px solid var(--line);
      border-radius: var(--radius-md);
      overflow: hidden;
      background: var(--panel-soft);
    }

    details.json-block summary {
      cursor: pointer;
      padding: 11px 12px;
      font-size: 13px;
      font-weight: 700;
      color: #334155;
      list-style: none;
    }

    details.json-block summary::-webkit-details-marker {
      display: none;
    }

    details.json-block pre {
      padding: 0 12px 12px;
    }

    .footer {
      margin-top: 18px;
      color: var(--muted);
      font-size: 13px;
    }

    @media (max-width: 1180px) {
      .summary-grid,
      .tag-rate-row {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }

      .controls-grid {
        grid-template-columns: 1fr 1fr;
      }

      .mini-metric-grid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }

      .detail-sheet-panel {
        width: min(100vw, 46rem);
      }
    }

    @media (max-width: 760px) {
      .shell {
        width: min(100vw - 20px, 100%);
        padding-top: 20px;
      }

      .summary-grid,
      .tag-rate-row,
      .controls-grid,
      .mini-metric-grid {
        grid-template-columns: 1fr;
      }

      .hero-title {
        font-size: 28px;
      }

      .detail-sheet-panel {
        width: 100vw;
        padding: 14px;
      }
    }
  </style>
</head>
<body>
  <div class="shell">
    <section class="hero">
      <h1 class="hero-title">Compiler Benchmark Console</h1>
      <p class="hero-subtitle">
        See the full regression overview first, then inspect any specific case from the right-side detail drawer.
        Rust remains the only regression judge; this page only consumes <code>viewer-json</code>.
        Generated at __GENERATED_AT__.
      </p>
      __REFRESH_BADGE__
    </section>

    <section class="summary-grid">
      __SUMMARY_CARDS__
    </section>

    __TAG_RATE_SECTION__

    <section class="controls-card">
      <div class="controls-header">
        <h2 class="controls-title">Controls</h2>
        <div class="summary-path">Data source: <code>viewer-json</code> from <code>compiler-rs</code>.</div>
      </div>
      <div class="controls-grid">
        <div class="control-field">
          <label for="search">Search</label>
          <input id="search" type="text" placeholder="case name / subject / metric / event / description" />
          <div class="results-meta">Searches case id, subject, metric, events, tags, and the structured description.</div>
        </div>
        <div class="control-field">
          <label>Quick View</label>
          <div class="quick-view-row">
            __QUICK_VIEW_BUTTONS__
          </div>
          <div class="results-meta">Fast slices inspired by the sibling Compiler benchmark console.</div>
        </div>
        <div class="control-field">
          <label for="tag-filter">Filter By Tag</label>
          <select id="tag-filter" multiple></select>
          <div class="results-meta">Multi-select. Matches if any selected tag is present.</div>
        </div>
        <div class="control-field">
          <label for="scope-filter">Scope</label>
          <select id="scope-filter">
            <option value="all">All</option>
            <option value="line">Line</option>
            <option value="group">Group</option>
          </select>
          <label class="control-check">
            <input id="failures-only" type="checkbox" />
            <span>Only failing cases</span>
          </label>
        </div>
      </div>
    </section>

    <section class="results-card">
      <div class="results-header">
        <h2 class="results-title">All Test Results</h2>
        <div id="visible-count" class="results-meta"></div>
      </div>
      <div class="results-table-wrap">
        <table class="results-table">
          <thead>
            <tr>
              <th>Case</th>
              <th>Scope</th>
              <th>Status</th>
              <th>Failed</th>
              <th>Check Rate</th>
              <th>Highlighted Series</th>
              <th>Preview</th>
              <th>Description</th>
              <th>Tags</th>
            </tr>
          </thead>
          <tbody id="results-body"></tbody>
        </table>
      </div>
    </section>

    <p class="footer">__FOOTER_TEXT__</p>
  </div>

  <div id="detail-layer" class="drawer-layer hidden" aria-hidden="true">
    <div id="detail-backdrop" class="sheet-backdrop"></div>
    <aside class="detail-sheet-panel" role="dialog" aria-modal="true" aria-labelledby="detail-title">
      <div class="detail-header">
        <div>
          <div class="detail-eyebrow">Case Detail</div>
          <h2 id="detail-title" class="detail-title"></h2>
          <p id="detail-subtitle" class="detail-subtitle"></p>
        </div>
        <button id="detail-close" type="button" class="close-button">Close</button>
      </div>
      <div id="detail-body"></div>
    </aside>
  </div>

  <script id="viewer-data" type="application/json">__VIEWER_JSON__</script>
  <script>
    const viewerData = JSON.parse(document.getElementById("viewer-data").textContent);
    const state = { quickView: "all", selectedCaseId: null };

    const searchInput = document.getElementById("search");
    const tagFilter = document.getElementById("tag-filter");
    const scopeFilter = document.getElementById("scope-filter");
    const failuresOnly = document.getElementById("failures-only");
    const visibleCount = document.getElementById("visible-count");
    const resultsBody = document.getElementById("results-body");
    const quickViewButtons = [...document.querySelectorAll("[data-quick-view]")];
    const detailLayer = document.getElementById("detail-layer");
    const detailBackdrop = document.getElementById("detail-backdrop");
    const detailClose = document.getElementById("detail-close");
    const detailTitle = document.getElementById("detail-title");
    const detailSubtitle = document.getElementById("detail-subtitle");
    const detailBody = document.getElementById("detail-body");

    function escapeHtml(value) {
      return String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#39;");
    }

    function populateTagFilter() {
      viewerData.tag_options.forEach((tag) => {
        const option = document.createElement("option");
        option.value = tag;
        option.textContent = tag;
        tagFilter.appendChild(option);
      });
    }

    function quickViewMatches(caseItem) {
      if (state.quickView === "all") return true;
      if (state.quickView === "failures") return !caseItem.passed;
      if (state.quickView === "spike") return caseItem.tags.includes("spike") || caseItem.tags.includes("drop");
      if (state.quickView === "shift") return caseItem.tags.includes("regime_shift");
      if (state.quickView === "volatile") return caseItem.tags.includes("oscillation");
      return true;
    }

    function selectedTags() {
      return [...tagFilter.selectedOptions].map((option) => option.value);
    }

    function filteredCases() {
      const query = searchInput.value.trim().toLowerCase();
      const tags = selectedTags();
      const scope = scopeFilter.value;
      const onlyFailures = failuresOnly.checked;

      return viewerData.cases.filter((caseItem) => {
        if (!quickViewMatches(caseItem)) return false;
        if (scope !== "all" && caseItem.scope !== scope) return false;
        if (onlyFailures && caseItem.passed) return false;
        if (tags.length > 0 && !tags.some((tag) => caseItem.tags.includes(tag))) return false;
        if (query && !caseItem.search_text.includes(query)) return false;
        return true;
      });
    }

    function hoverCell(value, fullValue, extraClass) {
      return [
        `<td class="hover-cell ${extraClass || ""}">`,
        `<div class="cell-truncate">${escapeHtml(value || "-")}</div>`,
        `<div class="cell-popover">${escapeHtml(fullValue || value || "-")}</div>`,
        `</td>`,
      ].join("");
    }

    function renderRow(caseItem) {
      const selectedClass = state.selectedCaseId === caseItem.case_id ? " result-row-selected" : "";
      const statusTone = caseItem.passed ? "pass" : "fail";
      return [
        `<tr class="results-row${selectedClass}" data-case-id="${escapeHtml(caseItem.case_id)}">`,
        `<td><code>${escapeHtml(caseItem.case_name)}</code><div class="case-subject">${escapeHtml(caseItem.subject_id)}</div><div class="case-metric">${escapeHtml(caseItem.metric_id || "-")}</div><div class="case-metric">problem=${escapeHtml(caseItem.problem_range || "-")}</div></td>`,
        `<td>${escapeHtml(caseItem.scope)}</td>`,
        `<td><span class="status-pill ${statusTone}">${caseItem.passed ? "PASS" : "FAIL"}</span></td>`,
        `<td>${escapeHtml(caseItem.failed_display)}</td>`,
        `<td>${escapeHtml(caseItem.check_rate_display)}</td>`,
        hoverCell(caseItem.highlight_preview, caseItem.highlight_full, "highlight-cell"),
        `<td class="preview-cell">${caseItem.preview_svg}</td>`,
        hoverCell(caseItem.description_preview, caseItem.description_full, "description-cell"),
        hoverCell(caseItem.tags_preview, caseItem.tags_full, ""),
        `</tr>`,
      ].join("");
    }

    function renderTable() {
      const cases = filteredCases();
      visibleCount.textContent = `${cases.length} / ${viewerData.cases.length} cases visible`;
      if (cases.length === 0) {
        resultsBody.innerHTML = `<tr><td colspan="9"><div class="empty-note">No regression cases match the current filters.</div></td></tr>`;
      } else {
        resultsBody.innerHTML = cases.map(renderRow).join("");
      }

      if (state.selectedCaseId && !cases.some((caseItem) => caseItem.case_id === state.selectedCaseId)) {
        closeDetail();
      }
    }

    function openDetail(caseId) {
      const caseItem = viewerData.cases.find((item) => item.case_id === caseId);
      if (!caseItem) return;
      state.selectedCaseId = caseId;
      detailTitle.innerHTML = `<code>${escapeHtml(caseItem.case_name)}</code>`;
      detailSubtitle.textContent = `scope=${caseItem.scope}; subject=${caseItem.subject_id}; metric=${caseItem.metric_id || "-"}`;
      detailBody.innerHTML = caseItem.detail_html;
      detailLayer.classList.remove("hidden");
      detailLayer.setAttribute("aria-hidden", "false");
      renderTable();
    }

    function closeDetail() {
      state.selectedCaseId = null;
      detailLayer.classList.add("hidden");
      detailLayer.setAttribute("aria-hidden", "true");
      detailBody.innerHTML = "";
      renderTable();
    }

    quickViewButtons.forEach((button) => {
      button.addEventListener("click", () => {
        state.quickView = button.dataset.quickView;
        quickViewButtons.forEach((item) => item.classList.toggle("active", item === button));
        renderTable();
      });
    });

    [searchInput, tagFilter, scopeFilter, failuresOnly].forEach((element) => {
      const eventName = element === searchInput ? "input" : "change";
      element.addEventListener(eventName, renderTable);
    });

    resultsBody.addEventListener("click", (event) => {
      const row = event.target.closest(".results-row");
      if (!row) return;
      openDetail(row.dataset.caseId);
    });

    detailBackdrop.addEventListener("click", closeDetail);
    detailClose.addEventListener("click", closeDetail);
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !detailLayer.classList.contains("hidden")) {
        closeDetail();
      }
    });

    populateTagFilter();
    renderTable();
  </script>
__LIVE_SCRIPT__
</body>
</html>
"""

    return (
        html_template.replace("__GENERATED_AT__", html.escape(generated_at))
        .replace("__REFRESH_BADGE__", refresh_badge)
        .replace("__SUMMARY_CARDS__", render_summary_cards(report))
        .replace("__TAG_RATE_SECTION__", render_tag_rate_section(tag_rates))
        .replace("__QUICK_VIEW_BUTTONS__", quick_view_buttons)
        .replace("__FOOTER_TEXT__", html.escape(footer_text))
        .replace("__VIEWER_JSON__", json_script(viewer_model))
        .replace("__LIVE_SCRIPT__", live_script)
    )


def build_case_view(index: int, case: dict[str, Any]) -> dict[str, Any]:
    output = case.get("output") or {}
    canonical = output.get("canonical") or {}
    llm = output.get("llm") or {}
    series = extract_series(case)
    highlighted = [item for item in series if item["highlight"]] or series[:1]
    series_summary = build_series_summary(series)
    collection_summary = build_collection_summary(case, series)

    expected = case.get("expected", [])
    actual = case.get("actual", [])
    missing = case.get("missing", [])
    unexpected = case.get("unexpected", [])
    matched_count = max(len(expected) - len(missing), 0)
    check_rate = 1.0 if not expected else matched_count / len(expected)
    failed_checks = len(missing) + len(unexpected)

    tags = derive_case_tags(case, canonical)
    description = llm.get("description", "no llm output")
    highlight_labels = [item["label"] for item in highlighted]
    preview_svg = build_series_chart(
        series,
        top_events=canonical.get("top_events", []),
        width=220,
        height=96,
        show_legend=False,
        show_axes=True,
    )
    detail_svg = build_series_chart(
        series,
        top_events=canonical.get("top_events", []),
        width=640,
        height=260,
        show_legend=True,
        show_axes=True,
    )
    highlight_svg = build_series_chart(
        highlighted,
        top_events=canonical.get("top_events", []),
        width=360,
        height=180,
        show_legend=True,
        show_axes=True,
    )

    search_text = " ".join(
        [
            case["case_name"],
            case["scope"],
            case["subject_id"],
            canonical.get("metric_id", ""),
            canonical.get("state", ""),
            canonical.get("trend", ""),
            " ".join(tags),
            " ".join(expected),
            " ".join(actual),
            " ".join(missing),
            " ".join(unexpected),
            description,
        ]
    ).lower()

    return {
        "case_id": f"case-{index}",
        "case_name": case["case_name"],
        "scope": case["scope"],
        "subject_id": case["subject_id"],
        "metric_id": canonical.get("metric_id", ""),
        "problem_range": infer_problem_range(canonical),
        "passed": case["passed"],
        "failed_display": f"{failed_checks}/{max(len(expected), 1)}",
        "check_rate_display": f"{check_rate:.0%}",
        "description_preview": truncate_text(description.replace("\n", " | "), limit=120),
        "description_full": description,
        "highlight_preview": truncate_text(", ".join(highlight_labels) or "-", limit=52),
        "highlight_full": ", ".join(highlight_labels) or "-",
        "preview_svg": preview_svg,
        "tags": tags,
        "tags_preview": truncate_text(", ".join(tags), limit=70),
        "tags_full": ", ".join(tags),
        "search_text": search_text,
        "detail_html": render_case_detail_html(
            case,
            canonical,
            llm,
            highlighted,
            detail_svg,
            highlight_svg,
            series_summary,
            collection_summary,
        ),
    }


def render_case_detail_html(
    case: dict[str, Any],
    canonical: dict[str, Any],
    llm: dict[str, Any],
    highlighted: list[dict[str, Any]],
    detail_svg: str,
    highlight_svg: str,
    series_summary: list[dict[str, Any]],
    collection_summary: dict[str, Any],
) -> str:
    expected = case.get("expected", [])
    actual = case.get("actual", [])
    missing = case.get("missing", [])
    unexpected = case.get("unexpected", [])
    state = canonical.get("state", "-")
    trend = canonical.get("trend", "-")
    peer_context = canonical.get("peer_context") or {}
    highlighted_text = ", ".join(item["label"] for item in highlighted) or "-"

    return (
        "<div class='detail-grid'>"
        + render_overview_card(case, canonical, llm, highlighted_text, expected, actual, missing, unexpected, highlight_svg)
        + render_collection_card(collection_summary, series_summary)
        + render_signals_card(canonical, state, trend, peer_context, detail_svg)
        + render_raw_json_card(case, canonical, llm)
        + "</div>"
    )


def render_overview_card(
    case: dict[str, Any],
    canonical: dict[str, Any],
    llm: dict[str, Any],
    highlighted_text: str,
    expected: list[str],
    actual: list[str],
    missing: list[str],
    unexpected: list[str],
    highlight_svg: str,
) -> str:
    failed_checks = len(missing) + len(unexpected)
    top_events = canonical.get("top_events", [])
    problem_range = infer_problem_range(canonical)
    return "".join(
        [
            "<section class='section-card detail-section'>",
            "<div class='section-label'>LLM Facts</div>",
            f"<div class='summary-block'>{html.escape(llm.get('description', 'no llm output'))}</div>",
            "<div class='mini-metric-grid'>",
            render_mini_metric("Passed", "yes" if case["passed"] else "no"),
            render_mini_metric("Failed checks", str(failed_checks)),
            render_mini_metric("Top events", str(len(top_events))),
            render_mini_metric("Window", format_window_secs(canonical.get("window_secs"))),
            render_mini_metric("Problem range", problem_range),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Expected / Actual</div>",
            render_named_pills("expected", expected, pill_type="signal"),
            render_named_pills("actual", actual, pill_type="signal"),
            render_named_pills("missing", missing, pill_type="missing") if missing else "",
            render_named_pills("unexpected", unexpected, pill_type="unexpected") if unexpected else "",
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Highlighted Series</div>",
            f"<div class='summary-block'>{html.escape(highlighted_text)}</div>",
            highlight_svg,
            "</div>",
            "</section>",
        ]
    )


def render_collection_card(collection_summary: dict[str, Any], series_summary: list[dict[str, Any]]) -> str:
    policy_json = json.dumps(collection_summary["policy"], indent=2, ensure_ascii=False)
    return "".join(
        [
            "<section class='section-card detail-section'>",
            "<div class='section-label'>Collection Data</div>",
            "<div class='mini-metric-grid'>",
            render_mini_metric("Series", str(collection_summary["series_count"])),
            render_mini_metric("Groups", str(collection_summary["group_count"])),
            render_mini_metric("Points", str(collection_summary["total_points"])),
            render_mini_metric("Metric ids", str(collection_summary["metric_count"])),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Series Snapshot</div>",
            render_series_summary_table(series_summary),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Policy</div>",
            f"<div class='summary-block'>{html.escape(policy_json)}</div>",
            "</div>",
            "</section>",
        ]
    )


def render_signals_card(
    canonical: dict[str, Any],
    state: str,
    trend: str,
    peer_context: dict[str, Any],
    detail_svg: str,
) -> str:
    state_and_trend = render_named_pills("state", [state], pill_type="signal") + render_named_pills(
        "trend", [trend], pill_type="signal"
    )
    if peer_context:
        state_and_trend += render_named_pills(
            "peer",
            [
                f"rank={peer_context.get('rank', '-')}/{peer_context.get('total', '-')}",
                f"percentile={peer_context.get('percentile', '-')}",
            ],
            pill_type="signal",
        )

    return "".join(
        [
            "<section class='section-card detail-section'>",
            "<div class='section-label'>Canonical Signals</div>",
            state_and_trend,
            "<div class='detail-section'>",
            "<div class='section-label'>Curve</div>",
            detail_svg,
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Event Timeline</div>",
            render_event_time_cards(canonical.get("top_events", []), canonical.get("window_start_ts_secs")),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Top Events</div>",
            render_event_table(canonical.get("top_events", []), canonical.get("window_start_ts_secs")),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Regimes</div>",
            render_regime_table(canonical.get("regimes", [])),
            "</div>",
            "<div class='detail-section'>",
            "<div class='section-label'>Evidence</div>",
            render_evidence_table(canonical.get("evidence", [])),
            "</div>",
            "</section>",
        ]
    )


def render_raw_json_card(case: dict[str, Any], canonical: dict[str, Any], llm: dict[str, Any]) -> str:
    return (
        "<section class='section-card detail-section'>"
        "<div class='section-label'>Raw JSON</div>"
        + render_json_block("Request", case.get("request", {}))
        + render_json_block("Canonical", canonical)
        + render_json_block("LLM", llm)
        + "</section>"
    )


def render_json_block(label: str, payload: Any) -> str:
    return (
        "<details class='json-block'>"
        f"<summary>{html.escape(label)}</summary>"
        f"<pre>{html.escape(json.dumps(payload, indent=2, ensure_ascii=False))}</pre>"
        "</details>"
    )


def render_series_summary_table(series_summary: list[dict[str, Any]]) -> str:
    if not series_summary:
        return "<div class='empty-note'>No series data available.</div>"
    rows = []
    for item in series_summary:
        rows.append(
            "<tr>"
            f"<td>{html.escape(item['label'])}</td>"
            f"<td>{html.escape(str(item['points']))}</td>"
            f"<td>{html.escape(item['min'])}</td>"
            f"<td>{html.escape(item['max'])}</td>"
            f"<td>{'yes' if item['highlight'] else 'no'}</td>"
            "</tr>"
        )
    return (
        "<div class='table-scroll'><table class='signal-table'>"
        "<thead><tr><th>Series</th><th>Points</th><th>Min</th><th>Max</th><th>Highlighted</th></tr></thead>"
        "<tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def render_event_time_cards(events: list[dict[str, Any]], window_start_ts_secs: Any) -> str:
    if not events:
        return "<div class='empty-note'>No event timing available.</div>"
    rows = []
    for event in events:
        points = event.get("timepoints_ts_secs", []) or []
        point_pills = "".join(
            f"<span class='event-time-pill'>{html.escape(label)}</span>"
            for label in format_event_timepoints(points, window_start_ts_secs)
        )
        if not point_pills:
            point_pills = "<span class='event-time-pill'>range event</span>"
        rows.append(
            "<div class='event-time-card'>"
            "<div class='event-time-card-header'>"
            f"<div class='event-time-kind'>{html.escape(event.get('kind', '-'))}</div>"
            f"<div class='event-time-range'>{html.escape(format_event_range_label(event, window_start_ts_secs))}</div>"
            "</div>"
            f"<div class='event-time-points'>{point_pills}</div>"
            "</div>"
        )
    return "<div class='event-time-grid'>" + "".join(rows) + "</div>"


def render_event_table(events: list[dict[str, Any]], window_start_ts_secs: Any) -> str:
    if not events:
        return "<div class='empty-note'>No events generated.</div>"
    rows = []
    for event in events:
        evidence = ", ".join(f"{item['label']}={item['value']}" for item in event.get("evidence", [])[:4]) or "-"
        score = f"{event.get('score', 0.0):.2f}"
        timepoints = ", ".join(format_event_timepoints(event.get("timepoints_ts_secs", []), window_start_ts_secs)) or "-"
        rows.append(
            "<tr>"
            f"<td>{html.escape(event.get('kind', '-'))}</td>"
            f"<td>{html.escape(score)}</td>"
            f"<td>{html.escape(format_ts_range(event.get('start_ts_secs'), event.get('end_ts_secs')))}</td>"
            f"<td>{html.escape(timepoints)}</td>"
            f"<td>{html.escape(str(event.get('impacted_members', '-')))}</td>"
            f"<td>{html.escape(evidence)}</td>"
            "</tr>"
        )
    return (
        "<div class='table-scroll'><table class='signal-table'>"
        "<thead><tr><th>Kind</th><th>Score</th><th>Range</th><th>Time points</th><th>Members</th><th>Evidence</th></tr></thead>"
        "<tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def render_regime_table(regimes: list[dict[str, Any]]) -> str:
    if not regimes:
        return "<div class='empty-note'>No regimes generated.</div>"
    rows = []
    for regime in regimes:
        delta = regime.get("delta_from_prev")
        rows.append(
            "<tr>"
            f"<td>{html.escape(format_ts_range(regime.get('start_ts_secs'), regime.get('end_ts_secs')))}</td>"
            f"<td>{html.escape(format_number(regime.get('mean')))}</td>"
            f"<td>{html.escape(format_number(delta) if delta is not None else '-')}</td>"
            "</tr>"
        )
    return (
        "<div class='table-scroll'><table class='signal-table'>"
        "<thead><tr><th>Range</th><th>Mean</th><th>Delta From Prev</th></tr></thead>"
        "<tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def render_evidence_table(evidence: list[dict[str, str]]) -> str:
    if not evidence:
        return "<div class='empty-note'>No evidence generated.</div>"
    rows = []
    for item in evidence:
        rows.append(
            "<tr>"
            f"<td>{html.escape(item.get('label', '-'))}</td>"
            f"<td>{html.escape(item.get('value', '-'))}</td>"
            "</tr>"
        )
    return (
        "<div class='table-scroll'><table class='signal-table'>"
        "<thead><tr><th>Label</th><th>Value</th></tr></thead>"
        "<tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def render_named_pills(label: str, values: list[str], *, pill_type: str) -> str:
    if not values:
        return ""
    pill_class = {
        "missing": "event-pill missing",
        "unexpected": "event-pill unexpected",
        "signal": "signal-pill",
    }.get(pill_type, "signal-pill")
    chips = "".join(f"<span class='{pill_class}'>{html.escape(value)}</span>" for value in values)
    return (
        "<div class='detail-section'>"
        f"<div class='field-label'>{html.escape(label)}</div>"
        f"<div class='chip-row'>{chips}</div>"
        "</div>"
    )


def render_mini_metric(label: str, value: str) -> str:
    return (
        "<div class='mini-metric'>"
        f"<div class='mini-metric-label'>{html.escape(label)}</div>"
        f"<div class='mini-metric-value'>{html.escape(value)}</div>"
        "</div>"
    )


def render_summary_cards(report: dict[str, Any]) -> str:
    total = report["total"]
    passed = report["passed"]
    case_pass_rate = 0.0 if total == 0 else passed / total
    line = report["line_level_top3"]
    group = report["group_level_top3"]
    failed = total - passed
    return "".join(
        [
            render_metric_card("Case pass rate", f"{case_pass_rate:.0%}", f"{passed}/{total} cases passed"),
            render_metric_card("Line top3", f"{line['passed']}/{line['total']}", f"{line['failed']} failing line cases"),
            render_metric_card("Group top3", f"{group['passed']}/{group['total']}", f"{group['failed']} failing group cases"),
            render_metric_card("Failed cases", str(failed), "Cases whose top3 did not match expected output"),
        ]
    )


def render_metric_card(label: str, value: str, meta: str) -> str:
    return (
        "<article class='metric-card'>"
        f"<div class='metric-label'>{html.escape(label)}</div>"
        f"<div class='metric-value'>{html.escape(value)}</div>"
        f"<div class='metric-meta'>{html.escape(meta)}</div>"
        "</article>"
    )


def render_tag_rate_section(tag_rates: list[tuple[str, dict[str, Any]]]) -> str:
    if not tag_rates:
        return ""
    cards = "".join(
        render_tag_rate_card(tag, metrics["passed"], metrics["total"], metrics["pass_rate"])
        for tag, metrics in tag_rates[:6]
    )
    return f"<section class='tag-rate-row'>{cards}</section>"


def render_tag_rate_card(tag: str, passed: int, total: int, pass_rate: float) -> str:
    return (
        "<article class='tag-rate-card'>"
        f"<div class='tag-rate-label'>{html.escape(tag)}</div>"
        f"<div class='tag-rate-value'>{pass_rate:.0%}</div>"
        f"<div class='tag-rate-meta'>{passed}/{total} cases passed</div>"
        "</article>"
    )


def build_tag_rates(cases: list[dict[str, Any]]) -> list[tuple[str, dict[str, Any]]]:
    excluded = {"pass", "fail", "line", "group"}
    stats: dict[str, dict[str, Any]] = {}
    for case in cases:
        unique_tags = {tag for tag in case["tags"] if tag not in excluded}
        for tag in unique_tags:
            bucket = stats.setdefault(tag, {"passed": 0, "total": 0, "pass_rate": 0.0})
            bucket["total"] += 1
            if case["passed"]:
                bucket["passed"] += 1
    for bucket in stats.values():
        bucket["pass_rate"] = 0.0 if bucket["total"] == 0 else bucket["passed"] / bucket["total"]
    return sorted(stats.items(), key=lambda item: (-item[1]["total"], item[0]))


def derive_case_tags(case: dict[str, Any], canonical: dict[str, Any]) -> list[str]:
    tags = {
        case["scope"],
        "pass" if case["passed"] else "fail",
        canonical.get("state", ""),
        canonical.get("trend", ""),
    }
    for key in ("expected", "actual", "missing", "unexpected"):
        tags.update(case.get(key, []))
    return sorted(tag for tag in tags if tag)


def infer_problem_range(canonical: dict[str, Any]) -> str:
    top_events = canonical.get("top_events", [])
    focused = [
        event
        for event in top_events
        if event.get("start_ts_secs") is not None
        and event.get("end_ts_secs") is not None
        and int(event.get("start_ts_secs", 0)) < int(event.get("end_ts_secs", 0))
        and event.get("kind") not in {"increasing_trend", "decreasing_trend"}
    ]
    events = focused or top_events[:1]
    if events:
        start = min(int(event.get("start_ts_secs", 0)) for event in events)
        end = max(int(event.get("end_ts_secs", 0)) for event in events)
        return f"{format_ts_label(start)}->{format_ts_label(end)}"

    window_secs = canonical.get("window_secs")
    if window_secs is None:
        return "-"
    return f"0m->{format_window_secs(window_secs)}"


def build_collection_summary(case: dict[str, Any], series: list[dict[str, Any]]) -> dict[str, Any]:
    request = case.get("request", {})
    point_counts = [len(item["points"]) for item in series]
    metric_ids = sorted({item.get("metric_id", "") for item in series if item.get("metric_id")})
    if request.get("scope") == "group":
        group_count = len(request.get("groups", []))
        policy = request.get("policy") or {}
    else:
        group_count = 1 if request.get("series") else 0
        policy = request.get("policy") or {}
    return {
        "series_count": len(series),
        "group_count": group_count,
        "total_points": sum(point_counts),
        "min_points_per_series": min(point_counts) if point_counts else 0,
        "max_points_per_series": max(point_counts) if point_counts else 0,
        "metric_count": len(metric_ids),
        "policy": policy,
    }


def build_series_summary(series: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows = []
    for item in series:
        values = [point["value"] for point in item["points"]]
        rows.append(
            {
                "label": item["label"],
                "points": len(item["points"]),
                "min": format_number(min(values) if values else None),
                "max": format_number(max(values) if values else None),
                "highlight": item["highlight"],
            }
        )
    return rows


def build_series_chart(
    series: list[dict[str, Any]],
    *,
    top_events: list[dict[str, Any]],
    width: int,
    height: int,
    show_legend: bool,
    show_axes: bool,
) -> str:
    if not series:
        return "<div class='empty-note'>No point data available.</div>"

    all_points = [point for item in series for point in item["points"]]
    if not all_points:
        return "<div class='empty-note'>No point data available.</div>"

    pad_left = 54 if show_axes else 18
    pad_right = 12
    pad_top = 12
    pad_bottom = 34 if show_axes else 18

    raw_min_x = min(point["ts_secs"] for point in all_points)
    raw_max_x = max(point["ts_secs"] for point in all_points)
    absolute_time_axis = looks_like_unix_timestamp(raw_min_x) and looks_like_unix_timestamp(raw_max_x)
    origin_ts_secs = raw_min_x if absolute_time_axis else 0
    min_x = raw_min_x - origin_ts_secs
    max_x = raw_max_x - origin_ts_secs
    min_y = min(point["value"] for point in all_points)
    max_y = max(point["value"] for point in all_points)
    if math.isclose(min_y, max_y):
        min_y -= 1.0
        max_y += 1.0
    y_pad = (max_y - min_y) * 0.12
    min_y -= y_pad
    max_y += y_pad

    def chart_x(ts_secs: int) -> int:
        return int(ts_secs) - origin_ts_secs

    def scale_x(ts_secs: int) -> float:
        span = max(max_x - min_x, 1)
        return pad_left + ((ts_secs - min_x) / span) * (width - pad_left - pad_right)

    def scale_y(value: float) -> float:
        span = max(max_y - min_y, 1e-9)
        return height - pad_bottom - ((value - min_y) / span) * (height - pad_top - pad_bottom)

    chart_layers: list[str] = []
    if show_axes:
        for ratio in (0.0, 0.25, 0.5, 0.75, 1.0):
            y = pad_top + ratio * (height - pad_top - pad_bottom)
            value = max_y - ratio * (max_y - min_y)
            chart_layers.append(
                f"<line x1='{pad_left:.1f}' y1='{y:.1f}' x2='{width - pad_right:.1f}' y2='{y:.1f}' stroke='#e2e8f0' stroke-width='1' />"
            )
            chart_layers.append(
                f"<text x='{pad_left - 8:.1f}' y='{y:.1f}' text-anchor='end' dominant-baseline='middle' font-size='10' fill='#64748b'>{html.escape(format_number(value))}</text>"
            )
        tick_values = sorted({min_x, min_x + (max_x - min_x) // 2, max_x})
        for tick in tick_values:
            x = scale_x(tick)
            chart_layers.append(
                f"<line x1='{x:.1f}' y1='{pad_top:.1f}' x2='{x:.1f}' y2='{height - pad_bottom:.1f}' stroke='#f1f5f9' stroke-width='1' />"
            )
            chart_layers.append(
                f"<text x='{x:.1f}' y='{height - 12:.1f}' text-anchor='middle' font-size='10' fill='#64748b'>{html.escape(format_chart_ts_label(tick, origin_ts_secs=origin_ts_secs, absolute_time_axis=absolute_time_axis))}</text>"
            )
        chart_layers.append(
            f"<line x1='{pad_left:.1f}' y1='{height - pad_bottom:.1f}' x2='{width - pad_right:.1f}' y2='{height - pad_bottom:.1f}' stroke='#94a3b8' stroke-width='1' />"
        )
        chart_layers.append(
            f"<line x1='{pad_left:.1f}' y1='{pad_top:.1f}' x2='{pad_left:.1f}' y2='{height - pad_bottom:.1f}' stroke='#94a3b8' stroke-width='1' />"
        )

    bands = []
    markers = []
    marker_labels = []
    for event in top_events:
        color = EVENT_COLORS.get(event.get("kind", ""), "#7c3aed")
        start = scale_x(int(event.get("start_ts_secs", min_x)))
        end = scale_x(int(event.get("end_ts_secs", max_x)))
        width_px = max(end - start, 2)
        bands.append(
            f"<rect x='{start:.1f}' y='{pad_top:.1f}' width='{width_px:.1f}' height='{height - pad_top - pad_bottom:.1f}' fill='{color}' opacity='0.08' />"
        )
        for index, point in enumerate(dedup_ints(event.get("timepoints_ts_secs", []))):
            x = scale_x(point)
            markers.append(
                f"<line x1='{x:.1f}' y1='{pad_top:.1f}' x2='{x:.1f}' y2='{height - pad_bottom:.1f}' stroke='{color}' stroke-width='1.6' stroke-dasharray='4 4' opacity='0.38' />"
            )
            if width >= 320:
                label_y = pad_top + 14 + min(index, 2) * 14
                marker_labels.append(
                    f"<text x='{x + 4:.1f}' y='{label_y:.1f}' font-size='10' fill='{color}'>{html.escape(format_chart_ts_label(point, origin_ts_secs=origin_ts_secs, absolute_time_axis=absolute_time_axis))}</text>"
                )

    lines = []
    legend = []
    for item in series:
        points = " ".join(
            f"{scale_x(chart_x(point['ts_secs'])):.2f},{scale_y(point['value']):.2f}" for point in item["points"]
        )
        opacity = 0.96 if item["highlight"] else 0.62
        stroke_width = 2.8 if item["highlight"] else 1.9
        color = item["color"]
        lines.append(
            f"<polyline fill='none' stroke='{color}' stroke-width='{stroke_width}' opacity='{opacity}' points='{points}' />"
        )
        if show_legend:
            legend.append(
                "<div class='chart-legend-item'>"
                f"<span class='chart-swatch' style='background:{color};'></span>"
                f"{html.escape(truncate_text(item['label'], limit=56))}"
                "</div>"
            )

    svg = (
        f"<svg viewBox='0 0 {width} {height}' width='100%' height='auto' xmlns='http://www.w3.org/2000/svg'>"
        f"<rect x='0' y='0' width='{width}' height='{height}' rx='12' fill='#f8fafc' />"
        + "".join(chart_layers)
        + "".join(bands)
        + "".join(markers)
        + "".join(lines)
        + "".join(marker_labels)
        + "</svg>"
    )
    if not show_legend:
        return svg
    return svg + "<div class='chart-legend'>" + "".join(legend) + "</div>"


def extract_series(case: dict[str, Any]) -> list[dict[str, Any]]:
    request = case["request"]
    scope = request["scope"]
    subject_id = case["subject_id"]
    series = []
    color_index = 0

    if scope == "line":
        for item in request["series"]:
            series.append(
                {
                    "label": item["entity_id"],
                    "metric_id": item["metric_id"],
                    "points": item["points"],
                    "highlight": item["entity_id"] == subject_id,
                    "color": SERIES_PALETTE[color_index % len(SERIES_PALETTE)],
                }
            )
            color_index += 1
        return series

    for group in request["groups"]:
        group_match = group["group_id"] == subject_id
        for member in group["members"]:
            color = SERIES_PALETTE[color_index % len(SERIES_PALETTE)] if group_match else "#94a3b8"
            series.append(
                {
                    "label": f"{group['group_id']} / {member['entity_id']}",
                    "metric_id": member["metric_id"],
                    "points": member["points"],
                    "highlight": group_match,
                    "color": color,
                }
            )
            if group_match:
                color_index += 1
    return series


def json_script(payload: Any) -> str:
    return json.dumps(payload, ensure_ascii=False).replace("</", "<\\/")


def truncate_text(value: str, *, limit: int) -> str:
    if len(value) <= limit:
        return value
    return value[: limit - 3].rstrip() + "..."


def format_number(value: Any) -> str:
    if value is None:
        return "-"
    number = float(value)
    if abs(number) >= 100 or float(number).is_integer():
        return f"{number:.0f}"
    return f"{number:.1f}"


def format_ts_label(ts_secs: int) -> str:
    if ts_secs % 3600 == 0 and ts_secs != 0:
        return f"{ts_secs // 3600}h"
    if ts_secs % 60 == 0:
        return f"{ts_secs // 60}m"
    return f"{ts_secs}s"


def looks_like_unix_timestamp(ts_secs: Any) -> bool:
    try:
        value = int(ts_secs)
    except (TypeError, ValueError):
        return False
    return 946684800 <= value <= 4102444800


def format_chart_ts_label(ts_secs: int, *, origin_ts_secs: int, absolute_time_axis: bool) -> str:
    if not absolute_time_axis:
        return format_ts_label(ts_secs)

    instant = datetime.fromtimestamp(origin_ts_secs + ts_secs, tz=timezone.utc)
    if ts_secs == 0:
        return instant.strftime("%H:%M")
    if instant.hour == 0 and instant.minute == 0:
        return instant.strftime("%m-%d")
    return instant.strftime("%H:%M")


def format_ts_range(start_ts_secs: Any, end_ts_secs: Any) -> str:
    if start_ts_secs is None and end_ts_secs is None:
        return "-"
    return f"{format_ts_label(int(start_ts_secs or 0))} -> {format_ts_label(int(end_ts_secs or 0))}"


def format_event_range_label(event: dict[str, Any], window_start_ts_secs: Any) -> str:
    points = event.get("timepoints_ts_secs", []) or []
    if points:
        labels = format_event_timepoints(points, window_start_ts_secs)
        if len(labels) == 1:
            return labels[0]
        return f"{labels[0]} -> {labels[-1]}"
    if window_start_ts_secs is not None:
        return format_absolute_ts_range(event.get("start_ts_secs"), event.get("end_ts_secs"), int(window_start_ts_secs))
    return format_ts_range(event.get("start_ts_secs"), event.get("end_ts_secs"))


def format_event_timepoints(timepoints_ts_secs: list[Any], window_start_ts_secs: Any) -> list[str]:
    labels = []
    for point in dedup_ints(timepoints_ts_secs):
        if window_start_ts_secs is not None:
            instant = datetime.fromtimestamp(int(window_start_ts_secs) + point, tz=timezone.utc)
            labels.append(instant.strftime("%H:%M UTC"))
        else:
            labels.append(format_ts_label(point))
    return labels


def format_absolute_ts_range(start_ts_secs: Any, end_ts_secs: Any, window_start_ts_secs: int) -> str:
    if start_ts_secs is None and end_ts_secs is None:
        return "-"
    start = datetime.fromtimestamp(window_start_ts_secs + int(start_ts_secs or 0), tz=timezone.utc)
    end = datetime.fromtimestamp(window_start_ts_secs + int(end_ts_secs or 0), tz=timezone.utc)
    if int(start_ts_secs or 0) == int(end_ts_secs or 0):
        return start.strftime("%H:%M UTC")
    return f"{start.strftime('%H:%M')} -> {end.strftime('%H:%M UTC')}"


def dedup_ints(values: list[Any]) -> list[int]:
    deduped: list[int] = []
    seen: set[int] = set()
    for value in values:
        point = int(value)
        if point in seen:
            continue
        seen.add(point)
        deduped.append(point)
    return deduped


def format_window_secs(window_secs: Any) -> str:
    if window_secs is None:
        return "-"
    seconds = int(window_secs)
    if seconds % 3600 == 0 and seconds >= 3600:
        return f"{seconds // 3600}h"
    if seconds % 60 == 0 and seconds >= 60:
        return f"{seconds // 60}m"
    return f"{seconds}s"


if __name__ == "__main__":
    raise SystemExit(main())
