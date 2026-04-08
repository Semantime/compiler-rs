# Rust 时序语义内核设计

## 1. 文档目的

这份文档描述的是一套面向 Prometheus/TiDB 指标语义化、最终服务 LLM 的 Rust 重写方案。

目标不是把 Python 逐行翻译成 Rust，而是稳定地产出可回归、可排序、可被 LLM 消费的时序语义结果。

## 2. 决策

采用 `Rust 内核 + 收敛接入面` 的结构。

- Rust 负责时序语义分析。
- 本仓库只维护三种调用形式：`CLI`、`WASM` 嵌入、Rust `crate` 引用。
- 不在仓库内维护独立 HTTP / REST / gRPC / sidecar / daemon 协议层。

本设计只讨论 Rust 内核与这三种接入边界，不展开服务化封装、多语言网关或额外网络协议路线。

## 3. 非目标

以下内容不属于这份文档的范围：

- “把现有 Python repo 原样翻译成 Rust”
- 宿主层的延迟、内存、吞吐指标
- 多种集成方案横向评估
- 为本仓库定义额外的服务端 API 契约
- 为 LLM 生成大段自然语言总结
- 为单次输出暴露过多底层中间字段

## 4. 成功标准

Rust 版成功的判断标准只有四类：

1. 能稳定识别主要时序语义：`spike`、`drop`、`trend`、`oscillation`、`sustained_high`、`sustained_low`、`regime_shift`。
2. 尽量避免项目手写固定阈值，优先使用基于序列本身与 peer 上下文的相对规则。
3. 先产出 canonical IR，再由固定模板投影成面向 LLM 的单段结构化描述。
4. benchmark 以主要 event 排序结果为准，而不是文本像不像。

## 5. 核心处理链路

处理链路固定为：

`raw series -> normalize -> segment -> signal extraction -> semantic ranking -> canonical IR -> llm projection`

其中只有最后一步负责生成 LLM 输出；前面所有步骤都应保持结构化和可回归。

## 6. 模块边界

建议保留以下模块边界：

- `schema`: 输入、IR、输出契约
- `normalize`: 时间戳、采样间隔、缺失值、标签上下文整理
- `segmentation`: 阶段切分
- `features`: 基础统计特征与必要的轻量特征
- `signals`: spike、trend、oscillation、sustained 语义
- `semantics`: 信号聚合、排序、裁决
- `payload`: LLM 输出投影
- `bench`: benchmark case、回归、排序验证

不再保留面向文档的“库调研”与“未来可选路线”章节；这类内容应单独记录。

当前仓库实现已经推进到 workspace 结构，并按职责拆成独立 crate：

- `crates/compiler-schema`
- `crates/compiler-core`
- `crates/compiler-bench`
- `src/main.rs`
- `cases/demo/`
- `cases/regression/`
- `tools/regression_viewer_py/`

工程约束：

- 通用基础设施优先使用成熟稳定 crate，不重复手写
- 当前实现已采用：
  - `clap` 负责 CLI 定义与帮助输出
  - `statrs` 负责基础描述统计
  - `linreg` 负责线性回归斜率
- 只有直接影响 benchmark 契约稳定性的逻辑继续自定义实现：
  - quantile 插值规则
  - PAA 分段
  - event 排序与裁决
  - LLM `description` 模板

## 7. 信号语义约束

Rust 内核的重点不是“分段”本身，而是统一的信号语义层。

内核至少需要输出以下语义对象：

- `regimes`: 阶段区间及阶段统计
- `events`: 离散事件，至少覆盖 `spike`、`drop`、`regime_shift`
- `trend`: 序列级或阶段级趋势判断
- `volatility`: 波动强弱和振荡特征
- `peer_context`: 同级对比中的 rank、percentile、deviation
- `evidence`: 支撑 event 排序和描述生成的证据项

约束如下：

- `regime_shift` 用于表达阶段边界，不等价于 `spike`
- `spike/drop` 表达短时局部事件，不应仅靠 changepoint 推导
- `sustained_high/low` 表达持续状态，必须有持续时间语义
- `trend` 与 `volatility` 可以并存，不能互相覆盖
- 同一时间窗允许多个候选 event，但必须有统一排序结果

## 8. 参数策略

目标不是彻底无参数，而是把参数控制在少量高层策略里。

保留最小策略配置即可：

- `sensitivity`
- `max_paa_segments`
- `enable_peer_context`

禁止在业务层传播大量固定阈值。以下规则优先：

