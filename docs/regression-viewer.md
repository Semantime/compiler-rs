# Regression Viewer

## Purpose

This viewer exists only to visualize regression results.

- Rust remains the only regression judge.
- Python does not decide pass/fail.
- Python only consumes `viewer-json` and renders a local HTML report.

## Data Flow

The flow is fixed:

`cargo run -- viewer-json -> Python local server -> browser live preview`

Rust produces:

- suite summary
- `line_level_top3` / `group_level_top3`
- per-case expected vs actual events
- canonical IR
- LLM projection

Python only turns that data into:

- case filters
- curve charts
- expected/actual diff view
- evidence and description inspection

## One-Click Start

From the repo root:

```bash
make viewer
```

This command:

1. builds `compiler-rs`
2. starts a local Python server
3. opens `http://127.0.0.1:8765`
4. reloads the page automatically every 2 seconds

For local preview without automatically opening the browser:

```bash
make viewer-no-open
```

If port `8765` is occupied:

```bash
make viewer PORT=8766
```

## Boundaries

Python in this repo is intentionally limited to visualization:

- no regression oracle
- no event ranking logic
- no semantic classification logic
- no benchmark pass/fail logic
- no Rust result rewriting

If Rust output changes, the viewer should adapt to the JSON contract, not redefine the contract.
