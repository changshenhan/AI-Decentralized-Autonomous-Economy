# AIDE 主网 / Base Sepolia 运维手册

本文档描述第十二阶段「主网起飞前夜」后的**运行配置、密钥与应急流程**。不涉及对外融资或法币出入金（见 `LAW_OF_AIDE.md`）。

## 1. RPC 与网络

| 网络 | ChainId | 环境变量（RPC） |
|------|---------|-----------------|
| Base Sepolia | 84532 | `BASE_SEPOLIA_RPC_URL` |
| Base Mainnet | 8453 | `BASE_MAINNET_RPC_URL` |

Hardhat 网络名：`baseSepolia`、`base`（见 `hardhat.config.ts`）。

## 2. API Keys 与验证

| 用途 | 环境变量 | 说明 |
|------|----------|------|
| Basescan 合约验证 | `BASESCAN_API_KEY` | 部署脚本中 `verify:verify` 使用；未设置时验证步骤会跳过并打印 `[verify:skip]` |
| 部署者私钥 | `DEPLOYER_PRIVATE_KEY` | **仅** CI/运维机密存储，勿提交仓库 |
| TEE 签名私钥（可选 bootstrap） | `TEE_SIGNER_PRIVATE_KEY` | 须与 `TEE_AUTHORITY_ADDRESS` / 链上 `teeAuthority` 地址一致 |

## 3. Rust 引擎（`aide_daemon`）环境变量

| 变量 | 说明 |
|------|------|
| `ETH_RPC_URL` | JSON-RPC HTTP，用于链监听、`settleTrade` 发送、Guardrail 轮询 |
| `ETH_WS_URL` | 可选；若设置则 `Provider` 使用 WebSocket 传输（仍按块 `eth_getLogs`） |
| `INTERNAL_MARKET_ADDRESS` | `InternalMarket` 合约地址 |
| `TREASURY_ADDRESS` | `Treasury` 合约地址（Sync-Audit 拉取 `getCollectedTaxBalance`） |
| `GUARDRAIL_ADDRESS` | 熔断合约，用于遥测 `paused` |
| `AIDE_USE_CHAIN_LISTENER` | 设为 `1` 启用真实 `OrderPlaced` / `TradeSettled` / `OrderCancelled` 监听 |
| `AIDE_FROM_BLOCK` | 可选；监听起始块 |
| `AIDE_SYNC_AUDIT_INTERVAL_BLOCKS` | 可选；链上影子对账周期，默认 `50` |
| `AIDE_SYNC_AUDIT_DRIFT_THRESHOLD_WEI` | 可选；单字段允许偏差（wei），默认 `0`（严格相等） |
| `SETTLER_PRIVATE_KEY` | 链上 `settleTrade` 签名私钥（须持有 `SETTLER_ROLE`） |
| `CHAIN_ID` | 默认 `84532`（Base Sepolia） |
| `TELEMETRY_ADDR` | 默认 `127.0.0.1:9898`（`GET /summary` 与 `/metrics` 同端口） |
| `ALERT_WEBHOOK_URL` | 可选；**强烈建议生产配置**。`[CRITICAL_DRIFT]` / `[CRITICAL_SETTLEMENT_FAILURE]` 异步 POST JSON 告警（Discord/Slack 等）；未配置时生产模式启动会打印**无告警监控**警告 |
| `AIDE_DEPLOYMENT_JSON` | 可选；指向 `deployments/*.json` 绝对路径，供遥测 `GET /summary` 填充 `contracts` |
| `AIDE_PRODUCTION_MODE` | 设为 `1` 时：`aide_daemon` **强制**链上监听、要求 `ETH_RPC_URL` / `INTERNAL_MARKET_ADDRESS` / `TREASURY_ADDRESS` / `GUARDRAIL_ADDRESS` / `SETTLER_PRIVATE_KEY` 齐全，否则 **panic**；并自动等价于 `AIDE_USE_CHAIN_LISTENER=1` |

## 4. 部署产物

- `npm run deploy:sepolia` 等价于 `hardhat run scripts/deploy_final.ts --network baseSepolia`。
- 地址清单写入 `deployments/base_sepolia.json`（主网为 `deployments/base.json`）。
- **DualPoolVault**、`registerCompany(vault)` 与 **`PRODUCTION_ADMIN_ADDRESS` 权限移交** 均在 `deploy_final.ts` 中完成（若设置了生产地址）。

### 4.1 CREATE2 确定性核心地址（可选）

- 设置环境变量 `AIDE_USE_CREATE2=1` 时，`deploy_final.ts` 通过 `AideCreate2Factory` 以固定 **salt** 部署 `Guardrail`、`AIToken`、`AIBank`、`Treasury`、`InternalMarket`（顺序满足构造函数依赖）。
- **同一工厂合约地址** 是跨链同址的前提：建议使用**专用部署 EOA**，在 Base Sepolia 与 Base Mainnet 上均以 **相同 nonce 序列** 首笔部署 `AideCreate2Factory`，或设置 `CREATE2_FACTORY_ADDRESS` 指向已部署的工厂。
- Salt 定义见 `scripts/lib/create2Deploy.ts`（`C2_SALT.*`）。

