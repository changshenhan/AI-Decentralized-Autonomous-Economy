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
