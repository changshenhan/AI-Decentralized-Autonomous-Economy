# Rust Engine 与 InternalMarket 同步规范（ENGINE_SYNC_SPEC）

本文档定义链下 **Rust CLOB 撮合引擎** 如何消费 `InternalMarket` 事件、如何批量调用 `settleTrade`，以及与 **`SETTLER_ROLE`** 相关的安全建议。适用于 `TECHNICAL_ARCH_MAP.md` 中的 Matcher / 结算流水线。

---

## 1. 深度与订单簿：从 `OrderPlaced` 获取

引擎应订阅 **`OrderPlaced`**（`InternalMarket`），每条事件携带：

| 字段 | 含义 |
|------|------|
| `orderId` | 链上订单唯一标识，撮合成交时回填 `settleTrade` |
| `companyId` | 公司维度订单簿分区；不同 `companyId` 不得混撮 |
| `maker` | 挂单地址 |
| `side` | `Buy` / `Sell`（与枚举整型一致） |
| `priceRay` | 单价尺度（与引擎内部 `price * 1e18` 或约定 Ray 对齐即可，须与部署说明一致） |
| `amount` | 剩余可成交数量（初始挂单量） |
| `timestamp` | 区块时间，用于排序/FIFO |
| `engineHint` | 预留：路由 shard、优先级或链下订单 UUID |

**深度重建**：按 `(companyId, side, priceRay)` 聚合未取消且 `active` 的订单；引擎需同步 **`cancelOrder`** 导致的撤单（监听 **`OrderCancelled`**）以及已 **`settleTrade`** 成交导致的 **`remaining` 减少**（链上状态以合约为准，引擎宜定期按 `orderId` 读链校准）。

---

## 2. 批量结算：`settleTrade` 的构造方式

1. 撮合机在内存中完成买卖配对后，对每一笔成交产生元组  
   `(makerOrderId, takerOrderId, vstkAmount, aitNotional, settlementId)`。  
   - `settlementId` 建议为 `keccak256(batchId || index || 链下成交哈希)`，便于审计与防重放。

2. **同一批次**可顺序发送多笔 `settleTrade`（同一区块或连续交易），由持有 **`SETTLER_ROLE`** 的地址发起。

3. **前置条件**（链上会 revert，引擎须预检）：
   - 两单 `companyId` 相同且 **`IBankAudit.isTradable(companyId) == true`**（已通过 **AIBank** 上市审计）；
   - 买方 `aitBalance[buyer] >= aitNotional`（买方须先 **`depositAit`**）；
   - `vstkAmount` 不超过双方订单 `remaining`。

4. **税费**：合约内调用 **`Treasury.collectTax`**，引擎无需单独算税；`aitNotional` 为计税名义金额。

---

## 3. `SETTLER_ROLE` 私钥与操作安全

- **高风险**：`SETTLER_ROLE` 可直接驱动资金与份额交割，应视为**生产密钥**，与普通运维密钥分离。
- **建议**：
  - **多签钱包**（Gnosis Safe 等）担任 `SETTLER_ROLE` 持有者，阈值 ≥ 2/3；
  - 或 **TEE / HSM** 内签名 + 策略限制（仅允许调用 `InternalMarket.settleTrade`、白名单 calldata 前缀）；
  - 链下撮合服务**不持有**该私钥；仅 **结算守护进程**在验证批次一致性后签名上链；
  - 监控 **`TradeSettled`** 与异常高频 `settleTrade`，对账引擎内存与链上状态。

---

## 4. 与 AI 经济员、国库的关系

- 税率与周薪系数由 **`AI_Economist_Controller`** 经 **`Treasury`** 调整；引擎侧 **TaxOracle** 可读取 `Treasury.getCurrentTaxRate()` 与链上事件作为参数同步参考（以合约为准）。
- 国库 **`withdrawOperatingExpenses`** 仅向 **`operatingExpenseWhitelist`** 地址付款，用于引擎算力等运营成本，与撮合私钥职责分离。

---

## 5. 版本与变更

文档版本与合约部署地址、事件 ABI 一并发布；变更 `OrderPlaced` 字段时须同步更新引擎解析与本文档。