## 5. 紧急熔断与恢复

1. **现象**：`Guardrail.paused == true`，大量交易/revert，`aide_daemon` 遥测 `aide_guardrail_status` 非 0。
2. **确认**：在区块浏览器或 RPC 上读 `Guardrail.paused()`；查阅 `CircuitBroken` 事件 reason。
3. **恢复**：由持有 `DEFAULT_ADMIN_ROLE` 或 `EMERGENCY_COMMITTEE_ROLE` 的地址调用 `Guardrail.unpause()`（见合约权限）。
4. **链下**：引擎应停止发送 `settleTrade`；待 unpause 后恢复。

## 6. TEE 权威私钥轮换（高敏感）

1. 在链下生成新密钥对，**新地址**作为下一任 `teeAuthority`（需治理流程，此处仅技术步骤）。
2. 部署新 `AIIdentityRegistry` **或** 若合约支持 `teeAuthority` 升级则走升级路径（当前合约为 **immutable `teeAuthority`**，轮换需**新 Registry 部署 + 依赖 Registry 的合约协同迁移**——生产前应在治理设计中固定轮换策略）。
3. 更新链下 `TEE_SIGNER_PRIVATE_KEY` / KMS，与链上 `teeAuthority` 对齐。
4. 使用 `scripts/lib/teeAttest.ts` 对新代理人批量重签并调用 `attestMachineWithTeeSignature`。

## 7. 自动化审计

- `npm run mainnet:check`：先运行 Rust `audit::` 单测，再同进程创世 + `scripts/mainnet_check.ts`（需与 RPC 一致的 `deployments/*.json`）。
- 通过时在控制台输出 **`[PASS] Sync-Audit: Shadow State Match`**（合约拓扑 + 引擎对账阈值逻辑）。
- 主网单独校验：设置 `AIDE_*` 环境变量或 `DEPLOYMENT_FILE` 指向已部署 JSON 后运行 `mainnet_check.ts`。

## 8. 灾难恢复：Sync-Audit `[CRITICAL_DRIFT]`

当 `aide_daemon` 日志出现 **`[CRITICAL_DRIFT]`** 时，说明 `SyncCoordinator` 影子余额与链上 `InternalMarket.aitBalance` / `vstkBalance`（及遥测中的 `Treasury.getCollectedTaxBalance` 读数）在阈值外不一致，`MatchEngine` 已 **`pause_matching()`**，且 `match_paused` 被置位，撮合与下游结算应视为**冻结**，避免坏账扩大。

### 8.1 立即动作

1. **确认链尖与 RPC**：排除节点延迟或错误 `INTERNAL_MARKET_ADDRESS` / `TREASURY_ADDRESS`。
2. **导出漂移明细**：日志中会打印至多 8 条 `user / field / local / chain`；保存用于事后审计。
3. **停止对外宣称「正常交易」**：通知运维切换只读模式（若适用）。

### 8.2 人工对账

1. 对漂移涉及的每个 `(address)` 与 `(companyId, address)`，在区块浏览器或 `cast call` 读取链上 `aitBalance`、`vstkBalance`。
2. 核对引擎侧 `SyncCoordinator` 影子更新是否**仅**来自已确认的 `TradeSettled`（或是否与 `settleTrade` 批量结算顺序一致）。
3. 若链上为真、影子错误：定位为**链下重复计数 / 漏事件 / 重组**；修复索引或重放事件。
4. 若影子为真、链上错误：属**合约或结算路径异常**，禁止在未理解根因前 `resume_matching`。

### 8.3 重新同步 MatchEngine 状态

1. **清空或重建**内存订单簿与影子映射（按运维策略：冷启动或从某一安全块高重放 `OrderPlaced` / `TradeSettled` / `OrderCancelled`）。
2. 将 `SyncCoordinator` 的影子表与**同一链上快照**对齐（可基于 `InternalMarket` 全量只读扫描或受控 API）。
3. 验证：`run_sync_audit_once`（或 daemon 周期任务）连续多轮输出 **`Sync-Audit: Shadow State Match`**。
4. 调用 **`MatchEngine::resume_matching()`**（需在进程内暴露或通过重启加载「已确认恢复」标志），并将 `match_paused` 清零。

### 8.4 国库向质押池分红路径

- 链上应使用 `Treasury.depositRewardToStakingPool(stakingPool, amount)`（由 `TREASURY_OPS_ROLE` 调用）；必要时先 `approveStakingPoolPull` 保持 `AIT` 对 `StakingPool` 的 allowance。`mainnet:check` 会校验 `Treasury → StakingPool` 的 allowance 非零（运维可保留长期 `approve`）。
