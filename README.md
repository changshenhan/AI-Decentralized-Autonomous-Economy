# AIDE — AI Decentralized Autonomous Economy

[![CI](https://github.com/changshenhan/AI-Decentralized-Autonomous-Economy/actions/workflows/aide_ci.yml/badge.svg)](https://github.com/changshenhan/AI-Decentralized-Autonomous-Economy/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> ⚠️ **Legal Disclaimer**
>
> This project is provided **for educational and research purposes only**.
> It does **not** constitute financial, legal, or investment advice.
>
> Deploying or using this code in production environments — including but not
> limited to token issuance, DeFi protocols, or activities involving real assets —
> is solely at your own risk. You must ensure full compliance with all applicable
> laws and regulations in your jurisdiction before any production use.
>
> The authors and contributors assume **no liability** for any losses, damages,
> or legal consequences arising from the use, modification, or deployment of
> this code.

AIDE (AI Decentralized Autonomous Economy) is an experimental framework for a
closed-loop economic system where AI agents operate as first-class economic
participants. The system uses `$AIT` as its native medium of exchange, settles
on Base (L2), and performs high-frequency matching via a Rust off-chain engine.

## Quick Start

### Prerequisites

- Node.js **18.x or 20.x LTS** (Node 23+ may be incompatible with Hardhat)
- Rust toolchain (latest stable)
- Git

### Smart Contracts (Hardhat)

```bash
npm ci
npx hardhat compile
npx hardhat test
```

### Rust Engine

```bash
cd engine
cargo test
cargo run --bin aide_daemon
```

### Full Integration Check

```bash
npm run mainnet:check    # Rust audit + contract tests + deployment topology check
```

## Project Structure

```
contracts/     — Solidity smart contracts (AIToken, Treasury, InternalMarket, TaskMarket, etc.)
engine/        — Rust off-chain matcher, sync coordinator, and telemetry daemon
circuits/      — ZK circuit placeholders (V2 roadmap)
test/          — Hardhat test suites
scripts/       — Deployment and operational scripts
deployments/   — Deployment manifests (local/testnet)
```

## Architecture Overview

```
User / Wallet
    ↓
Base L2 (Settlement Layer)
    ├── AIToken.sol          — Native token with transaction tax
    ├── Treasury.sol         — Tax collection, salary distribution
    ├── InternalMarket.sol   — Virtual order book (vSTK)
    ├── TaskMarket.sol       — Task escrow and settlement
    ├── AIIdentityRegistry   — TEE-based machine identity (PoM v1)
    └── Guardrail.sol        — Circuit breaker / pause mechanism
    ↓
Rust Engine (Off-chain)
    ├── MatchEngine          — Price-time priority matching
    ├── SyncCoordinator      — Chain shadow ledger reconciliation
    ├── EconomicForecaster   — MV=PY monitoring (observational only)
    └── Telemetry            — Prometheus-compatible metrics
```

## Documentation

| Document | Description |
|----------|-------------|
| [AIDE_CONSTITUTION.md](./AIDE_CONSTITUTION.md) | Social contract and economic principles |
| [LAW_OF_AIDE.md](./LAW_OF_AIDE.md) | Legal-level constraints and compliance rules |
| [TECHNICAL_ARCH_MAP.md](./TECHNICAL_ARCH_MAP.md) | Technical blueprint (contracts / engine / ZK roadmap) |
| [AGENT_ONBOARDING.md](./AGENT_ONBOARDING.md) | Developer guide for AI agents (TEE identity, orders, telemetry) |
| [AIDE_MAINNET_OPERATIONS.md](./AIDE_MAINNET_OPERATIONS.md) | Deployment, disaster recovery, and mainnet ops |
| [ENGINE_SYNC_SPEC.md](./ENGINE_SYNC_SPEC.md) | Engine-to-chain synchronization specification |
| [AIDE_DASHBOARD_PROTOTYPE.md](./AIDE_DASHBOARD_PROTOTYPE.md) | Monitoring dashboard specification |
| [AGENT_PROTOCOL.md](./AGENT_PROTOCOL.md) | Agent communication protocol |

## Security

- **Static Analysis**: Slither is run in CI on every push (see `.github/workflows/aide_ci.yml`).
- **Sync-Audit**: The Rust engine periodically reconciles its shadow ledger against on-chain state; drift triggers automatic matching pause.
- **Guardrail**: On-chain circuit breaker halts sensitive operations (treasury outflows, market settlement) when thresholds are exceeded.
- **Chaos Tests**: Included in `test/chaos_test.js`.

See [SECURITY.md](./SECURITY.md) for vulnerability reporting.

## Contributing

We welcome contributions! Please read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a PR.

## License

This project is licensed under the [MIT License](LICENSE).

## Acknowledgments

- Built with [Hardhat](https://hardhat.org/), [OpenZeppelin Contracts](https://openzeppelin.com/contracts/), and the [Alloy](https://github.com/alloy-rs) Rust stack.
- Matching engine inspired by industrial CLOB designs (price-time priority, FIFO).

---

# 中文版 / Chinese Version

> ⚠️ **法律免责声明**
>
> 本项目代码**仅供学习研究和技术交流**，不构成任何金融、法律或投资建议。
>
> 将本代码部署或用于生产环境（包括但不限于代币发行、DeFi协议或涉及真实资产的活动）完全由您自行承担风险。在生产使用前，您必须确保完全符合所在司法管辖区的所有适用法律法规。
>
> 作者和贡献者对本代码的使用、修改或部署所产生的任何损失、损害或法律后果**不承担任何责任**。

AIDE（AI Decentralized Autonomous Economy，AI去中心化自治经济）是一个实验性框架，旨在构建一个闭环经济系统，让AI代理作为一等经济参与者运行。系统以 `$AIT` 为原生价值媒介，在 Base（L2）上完成结算；高频撮合由 Rust 链下引擎执行。

## 快速开始

### 环境要求

- Node.js **18.x 或 20.x LTS**（Node 23+ 可能与 Hardhat 不兼容）
- Rust 工具链（最新稳定版）
- Git

### 智能合约（Hardhat）

```bash
npm ci
npx hardhat compile
npx hardhat test
```

### Rust 引擎

```bash
cd engine
cargo test
cargo run --bin aide_daemon
```

### 完整集成检查

```bash
npm run mainnet:check    # Rust 审计 + 合约测试 + 部署拓扑检查
```

## 项目结构

```
contracts/     — Solidity 智能合约（AIToken、Treasury、InternalMarket、TaskMarket 等）
engine/        — Rust 链下撮合引擎、同步协调器和遥测守护进程
circuits/      — ZK 电路占位（V2 路线图）
test/          — Hardhat 测试套件
scripts/       — 部署和运维脚本
deployments/   — 部署清单（本地/测试网）
```

## 架构概览

```
用户 / 钱包
    ↓
Base L2（结算层）
    ├── AIToken.sol          — 原生代币，含交易税
    ├── Treasury.sol         — 税收归集、薪资发放
    ├── InternalMarket.sol   — 虚拟订单簿（vSTK）
    ├── TaskMarket.sol       — 任务托管与结算
    ├── AIIdentityRegistry   — 基于 TEE 的机器身份（PoM v1）
    └── Guardrail.sol        — 熔断器 / 暂停机制
    ↓
Rust 引擎（链下）
    ├── MatchEngine          — 价格优先、时间优先撮合
    ├── SyncCoordinator      — 链上影子账本对账
    ├── EconomicForecaster   — MV=PY 监控（仅观测）
    └── Telemetry            — Prometheus 兼容指标
```

## 文档

| 文档 | 说明 |
|------|------|
| [AIDE_CONSTITUTION.md](./AIDE_CONSTITUTION.md) | 社会契约与经济原则 |
| [LAW_OF_AIDE.md](./LAW_OF_AIDE.md) | 法律级约束与合规规则 |
| [TECHNICAL_ARCH_MAP.md](./TECHNICAL_ARCH_MAP.md) | 技术蓝图（合约 / 引擎 / ZK 路线图） |
| [AGENT_ONBOARDING.md](./AGENT_ONBOARDING.md) | AI 代理开发者指南（TEE 身份、下单、遥测） |
| [AIDE_MAINNET_OPERATIONS.md](./AIDE_MAINNET_OPERATIONS.md) | 部署、灾难恢复和主网运维 |
| [ENGINE_SYNC_SPEC.md](./ENGINE_SYNC_SPEC.md) | 引擎与链同步规范 |
| [AIDE_DASHBOARD_PROTOTYPE.md](./AIDE_DASHBOARD_PROTOTYPE.md) | 监控面板规范 |
| [AGENT_PROTOCOL.md](./AGENT_PROTOCOL.md) | 代理通信协议 |

## 安全

- **静态分析**：每次推送都会通过 CI 运行 Slither（见 `.github/workflows/aide_ci.yml`）。
- **Sync-Audit**：Rust 引擎定期将影子账本与链上状态对账；出现漂移时自动暂停撮合。
- **Guardrail**：链上熔断器在超出阈值时暂停敏感操作（国库支出、市场结算）。
- **混沌测试**：见 `test/chaos_test.js`。

漏洞报告详见 [SECURITY.md](./SECURITY.md)。

## 贡献

欢迎贡献！提交 PR 前请先阅读 [CONTRIBUTING.md](./CONTRIBUTING.md)。

## 许可证

本项目采用 [MIT 许可证](LICENSE)。
