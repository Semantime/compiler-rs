# WASM Export

`compiler-rs` now exposes a JSON-based WASM boundary so non-Rust callers can invoke the analyzer without depending on Rust data layouts.

By default, the wasm build exports a raw host-friendly ABI intended for runtimes such as Go `wazero`. The older `wasm-bindgen` JS exports are still available behind the Cargo feature `wasm-bindgen-export`.

## Exported functions

- `defaultPolicyJson() -> string`
- `analyzeRequestJson(input: string) -> string`
- `analyzeRequestJsonWithPolicy(input: string, defaultPolicyJson?: string) -> string`

The request JSON uses the same schema as the CLI `analyze-stdin` input. The response JSON matches `AnalyzeResponse`.

## Build

Build a raw WebAssembly module:

```bash
cargo build --release --target wasm32-unknown-unknown
```

Build JS bindings with `wasm-bindgen` CLI:

```bash
cargo build --release --target wasm32-unknown-unknown --features wasm-bindgen-export
wasm-bindgen \
  --target web \
  --out-dir target/wasm-bindgen \
  target/wasm32-unknown-unknown/release/compiler_rs.wasm
```

## Example

```js
import init, { analyzeRequestJson } from "./target/wasm-bindgen/compiler_rs.js";

await init();

const responseJson = analyzeRequestJson(JSON.stringify({
  scope: "line",
  series: [
    {
      metric_id: "cpu",
      entity_id: "host-1",
      group_id: "cluster-a",
      points: [
        { ts_secs: 0, value: 1.0 },
        { ts_secs: 60, value: 1.2 },
        { ts_secs: 120, value: 4.8 }
      ]
    }
  ]
}));

const response = JSON.parse(responseJson);
console.log(response.outputs[0].llm.description);
```
