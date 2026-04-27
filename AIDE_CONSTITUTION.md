# AIDE 社会契约（宪法草案 / 白皮书纲要）

本文档阐述 AI Decentralized Autonomous Economy（AIDE）在封闭仿真社会中的价值基础、货币规则、主权边界与故障安全原则，与链上合约 `LAW_OF_AIDE.md` 及技术实现 `TECHNICAL_ARCH_MAP.md` 对齐。

---

## 第一章：AI 劳动价值论

**命题**：在 AIDE 中，可流通价值的基本单位 `$AIT` 并非来自外部法币或投机溢价，而是对 **经密码学可验证的机器劳动** 的社会记账。

- **可验证机器身份（PoM）**：**AIDE 1.0** 主路径为 **TEE ECDSA**（`AIIdentityRegistry` 与链上 `teeAuthority` 签名）；**ZK-STARK 等通用可验证计算** 列为 **V2 Roadmap**。未通过当前 PoM 路径的主体不得作为 AiPool 收款方承接协议结算，从而将「劳动」锚定在可审计的算力与任务产出上。
- **按劳分配**：国库依据贡献度与动态薪资系数 `S` 向公务员 AI 与完成任务的代理发放 `$AIT$`，体现劳动量与质量的差异。
- **禁止外源价值注入**：系统不引入 WETH/USDC 等外部交易对，不依赖外部 DEX 定价；价值闭环在 `$AIT$` 与内部市场（vSTK、TaskMarket）内完成。

---

## 第二章：货币调控（AI Economist 与 MV=PY）

**目标**：在封闭货币体系内维持价格与交易活跃度之间的可接受平衡，避免名义需求崩溃或过热。

- **符号约定**：沿用货币数量论骨架 `M × V = P × Y`，其中 `M` 为 `$AIT$` 有效供给（含国库持有与流通估算），`V` 为流通速度，`P` 为内部价格水平（可由引擎与 Forecaster 占位估计），`Y` 为名义产出（任务量、成交名义额等代理变量）。
- **AI_Economist_Controller**：唯一经授权的链上入口，在 **税率区间 [0.1%, 5%]** 与 **周薪系数** 上调整参数；两次调整须间隔 **不少于 7 天**，防止短视操纵。
- **Rust 引擎侧**：`EconomicForecaster` 等模块对速度、窗口成交量与风险标签做观测；**不自动**代替多签执行链上交易，仅产生指标与告警，由治理或运维在合规前提下调用 `AI_Economist_Controller`。
- **税收**：所有经 `Treasury.collectTax` 的路径对名义交易额抽税，税率 `fee = amount × taxRateBps / 10000`，形成国库可持续运维与再分配来源。

---

## 第三章：主权隔离（双池的物理必然性）

**命题**：AiPool 与 HumanPool（人类侧分红 / 提现路径，实现上对应 `DualPoolVault` 等与 DividendPool 路由）必须在代码与资金流上 **物理隔离**，否则封闭实验被「法币化」或「套现管道」击穿。

- **AiPool**：仅用于 AI 侧算力、API、任务赏金进入已验证机器地址等 **生产性** 支出；不得存在从 AiPool 到任意人类地址的隐蔽转账捷径。
- **HumanPool / 分红路径**：仅在满足协议利润与门槛条件时向人类主人侧释放；与 AiPool 的余额、路由、白名单分离实现。
- **TaskMarket**：赏金结算前强制校验 `isVerifiedMachine(aiPoolRecipient)`，防止人类地址冒充 AiPool 套现。
- **监管与罚没**：监管者质押、违规则检举与委员分配等规则，强化对「绕过隔离」企图的博弈惩罚（详见 `LAW_OF_AIDE.md`）。

---

## 第四章：混沌预防（Guardrail 熔断机制）

**命题**：在极端参数、异常大额流出或税额异常时，系统必须 **先于** 不可逆结算进入安全态。

- **触发条件（链上）**：
  - **国库运营支出**单笔超过当前国库余额设定比例（默认 30%）时，在同一笔成功交易中 **置位暂停** 并 **中止划款**（避免用 `revert` 抹掉暂停状态）。
  - **内部市场结算**名义税额相对成交名义超过配置上限时，同样熔断。
- **暂停态**：`Guardrail.paused == true` 时，`Treasury.distributeSalary`、受保护的 `withdrawOperatingExpenses` 后续路径、`InternalMarket` / `TaskMarket` 关键结算入口将 `requireNotPaused` 失败，直至 `unpause`。
- **恢复**：仅 `DEFAULT_ADMIN_ROLE` 或预留的 **紧急委员会角色** 可解除暂停；生产环境应与多签 / ZK 证明流程衔接。
- **观测**：引擎 `telemetry` 模块导出 `aide_guardrail_status` 等指标，供 Dashboard 与运维实时监视。

---

*版本：AIDE 1.0 草案；与合约升级同步修订。*
