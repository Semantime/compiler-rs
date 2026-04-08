# Integration Policy

## Supported Invocation Forms

这个仓库未来只接受三种调用方式：

- `CLI` 运行
- `WASM` 嵌入
- Rust 直接 `crate` 引用

除此以外的调用形式都不属于本仓库维护范围。

明确不提供：

- 独立 HTTP API
- REST / gRPC / RPC 服务契约
- sidecar / daemon / 长驻进程
- `api_version + kind + data` 这类 envelope 协议层

如果上层系统需要服务化、权限、缓存、调度或多语言封装，应在本仓库之外自行包装 `CLI`、`WASM` 或 Rust `crate`。

## Current Status

当前已经稳定提供：

- `CLI`
- Rust `crate`

`WASM` 是保留的嵌入方向，但当前仓库还没有承诺一套独立的 wasm 绑定层；后续如果补齐，也应只作为对现有内核能力的薄封装，而不是再引入一套新的网络/API 协议。

## CLI Contract

`analyze-file` 和 `analyze-stdin` 直接接收 `AnalyzeRequest` JSON，不再接受 envelope：

```json
{
  "scope": "line",
  "series": []
}
```

或：

```json
{
  "scope": "group",
  "groups": []
}
```

输出是 `AnalyzeResponse` 的 pretty JSON：

```json
{
  "outputs": [
    {
      "canonical": {},
      "llm": {}
    }
  ]
}
```

`demo-json`、`regress-json`、`viewer-json` 仍然保留 JSON 输出，但它们只是 CLI 子命令，不构成额外 API 契约。

## Rust Crate Entry

推荐直接复用现有 Rust 类型和函数：

- `compiler_bench::analyze_request`
- `compiler_bench::analyze_request_file`
- `compiler_core::analyze_lines`
- `compiler_core::analyze_groups`
- `compiler_bench::run_regression_suite`

root crate `compiler_rs` 继续 re-export 这些稳定入口，方便上层直接引用。