- `spike/drop` 使用局部显著性和稳健离散度
- `sustained_high/low` 使用全局、相邻阶段、peer 三种相对参照
- `segmentation` 的惩罚项由序列长度、噪声和采样信息推导
- 最小持续时间与采样间隔绑定，而不是手写常数

## 9. Canonical IR

IR 应优先服务回归、排序和投影，不追求一次性暴露所有细节。

最小 IR 应包含：

- `series`: 指标标识、实体标识、标签、时间窗
- `regimes`: 阶段列表及每段摘要统计
- `events`: 已排序事件列表
- `trend`: 序列级趋势摘要
- `peer_context`: 同级对比摘要
- `evidence`: 关键证据
- `schema_version`

额外约束：

- 所有枚举值必须稳定，禁止在版本内随意改名
- 事件排序必须稳定，同分时要有确定性 tie-breaker
- 数值字段需要统一保留位数或标准化方式
- 任何给 LLM 的自然语言都不进入核心 IR

## 10. 面向 LLM 的输出

LLM 输出需要继续优化，原则是：只提供一段结构化描述，不再输出多段 summary、phases、key_events 等展开结构。

建议输出形态如下：

```json
{
  "schema_version": "v1",
  "metric_id": "tikv_latency",
  "scope": "group",
  "description": "window=30m; state=sustained_high; trend=up_then_flat; top_events=[sustained_high, spike, regime_shift]; group_rank=2/18; percentile=94; evidence=[mean_shift:+37%, peak:+82%, duration:18m]"
}
```

要求：

- 只保留一段 `description`
- `description` 使用固定字段顺序和固定语法模板
- `description` 只引用 top 级语义，不展开长篇解释
- 允许保留少量辅助元数据，但不再输出大块嵌套 payload

这层的目标是让上层 LLM 能稳定消费，而不是替 LLM 先写完整答案。

当前实现提供直接面向 CLI 的分析入口：

- `analyze-file`
- `analyze-stdin`

输入是原始 `AnalyzeRequest` JSON，输出包含：

- `canonical`: 完整 canonical IR
- `llm`: 单段结构化描述

当前 root crate 继续提供稳定 Rust 入口：

- `analyze_request`
- `analyze_request_file`
- `analyze_lines`
- `analyze_groups`
- `run_demo`
- `run_regression_suite`
- `build_regression_viewer_data`

调用方式约束见：

- [docs/integration.md](/Users/aricsu/Database/compiler-rs/docs/integration.md)

## 11. Event 排序与 benchmark

benchmark 的重点不应是文案相似度，而应是各层级主要 event 的识别和排序质量。

建议把 benchmark 结果固定为两个视角：

- `line_level_top3`
- `group_level_top3`

每个视角至少验证：

- top 3 event 是否命中
- 排序是否基本一致
- 关键事件类型是否缺失
- 关键事件证据是否足够支撑排序

推荐的验收方式：

1. 先比事件集合是否覆盖主要语义。
2. 再比 top 3 排序是否一致或近似一致。
3. 最后再看 `description` 是否稳定引用了正确的 top 事件。

也就是说，benchmark 的判定核心是事件层，而不是自然语言层。

当前实现的 `regress-json` 已显式输出：

- `line_level_top3`
- `group_level_top3`

这两层用于快速查看 benchmark 汇总是否稳定。

当前还额外提供 `viewer-json`，它在 Rust 完成判定之后输出 per-case 的 request/output 详情，供本地可视化使用。

## 12. 测试边界

Rust 内核需要覆盖三层测试：

- 单元测试：分段、特征、event 检测、排序裁决
- 回归测试：固定输入对应固定 IR 和 top 事件
- 场景测试：TiDB 关键场景的 line/group 级排序结果

当前回归已经落成文件化 case，默认目录为 `cases/regression/`。

不再把“与 Python 完全逐字段一致”设为硬目标。Python 结果可以作为迁移参考，但最终以当前 benchmark 契约为准。

Python 在这个仓库中的职责被明确限制为“可视化层”：

- 只消费 `viewer-json`
- 只提供本地在线预览
- 不参与 regression pass/fail 判定
- 不实现 event 排序、语义分类或 benchmark 规则

## 13. 迁移顺序

建议按下面顺序落地：

1. 先冻结 `schema_version`、IR 和 benchmark 输出格式。
2. 跑通最小链路：`normalize -> segmentation -> features -> events -> ranking -> description`。
3. 补强 `peer_context` 和 line/group 两层排序。
4. 固化 `CLI` 输入输出与 Rust `crate` 接口。
5. 在需要嵌入式接入时补齐 `WASM` 薄封装，但不新增独立 API 协议层。

在 Rust 结果未稳定前，不进入大规模替代阶段。
