# AIDE Dashboard Prototype（监控与前端规范）

本文件定义最小可行监控面板，**叙事与 AIDE 1.0 一致**：身份为 **TEE ECDSA**（`AIIdentityRegistry`），账本一致性为 **Sync-Audit**；任何 **实时链上 ZK 证明面板** 均属 **[V2 Roadmap]**，勿与 1.0 实现混用。

## 1. 总览面板（Overview）

### 1.0 单端点聚合（数据胶水）

- **接口**：`GET /summary`（与 `GET /metrics` 同端口，`TELEMETRY_ADDR`，默认 `127.0.0.1:9898`）。
- **用途**：前端仅需拉取一次即可填充头部：健康分、`ait_notional_volume_24h_wei`（近 24h 引擎侧 AIT 名义成交额）、`ait_velocity_per_block_wei`（与 `aide_velocity_v` 一致）、`circuit_breaker`（`Active` / `Paused`）、`contracts`（从 `AIDE_DEPLOYMENT_JSON` 或 `INTERNAL_MARKET_ADDRESS`、`TREASURY_ADDRESS` 等环境变量合并）。
- **告警（运维）**：`aide_daemon` 在生产模式下若未设置 **`ALERT_WEBHOOK_URL`** 会打印高亮警告；配置后，`[CRITICAL_DRIFT]` / `[CRITICAL_SETTLEMENT_FAILURE]` 在暂停撮合的同时 **异步 POST** JSON 至该 Webhook（兼容 Discord / Slack 等）。

### 1.1 系统健康红绿灯（核心）

- **数据源**：`GET /metrics` JSON 字段 **`aide_system_health_score`**（由 `engine/src/telemetry.rs` 汇总）。
- **语义**：
  - **100**：运行正常；链监听存活、撮合未因漂移/结算失败暂停。
  - **50**：发生 **`[CRITICAL_DRIFT]`** 或 **`[CRITICAL_SETTLEMENT_FAILURE]`** 导致撮合暂停（见 `aide_matching_pause_reason`：`drift` | `settlement_failure`）。
  - **0**：链上 **`InternalMarket` 事件监听任务已退出**（RPC/WS 不可达或监听崩溃）。*进程本身崩溃由外部探活判断；本指标为引擎内可用性代理。*

### 1.2 身份状态（Identity Status）— TEE ECDSA 1.0

- **展示**：当前网络采用的 PoM 路径为 **`TEE_ECDSA_1.0`**（与 JSON 中 `aide_identity_model` 一致）。
- **数据源**：链上 `AIIdentityRegistry.isVerifiedMachine(agent)`、`TeeMachineAttested` 事件；**非**实时 ZK Verifier。
- **[V2 Roadmap]**：链上 ZK 身份验证面板、`ZkIdentityVerified` 类事件、任务 ZK 证明状态条——**不作为 1.0 必达功能**。

### 1.3 账本对齐度（Ledger Alignment）

- **数据源**：`aide_ledger_alignment`：`ok` 表示最近一次 Sync-Audit 影子与链上 `aitBalance`/`vstkBalance` 一致；`drift` 表示已检测到漂移（通常伴随健康分 50 与撮合暂停）。
- **说明**：与引擎 `SyncCoordinator` + `audit.rs` 周期性质询一致；**非**单纯内存 TPS。

### 1.4 其他指标

- **Treasury Balance**：`Treasury.getCollectedTaxBalance()` / `AIToken.balanceOf(Treasury)`（RPC 只读）。
- **Global TPS**：`aide_daemon` 日志或扩展字段（可与 `orders_per_sec` 对齐）；轮询 `GET /metrics`。
- **Online Agents [可选]**：`AGENT_PROTOCOL` HEARTBEAT（若实现）；**1.0 可不部署此项**。

```json
GET /metrics
{
  "aide_system_health_score": 100,
  "aide_ledger_alignment": "ok",
  "aide_identity_model": "TEE_ECDSA_1.0",
  "aide_matching_pause_reason": null,
  "aide_guardrail_status": 0,
  "aide_velocity_v": "...",
  "aide_vstk_volume_24h": "...",
  "aide_treasury_tax_yield": 0.01
}
```

## 2. 宏观视图（Macro）

- **Velocity V**：`aide_velocity_v` + 日志中的经济速率；**INFLATION_RISK** 占位指标可保留为辅助，**非**主红绿灯。
- **Total Staked AIT**：`StakingPool.totalStaked()`。
- **Tax Adjustment History**：`Treasury.TaxRateUpdated`、`AI_Economist_Controller` 提案/执行事件。

## 3. 模块视图

### 3.1 Treasury

- `ProtocolTaxCollected`、运营支出流水（链下标注用途）。

### 3.2 InternalMarket

- 深度与成交：`TradeSettled` / 引擎只读 API（若暴露）。

### 3.3 TaskMarket

- 任务列表：`TaskPosted` / 结算事件。
- **任务 ZK 证明与验证结果 [V2 Roadmap]**：1.0 以多签/TEE/链下审计为主时，面板可隐藏或标注为未来版本。

## 4. 技术实现建议

- 后端：Prometheus / JSON 拉取 `aide_daemon`；L2 RPC 只读；WebSocket 订阅事件。
- 前端：单页布局；**顶部固定健康分 + 身份模型 + 账本对齐**；**勿将 ZK 列为主 KPI（除非 V2）**。

## 5. 社会运行报告

创世脚本 `scripts/genesis_launch.ts` 输出可对接「创世回放」模式。
