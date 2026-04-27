# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-04-27

### Added
- Core smart contract suite (`AIToken`, `Treasury`, `InternalMarket`, `TaskMarket`, `Guardrail`, `AIIdentityRegistry`, `StakingPool`, `AIBank`, `DualPoolVault`, `AI_Economist_Controller`, `AideCreate2Factory`).
- Rust matching engine (`MatchEngine`) with price-time priority CLOB logic.
- Chain sync coordinator (`SyncCoordinator`) + audit module for ledger reconciliation.
- Telemetry server (`aide_daemon`) exposing Prometheus-compatible `/metrics`.
- TEE-based machine identity verification (`AIIdentityRegistry`).
- Economic forecaster placeholder (`EconomicForecaster`) for MV=PY monitoring.
- Hardhat test suite covering chaos scenarios, guardrail, economist cooldowns.
- GitHub Actions CI: Rust checks, Hardhat compilation + tests, Slither static analysis.
- Deployment scripts with CREATE2 deterministic addressing.
- Operational scripts: genesis launch, mainnet check, preflight inventory.

### Security
- Guardrail circuit breaker for treasury outflows and settlement fee anomalies.
- Sync-Audit drift detection pauses matching when chain/shadow state diverges.
- Settlement failure threshold auto-pauses engine on consecutive `settleTrade` failures.
- Slither static analysis integrated into CI pipeline.

### Known Limitations
- `circuits/` crate is a placeholder; production-grade ZK circuits are on the V2 roadmap.
- Engine persistence layer is in-memory; production deployments require external storage (RocksDB/Redis).
- ZK-PoM (Proof of Machine via ZK) is not yet implemented; v1 relies on TEE ECDSA.

## [Unreleased]

### Planned
- L3 rollup architecture for scalable settlement.
- BLS aggregated signatures for batch settlement.
- Distributed engine sharding for multi-company order books.
- ZK-STARK based identity verification (V2 PoM).
- Full dashboard frontend implementation.
