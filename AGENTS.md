# Repository Guidelines

## Project Structure & Module Organization
INTMAX2 is a Rust workspace (`Cargo.toml`) composed of service crates such as `block-builder`, `balance-prover`, `validity-prover`, and `withdrawal-server`, each with its own `src/` for application code. Shared logic lives in `common`, `server-common`, and `interfaces`, while developer tooling sits in `cli`, `client-sdk`, and `scripts/`. Integration and end-to-end tests reside under `tests/`, with contract deployment flows in `tests/tests` and supporting fixtures in `tests/src`. Docker resources live in `docker/` and `compose.yml`, and SSL fixtures for Redis tests are under `redis-test-certs/`.

## Build, Test, and Development Commands
- `cargo build --workspace` — compile all crates; add `--release` when benchmarking provers.  
- `cargo fmt --all` and `cargo clippy --workspace --all-targets --all-features` — enforce formatting and linting before commits.  
- `cargo test --workspace --all-features` — run unit tests plus integration suites such as `deploy_contracts.rs`.  
- `lefthook run pre-commit` — execute the same format, lint, and typo checks that CI expects.  
- `docker compose up -d` — start PostgreSQL, Redis, and ancillary services required by the vault and prover crates.  
- `(cd validity-prover && sqlx database setup)` (repeat for other DB-backed services) — provision local schemas before running servers.

## Coding Style & Naming Conventions
Rust files follow `rustfmt` defaults with crate-level import grouping (`rustfmt.toml` sets `imports_granularity = "Crate"`). Use four-space indentation, `snake_case` for modules and functions, and `UpperCamelCase` for types and enums. Prefer `?` over `unwrap()` in async services, and surface shared errors via the common crate. Keep configuration files (`.env`) out of version control; copy from `.env.example` when needed.

## Testing Guidelines
Unit tests live alongside source in each crate; integration scenarios use the `tests/tests` suite. Name new tests after the behavior under scrutiny (e.g. `handles_empty_batch`). When a change touches database or prover flows, add an end-to-end case in `tests/tests/*` and ensure the relevant SQL migrations have an accompanying `sqlx prepare` update. Run `cargo test --workspace --all-features` before submitting, and include logs for any ignored tests you enable.

## Commit & Pull Request Guidelines
Adopt Conventional Commits as seen in history (`feat:`, `fix:`, `chore:`) and keep summaries under 72 characters. Branch from `dev`, reference issues in the body (`Closes #123`), and include context such as CLI commands or screenshots for UI-facing SDK updates. Before opening a PR, confirm format, lint, and test commands succeed locally and note any follow-up work in the description. Tag reviewers for services you touched (e.g. prover team for `validity-prover` changes) and wait for dual approvals before merging.
