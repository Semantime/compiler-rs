# Go Binding

`bindings/go/pkg/compilerwasm` is the Go host binding for the Rust WASM analyzer.

## Layout

- `pkg/compilerwasm`: public Go package
- `internal/assets`: embedded wasm artifact and test fixture
- `build-wasm.sh`: rebuilds `compiler_rs.wasm` from the Rust workspace root

## Rebuild WASM

```bash
./bindings/go/build-wasm.sh
```

## Test

```bash
cd bindings/go
go test ./...
```

## Use

```go
package main

import (
  "context"
  "os"

  "github.com/Semantime/compiler-rs/bindings/go/pkg/compilerwasm"
)

func main() {
  ctx := context.Background()
  wasmBytes, _ := os.ReadFile("bindings/go/compiler_rs.wasm")
  payload, _ := os.ReadFile("bindings/go/sample_request.json")

  client, _ := compilerwasm.New(ctx, wasmBytes)
  defer client.Close(ctx)

  _, _ = client.DefaultPolicyJSON(ctx)
  _, _ = client.AnalyzeJSON(ctx, payload)
}
```

## WASM ABI

The Rust module exports:

- `compiler_alloc(len) -> ptr`
- `compiler_free(ptr, len)`
- `compiler_analyze_json(ptr, len) -> status`
- `compiler_default_policy_json() -> status`
- `compiler_result_ptr() -> ptr`
- `compiler_result_len() -> len`
- `compiler_error_ptr() -> ptr`
- `compiler_error_len() -> len`

`status == 0` means success. Any non-zero status means the host should read the error buffer.
