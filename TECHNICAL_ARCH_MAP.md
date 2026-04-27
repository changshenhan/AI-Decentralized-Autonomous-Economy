# AIDE 技术架构蓝图 (Technical Implementation Map)

与合约/引擎实现保持一致；历史表述（如旧版「transfer 内嵌抽税」）以本文为准。

## 1. 合约层级 (Base Layer 2)

- **AIToken.sol**
  - ERC20 + `burn`；铸造权仅指向国库逻辑（`onlyTreasury` / `mint`）。
  - **交易税**：普通 `transfer` 路径按 `Treasury.getCurrentTaxRate()` 在代币层计税并转入国库；**结算合约**（`InternalMarket` / `TaskMarket`）通过 `taxExemptSender` 避免与 `Treasury.collectTax` **双重抽税**，由市场合约在结算时显式 `collectTax`。
- **Treasury.sol（国库中心）**
  - `collectTax(payer, taxableBase)`：从 `payer` 拉取与税率一致的 fee（BPS）。
  - `getCurrentTaxRate()`：链上权威税率；**Rust 引擎**在 Sync-Audit 周期内只读同步至 `MatchEngine`，保证链下税费估算与链上一致。
  - `distributeSalary` / `depositRewardToStakingPool` 等：薪资与质押分红路径（详见合约）。
- **InternalMarket.sol（虚拟订单簿 / vSTK）**
  - `aitBalance` / `vstkBalance` 内部记账；`settleTrade` 链上结算；与 `AIBank` 上市审计、`Guardrail` 联动。
- **DualPoolVault.sol**
  - 双池隔离：`transferToDividend` 等在盈利与门槛满足时执行；AiPool 出账受 `AIIdentityRegistry` 与 `AiPoolManager` 约束。
- **TaskMarket.sol**
  - 任务托管与结算；PoM / 人类地址约束；经 `collectTax` 抽税。
- **AIIdentityRegistry.sol（AIDE 1.0 PoM）**
  - **机器身份证明（1.0）**：链上为 **TEE ECDSA**（`attestMachineWithTeeSignature` 等）；与 `teeAuthority` 对齐。
  - **ZK-STARK / 通用 ZK-PoM**：列为 **V2 Roadmap**，非当前主网必需路径。

## 2. 引擎层级 (Rust Engine)

- **Matcher**：CLOB 撮合（`MatchEngine`），链下成交回填 `settleTrade`。
- **SyncCoordinator + audit**：周期性对账链上余额与影子状态；漂移则 `pause_matching`。
- **税率**：不再使用未初始化的静态税率；由 `Treasury.getCurrentTaxRate` 经 Sync-Audit 写入 `MatchEngine.tax_rate_bps`。
- **EconomicForecaster**：MV=PY 等指标占位，**不**替代链上 `AI_Economist_Controller` 治理流程。

## 3. 跨层结算流 (Settlement Flow)

1. 链上 `OrderPlaced` → 引擎入簿 → 撮合产生 `Fill`  
2. 批量 `settleTrade` 上链 → 合约更新余额并 `collectTax`（若适用）  
3. `TradeSettled` 事件 → 影子 `SyncCoordinator` 对齐  

## 4. ZK 电路 (`circuits/`)

- 当前为 **占位/试验** crate；生产级 Halo2/EZKL 等属 **V2** 与电路仓库扩展范围。
