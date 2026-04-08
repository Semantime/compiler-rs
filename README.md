# compiler-rs

`compiler-rs` is a Rust semantic kernel for time-series analysis.

It takes either a single series or a group of related series and produces:

- stable ranked events such as `spike`, `drop`, `trend`, `oscillation`, `sustained_high`, `sustained_low`, and `regime_shift`
- a canonical JSON IR for programmatic use
- a compact LLM-facing description string

This repository supports three integration boundaries:

- CLI
- Rust crate
- WASM embedding

It does not provide an HTTP, REST, gRPC, sidecar, or daemon service layer.

## Quick Start

Requirements:

- Rust stable toolchain
- `cargo`
- optional: `python3` for the local regression viewer
- optional: Go, only if you work on the Go binding

Build:

```bash
cargo build
```

Use these first:

```bash
make test
make viewer-json
make viewer-no-open
make go-binding-check
cargo run -- analyze-file cases/demo/01-line.json
cargo run -- --help
```

If you run the binary without a subcommand, it defaults to:

```bash
cargo run -- demo
```

## Everyday Commands

- `make test`: run all workspace tests
- `make viewer-json`: emit JSON for the regression viewer
- `make viewer` / `make viewer-no-open`: start the local viewer
- `make go-binding-check`: rebuild the wasm artifact and test the Go binding
- `cargo run -- analyze-file <path>`: analyze one JSON request file
- `cargo run -- analyze-stdin`: read a request from stdin and analyze it
- `cargo run -- regress`: run the regression suite with text output
- `cargo run -- regress-json`: run the regression suite with JSON output

## Input

The CLI accepts raw `AnalyzeRequest` JSON with no extra envelope.

Line-level requests use `scope = "line"` and `series`:

```json
{
  "scope": "line",
  "series": [
    {
      "metric_id": "cpu",
      "entity_id": "host-1",
      "group_id": "cluster-a",
      "points": [
        { "ts_secs": 0, "value": 1.0 },
        { "ts_secs": 60, "value": 1.2 },
        { "ts_secs": 120, "value": 4.8 }
      ]
    }
  ]
}
```

Group-level requests use `scope = "group"` and `groups`:

```json
{
  "scope": "group",
  "groups": [
    {
      "metric_id": "cpu",
      "group_id": "cluster-a",
      "members": []
    }
  ]
}
```

Sample inputs:

- `cases/demo/01-line.json`
- `cases/demo/02-group.json`

## Output

The output is `AnalyzeResponse` JSON with two main parts:

- `canonical`: stable IR for downstream code and regression checks
- `llm`: compact structured text for LLM consumers

Example shape:

```json
{
  "outputs": [
    {
      "canonical": {
        "schema_version": "v1",
        "scope": "line",
        "metric_id": "cpu",
        "subject_id": "host-1",
        "state": "elevated",
        "trend": "increasing",
        "top_events": [
          { "kind": "sustained_high" },
          { "kind": "regime_shift" },
          { "kind": "spike" }
        ]
      },
      "llm": {
        "schema_version": "v1",
        "metric_id": "cpu",
        "scope": "line",
        "subject_id": "host-1",
        "description": "window=11m; state=elevated; trend=increasing; top_events=[sustained_high, regime_shift, spike]"
      }
    }
  ]
}
```

## Rust Usage

If you want the JSON boundary from Rust:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::fs::read_to_string("cases/demo/01-line.json")?;
    let output = compiler_rs::analyze_request_json(&input)?;
    println!("{output}");
    Ok(())
}
```

Stable Rust entry points include:

- `compiler_rs::analyze_request`
- `compiler_rs::analyze_request_file`
- `compiler_rs::analyze_lines`
- `compiler_rs::analyze_groups`
- `compiler_rs::CompilerPolicy`

## Layout

- `crates/compiler-schema`: schemas, IR, output types
- `crates/compiler-core`: normalization, features, segmentation, analysis, payload projection
- `crates/compiler-bench`: demos, fixtures, regression runner, viewer data
- `src/lib.rs`: public re-exports and JSON/WASM boundary
- `src/main.rs`: CLI entry point
- `cases/demo/`: demo inputs
- `cases/regression/`: regression cases
- `tools/regression_viewer_py/`: local viewer
- `bindings/go/`: Go host binding over the WASM ABI

## Validation

Recommended checks before merging:

```bash
make test
cargo run -- regress
cargo run -- regress-json
```

If you touched the viewer or Go binding:

```bash
make viewer-no-open
make go-binding-check
```

## More Docs

- [Semantic kernel design](docs/rust-semantic-kernel.md)
- [Integration policy](docs/integration.md)
- [WASM export](docs/wasm.md)
- [Regression viewer](docs/regression-viewer.md)
- [Go binding](bindings/go/README.md)
