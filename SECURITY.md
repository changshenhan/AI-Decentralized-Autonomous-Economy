# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | ✅ Yes    |
| < 1.0   | ❌ No     |

## Reporting a Vulnerability

If you discover a security vulnerability in AIDE, please **do not** open a public
GitHub issue. Instead, follow the responsible disclosure process below.

### Contact

Email: **123090150@link.cuhk.edu.cn**

Please include:
- A clear description of the vulnerability.
- Steps to reproduce (proof-of-concept if possible).
- Affected contracts / engine modules.
- Severity assessment (Critical / High / Medium / Low).

### Response Timeline

| Stage | Timeline |
|-------|----------|
| Acknowledgment | Within 48 hours |
| Initial assessment | Within 7 days |
| Fix & verification | Within 30 days (critical), 90 days (non-critical) |
| Public disclosure | Coordinated with reporter |

## Security Measures

### Automated
- **Slither static analysis** runs on every push via GitHub Actions.
- `cargo test` and `npx hardhat test` must pass before merge.

### Manual Review
- All contract changes require at least one review from a maintainer.
- High-risk paths (treasury transfers, settlement, identity attestation) require
  additional scrutiny.

### Known Security Considerations

| Component | Risk | Mitigation |
|-----------|------|------------|
| `Treasury.withdrawOperatingExpenses` | Large outflow | Guardrail `evaluateOutflow` pauses if >30% balance |
| `InternalMarket.settleTrade` | Reentrancy / double-spend | `nonReentrant` + balance checks + Guardrail |
| `AIToken._update` | Tax calculation overflow | `rate <= BPS_DENOMINATOR` + `_processingTax` flag |
| `AIIdentityRegistry` | Fake TEE attestation | `recover(digest, sig) == teeAuthority` |
| Engine shadow ledger | Drift from chain state | Sync-Audit pauses matching on discrepancy |

## Audit History

| Date | Auditor | Scope | Result |
|------|---------|-------|--------|
| 2026-04 | Slither (CI) | All `.sol` contracts | No critical issues |
| — | — | — | — |

*No external professional audit has been performed yet. Use at your own risk.*

## Legal Disclaimer

This software is provided "as is" without warranty. See [LICENSE](LICENSE) for
the full disclaimer. No security measure can guarantee absolute safety in
blockchain environments.
