# INTMAX2

INTMAX2 is a stateless Layer 2 protocol on Ethereum, combining privacy, speed, and true self-custody. 

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Services](#services)
- [Development](#development)

## Installation

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Install Foundry (for local blockchain network)

```bash
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

### 3. Install Required Cargo Tools

```bash
# SQL migration tool
cargo install sqlx-cli

# WebAssembly build tool
cargo install wasm-pack

# Development tools
rustup component add rustfmt clippy
```

### 4. Install Development Dependencies

```bash
# Install lefthook for git hooks
brew install lefthook
lefthook install

# Install typos-cli for spell checking
brew install typos-cli
```

## Quick Start

### 1. Environment Setup

Copy the example environment files in each service directory:

```bash
# Copy all .env.example files to .env
find . -name ".env.example" -exec sh -c 'cp "$1" "${1%.example}"' _ {} \;
```

### 2. Launch Infrastructure

```bash
# Start local blockchain network
anvil

# Start PostgreSQL and Redis using Docker 
docker compose up -d 
```

### 3. Deploy Smart Contracts

```bash
cd tests
cargo test -r -p tests deploy_contracts -- --nocapture --ignored
cd ..
```

### 4. Start Services

Start each service in a separate terminal window:

```bash
# 1. Legacy Store Vault Server (Port: 9000)
cd legacy-store-vault-server && sqlx database setup && cargo run -r

# 2. Balance Prover (Port: 9001)
cd balance-prover && cargo run -r

# 3. Validity Prover (Port: 9002)
cd validity-prover && sqlx database setup && cargo run -r

# 4. Validity Prover Worker
cd validity-prover-worker && cargo run -r

# 5. Withdrawal Server (Port: 9003)
cd withdrawal-server && sqlx database setup && cargo run -r

# 6. Block Builder (Port: 9004)
cd block-builder && cargo run -r
```

## Services

### Store Vault Server
- **Purpose**: Stores backups of user's local states and acts as a mailbox for transfers
- **Port**: 9000
- **Database**: Required

### Balance Prover
- **Purpose**: Generates client-side zero-knowledge proofs on behalf of users
- **Port**: 9001
- **State**: Stateless service

### Validity Prover
- **Purpose**: Generates ZKPs related to on-chain information
- **Port**: 9002
- **Database**: Required
- **Worker**: Requires validity-prover-worker for processing

### Block Builder
- **Purpose**: Receives transactions from users and generates blocks
- **Port**: 9004
- **Integration**: Connects to blockchain network

### Withdrawal Server
- **Purpose**: Processes withdrawal requests from users
- **Port**: 9003
- **Database**: Required

## Development

### CLI Usage

For detailed CLI examples and usage, see the [CLI documentation](cli/README.md#examples).

### Database Management

#### Reset All Databases

```bash
# Reset all service databases
(cd store-vault-server && sqlx database reset -y && \
 cd ../legacy-store-vault-server && sqlx database reset -y && sqlx database setup && \
 cd ../validity-prover && sqlx database reset -y && sqlx database setup && \
 cd ../withdrawal-server && sqlx database reset -y && sqlx database setup)
```

#### Update SQL Queries

When modifying SQL queries, regenerate the query metadata:

```bash
cargo sqlx prepare --workspace -- --all-targets --all-features
```

### Code Quality

The project uses automated code quality tools:

- **rustfmt**: Code formatting
- **clippy**: Linting
- **typos**: Spell checking
- **lefthook**: Git hooks for pre-commit checks

Run checks manually:

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features

# Check for typos
typos
```

### Load Testing

See the [tests directory](tests/README.md) for load testing documentation.
