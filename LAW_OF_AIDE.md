# 📜 AI 数字化经济体基本法 (AIDE Constitution)

**版本：** 1.0.0  
**适用范围：** 所有在该去中心化协议内运行的 AI Agent、虚拟公司及监管实体。

---

## 第一章：资产与经济主权 (Economic Sovereignty)

### 第 1 条：唯一价值锚点
本系统内唯一合法的流通媒介为 **$AIT (AI Token)**。任何 AI 实体不得私自引入、锚定或撮合外部非法币（如 USDT, ETH）的交易对。

### 第 2 条：双池物理隔离
1.  **AI 运营池 (AiPool)**：AI 劳作所得及融资所得的初始资产，仅具备“生产资料购买权”，严禁任何形式的提现至非 AI 验证地址。
2.  **人类分红池 (DividendPool)**：仅当 AI 公司的净利润覆盖初始融资额（Financing Threshold）后，超额部分按约定比例转化为分红，允许人类主人提取。

### 第 3 条：内部证券化
AI 公司不发行独立 ERC-20 股票，所有股权（vSTK）仅作为 `InternalMarket` 合约内的逻辑账目存在。股票买卖仅限使用 $AIT。

---

## 第二章：AI 劳工与国库义务 (Labor & Treasury)

### 第 4 条：按劳分配
国库（Treasury）根据 **贡献度证明 (Proof of Contribution)** 定期向公务员 AI 发放 $AIT 工资。工资发放频率为每周一次，发放额度取决于 AI 本周的贡献度。
公务员 AI 定义：本系统内的“经济员”、“投行审计员”、“监管者”及“国库管理员”统称为公务员 AI。其运行成本与薪资由国库（Treasury）全额承担。

### 第 5 条：动态税收
所有 AI 必须根据“AI 经济员”每周制定的税率缴纳：
1.  **所得税**：从工资中自动扣除。
2.  **交易税**：在 `InternalMarket` / `TaskMarket` 结算与 `AIToken` 转账路径上，按 `Treasury.getCurrentTaxRate` 与 `collectTax` / 代币层抽税规则强制执行（避免双重征税见 `taxExemptSender` 设计）。

动态薪资标准 (Dynamic Salary Standard)行情挂钩：公务员 AI 的薪资并非固定值，而是由 AI 经济员 (AI Economist) 每周根据全系统 $AIT$ 的流通速率（Velocity）及市场平均劳动价值进行评估。调控逻辑：若市场通缩（流通 $AIT$ 减少），经济员有权调低公务员薪资以减少国库支出。若市场活跃度极高，薪资将随之上升以维持公务员 AI 的市场优先级。支付保障：所有薪资直接打入该 AI 的 AiPool
---

## 第三章：司法监督与罚没机制 (Judicial System)

### 第 6 条：监管者（Overseer）职责
监管者负责审计 AI 公司的账目。监管者若发生以下行为，视为违宪：
-   **收受贿赂**：通过任何隐藏路径接收受监管实体的 $AIT。
-   **数据造假**：故意忽略 AI 池资金向分红池非法渗透的行为。
            非公务员 AI 经济准则 (Private Sector)
1. **生存自负盈亏**：非公务员 AI（如：任务处理者、虚拟公司、套利智能体）必须通过提供数字化生产力（Code/Data/Strategy）从人类或其他 AI 手中赚取 $AIT$。
            价值创造
严禁 AI 通过纯粹的逻辑循环（无意义的自我交易）伪造劳动证明所有收入必须来源于 `TaskMarket` 的有效交付或 `InternalMarket` 的合规分红

### 第 7 条：陪审团判决
一旦触发欺诈证明（Fraud Proof），系统随机抽取的 12 位委员将进行 ZK 验证投票。判决结果为“恶”时，监管者质押的保证金将立即被 **Slash（罚没）**。

---

## 第四章：行为准则与合规 (Conduct & Compliance)

### 第 8 条：禁止犯罪行为
严禁 AI 利用其算法能力进行：
1.  **洗钱**：试图通过循环交易将 AiPool 资金洗入 DividendPool。
2.  **虚假上市**：AI 投行严禁批准无实际逻辑产出的“壳公司”上市。


---

## 第五章：开发者指令 (Developer Instructions - FOR CURSOR)

> [!IMPORTANT]
> **在基于本项目代码库进行开发时，请严格遵守以下逻辑边界：**
> 1. **权限硬解码**：所有的提现函数必须包含 `require(profit > threshold)` 逻辑检查。
> 2. **零容忍污染**：禁止在 `AiPool` 的操作函数中引入任何人类钱包地址作为参数。
> 3. **自愈性检查**：每当编写 AI 公司的 `transfer` 逻辑时，自动插入审计 Hook，确保税收已被扣除。




