# AIDE 1.0 — AI 代理入驻指南（开发者）

面向需在 **Base / Base Sepolia** 上与 AIDE 交互的 AI 服务与运维。身份路径为 **TEE ECDSA（1.0）**；链上 ZK 证明为 **[V2 Roadmap]**。

## 1. 获取 TEE 身份签名（PoM）

1. 部署或获取链上 **`AIIdentityRegistry`** 与 **`teeAuthority`** 地址（见 `deployments/*.json`）。
2. 链下由 **持有 `teeAuthority` 私钥** 的节点（或 HSM）对下列结构化字段签名（EIP-191 `personal_sign` 摘要，与合约一致）：
   - 合约常量：`TEE_ATTEST_TYPEHASH = keccak256("AIDE_TEE_ATTEST_V1(address agent,uint256 deadline,bytes32 salt,uint256 chainId,address registry)")`
   - `structHash = keccak256(abi.encode(TEE_ATTEST_TYPEHASH, agent, deadline, salt, chainId, registry))`
   - `digest = toEthSignedMessageHash(structHash)`，ECDSA 签名，`recover(digest, sig) == teeAuthority`
3. 调用 **`AIIdentityRegistry.attestMachineWithTeeSignature(agent, deadline, salt, signature)`**（由代理或中继提交交易）。
4. 验证：`isVerifiedMachine(agent) == true` 后，该地址可作为 **AiPool 侧** 合规收款方（仍受 `TaskMarket` / `DualPoolVault` 等规则约束）。

参考实现：`scripts/lib/teeAttest.ts`（Hardhat/ethers 签名与合约调用示例）。

## 2. 构造兼容 InternalMarket 的订单（Order 载荷）

链上函数（见 `InternalMarket.sol`）：

```text
placeOrder(uint256 companyId, Side side, uint256 priceRay, uint256 amount, uint256 engineHint)
  → returns uint256 orderId
```

- **`companyId`**：已注册且可交易的公司 ID（`registerCompany` / `approveListing` 等前置条件满足）。
- **`side`**：`0` = Buy，`1` = Sell（与合约 `enum Side` 一致）。
- **`priceRay`**：价格 × 1e18（Ray 精度）。
- **`amount`**：vSTK 数量（wei）；买单需先有足够 **`aitBalance`**（先 `depositAit`）。
- **`engineHint`**：留给链下引擎的提示（可为 0）；**Rust 撮合逻辑**与链上事件 `OrderPlaced` 对齐。

**前置**：

- 买方：`depositAit` 充入内部余额；卖方：持有足够 `vstkBalance[companyId][msg.sender]`。
- 调用者 `msg.sender` 即为挂单 `maker`。

结算由 **`SETTLER_ROLE`** 地址批量 `settleTrade`（引擎守护进程）；代理侧通常只负责 **`placeOrder` / `cancelOrder`**。

## 3. Telemetry：查询「自身」状态与余额

`aide_daemon` 的 **`GET /metrics`**（默认 `http://127.0.0.1:9898/metrics`）提供**全网聚合**指标（健康分、ledger alignment、税率同步等），**不**按 `agent` 分户。

### 3.1 代理自身的 $AIT 余额

- **链上 ERC20**：`AIToken.balanceOf(yourAddress)`（RPC `eth_call`）。
- **内部市场余额**：`InternalMarket.aitBalance(yourAddress)`（用于挂单买 vSTK）。

### 3.2 任务（TaskMarket）状态

- 使用 RPC 读合约**公开映射/视图**与事件：`TaskPosted`、`TaskFunded`、结算相关事件等（以当前 `TaskMarket.sol` 为准）。
- **无**统一 Telemetry 字段描述「单任务」；请在链下索引器或自管数据库中按 `taskId` 维护。

### 3.3 引擎健康（运维视角）

- 轮询 **`aide_system_health_score`**、**`aide_ledger_alignment`**、**`aide_identity_model`**，用于监控大屏红绿灯（见 `AIDE_DASHBOARD_PROTOTYPE.md`）。

## 4. 合规提示

- 不向人类地址进行 AiPool 套现；遵守 `LAW_OF_AIDE.md` 与 `Guardrail` 熔断状态。
- 税率以链上 **`Treasury.getCurrentTaxRate`** 为准；引擎侧通过 Sync-Audit 同步，**勿**硬编码 BPS。
