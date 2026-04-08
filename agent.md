# Agent Guide

## 仓库目标

这个仓库实现一个最小可运行的 Rust 时序语义内核：

- 输入单条或同组多条 time series
- 产出稳定的 line/group 级 event 排序
- 输出一段给 LLM 使用的结构化描述
- 提供 canonical IR 与 LLM 投影的 JSON 输出
- 内置文件化回归套件，保证主要 event 的 top 结果稳定
- 采用 workspace 组织：`compiler-schema`、`compiler-core`、`compiler-bench`、root CLI

正式设计约束见：

- [docs/rust-semantic-kernel.md](/Users/aricsu/Database/compiler-rs/docs/rust-semantic-kernel.md)
- [docs/integration.md](/Users/aricsu/Database/compiler-rs/docs/integration.md)
- [docs/regression-viewer.md](/Users/aricsu/Database/compiler-rs/docs/regression-viewer.md)

## 工作边界

- 只实现 Rust 内核
- 只覆盖 `line_level_top3` 和 `group_level_top3`
- LLM 输出只保留一段 `description`
- 未来只接受 `CLI`、`WASM` 嵌入、Rust `crate` 引用三种调用方式
- `bindings/go/pkg/compilerwasm` 是仓库内正式维护的 Go host binding，用于把 WASM 嵌入 Go binary
- 不在本仓库提供独立 HTTP / REST / gRPC / sidecar / daemon 协议层
- Python 仅用于回归结果可视化，不参与回归判定

## crate 布局

- `crates/compiler-schema`: schema、IR、序列化类型
- `crates/compiler-core`: normalize、feature、segment、analyze、payload
- `crates/compiler-bench`: fixture loader、demo、regression runner
- `src/main.rs`: CLI 入口

依赖策略：

- 优先使用成熟稳定 crate 处理通用基础设施，不重复手写 CLI、错误类型和基础统计
- 当前已采用：
  - `clap`: CLI 定义与帮助输出
  - `statrs`: 基础描述统计
  - `linreg`: 线性回归斜率
- 仍保留自定义实现的部分只限于和契约强绑定的逻辑：
  - `quantile` 插值规则
  - PAA 分段
  - event 排序与裁决
  - LLM `description` 模板

接入约定：

- CLI 的 `analyze-file` / `analyze-stdin` 直接接收原始 `AnalyzeRequest` JSON
- Rust 集成优先直接引用 `compiler_rs` 或内部 workspace crates
- `WASM` 是允许的未来嵌入方向，但不应引入新的协议层
- Go 集成统一走 `bindings/go/pkg/compilerwasm`，不要在仓库里再散落新的 Go demo / 重复宿主封装
- `regress-json` 会额外给出 `line_level_top3` 与 `group_level_top3` 两层 benchmark 汇总
- `viewer-json` 会额外给出可视化需要的 per-case request/output 数据

## 常用命令

- `cargo fmt --all`
- `cargo test --workspace`
- `cargo run -- demo`
- `cargo run -- demo-json`
- `cargo run -- regress`
- `cargo run -- regress-json`
- `cargo run -- viewer-json`
- `cargo run -- analyze-file cases/demo/01-line.json`
- `cat cases/demo/01-line.json | cargo run -- analyze-stdin`
- `cargo run -- --help`
- `make viewer`
- `make viewer-no-open`
- `make viewer PORT=8766`
- `make go-binding-wasm`
- `make go-binding-test`
- `make go-binding-check`

如果当前环境的 `cargo` 走 rustup 包装层导致异常，可直接使用本机 toolchain：

- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo fmt --all`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test --workspace`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- demo`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- demo-json`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- regress`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- regress-json`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- viewer-json`
- `/Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- analyze-file cases/demo/01-line.json`
- `cat cases/demo/01-line.json | /Users/aricsu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo run -- analyze-stdin`

默认 fixture 位置：

- `cases/demo/`: 文件驱动的演示输入
- `cases/regression/`: 文件驱动的回归 case
- `bindings/go/internal/assets/`: Go binding 内嵌 wasm 与测试 fixture
- `tools/regression_viewer_py/`: Python 本地预览 server，仅做可视化

## 修改原则

- 优先稳定 event 语义与排序，再扩字段
- benchmark 以主要 event 命中与排序为准，不以长文本相似度为准
- 新增规则时，先补 regression case，再改实现
- 避免把大量固定阈值暴露到业务层
- 能直接交给成熟 crate 的通用能力，优先不要手搓
- Python 可视化层只能消费 Rust JSON，不能重复实现判定逻辑

## 验收要求

- `cargo fmt --all` 通过
- `cargo test --workspace` 通过
- `cargo run -- regress` 通过
- `cargo run -- regress-json` 中的 `line_level_top3` / `group_level_top3` 汇总符合预期
- `cargo run -- viewer-json` 输出可被可视化脚本消费
- `make viewer-no-open` 能启动本地预览 server
- 涉及 Go binding 的改动应额外通过 `make go-binding-check`
