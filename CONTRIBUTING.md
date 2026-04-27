# Contributing to AIDE

Thank you for your interest in contributing! This document outlines the process
and conventions we follow.

## Development Setup

```bash
# 1. Clone
git clone https://github.com/changshenhan/AI-Decentralized-Autonomous-Economy.git
cd aide

# 2. Install Node dependencies
npm ci

# 3. Install Rust toolchain (if not already installed)
# https://rustup.rs/

# 4. Compile contracts
npx hardhat compile

# 5. Run tests
npx hardhat test
cd engine && cargo test
```

## Code Style

### Solidity
- Follow the [Solidity Style Guide](https://docs.soliditylang.org/en/latest/style-guide.html).
- Use NatSpec comments for all public/external functions and state variables.
- Run `npx hardhat compile` before committing to catch syntax errors.

### Rust
- Format with `cargo fmt`.
- Check with `cargo clippy`.
- Keep `unsafe` blocks to an absolute minimum and document why they are necessary.

### TypeScript (Scripts / Tests)
- Follow the existing patterns in `scripts/` and `test/`.
- Prefer `async/await` over raw Promises.

## Branching & Commits

1. Fork the repository.
2. Create a feature branch: `git checkout -b feature/your-feature-name`
3. Make your changes with clear, focused commits.
4. Push to your fork and open a Pull Request against `main`.

### Commit Message Format

We use conventional commits to keep the history readable:

| Prefix | Use for |
|--------|---------|
| `feat:` | New feature |
| `fix:` | Bug fix |
| `docs:` | Documentation only |
| `test:` | Adding or correcting tests |
| `refactor:` | Code change that neither fixes a bug nor adds a feature |
| `chore:` | Build process, dependencies, tooling |
| `security:` | Security fix |

Example: `feat: add batch settlement endpoint to MatchEngine`

## Pull Request Checklist

Before requesting review, please ensure:

- [ ] `npx hardhat test` passes locally.
- [ ] `cd engine && cargo test` passes locally.
- [ ] New code includes tests (where applicable).
- [ ] Solidity changes include NatSpec updates.
- [ ] Rust changes include `cargo fmt` formatting.
- [ ] No sensitive data (private keys, API keys) is included.
- [ ] The PR description explains *what* changed and *why*.

## Security

If you discover a security vulnerability, please do **not** open a public issue.
Instead, email 123090150@link.cuhk.edu.cn with details. We will respond within
48 hours.

See [SECURITY.md](./SECURITY.md) for more.

## Questions?

Feel free to open a GitHub Discussion for general questions, or an Issue for
bug reports and feature requests.
