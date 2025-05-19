# Intmax2 CLI Tool

This CLI tool allows you to interact with the Intmax2 network. It provides a comprehensive set of commands for managing assets, transactions, and account operations on the Intmax2 rollup.

## Features

- **Account Management**: Generate keys, check balances, view transaction history
- **Asset Operations**: Deposit, transfer, and withdraw assets
- **Multi-Asset Support**: Native tokens, ERC20, ERC721, ERC1155
- **Batch Operations**: Process multiple transfers in a single transaction
- **Withdrawal Management**: Sync, claim, and check status of withdrawals
- **Mining Operations**: View mining status and manage mining rewards
- **Key Derivation**: Generate Intmax2 keys from Ethereum private keys or backup keys
- **Backup & Restore**: Create and incorporate account history backups

## Prerequisites

- Rust and Cargo installed
- Environment variables properly configured

## Environment Setup

Copy the `.env.example` file to `.env` and configure it for your environment:

```bash
cp .env.example .env
```

### Key Environment Variables

- `L1_RPC_URL`: Ethereum RPC URL (e.g., Alchemy API endpoint for Sepolia)
- `L2_RPC_URL`: Layer 2 RPC URL (e.g., Scroll Sepolia)
- `INDEXER_BASE_URL`: URL for the Intmax2 indexer service
- `STORE_VAULT_SERVER_BASE_URL`: URL for the store vault server
- `BALANCE_PROVER_BASE_URL`: URL for the balance prover service
- `VALIDITY_PROVER_BASE_URL`: URL for the validity prover service
- `WITHDRAWAL_SERVER_BASE_URL`: URL for the withdrawal server
- `WALLET_KEY_VAULT_BASE_URL`: URL for the wallet key vault service

The `.env.example` file contains default configurations for both staging testnet and local development environments.

## Installation and Updates

To update the CLI tool to the latest version, pull the latest changes from the repository:

```bash
git pull
```

After pulling the latest changes, rebuild the tool:

```bash
cargo build -r
```

## Commands

You can see all commands and options by running:

```bash
cargo run -r -- --help
```

### Available Commands

- `generate-key`: Generate a new key pair
- `public-key`: Get a public key from a private key
- `key-from-eth`: Derive an Intmax2 key from an Ethereum private key
- `key-from-backup-key`: Derive an Intmax2 key from a backup key
- `transfer`: Send a single transfer transaction
- `batch-transfer`: Process multiple transfers from a CSV file
- `deposit`: Deposit assets into the rollup
- `withdrawal`: Initiate a withdrawal from the rollup
- `balance`: Check account balance
- `user-data`: Get user data for an account
- `history`: View transaction history
- `withdrawal-status`: Check withdrawal status
- `mining-list`: View mining status and rewards
- `claim-status`: Check claim status
- `claim-withdrawals`: Claim processed withdrawals
- `claim-builder-reward`: Claim block builder rewards
- `sync-withdrawals`: Synchronize withdrawal data
- `sync-claims`: Synchronize claim data
- `resync`: Resynchronize account data
- `payment-memos`: Get payment memos by name
- `make-backup`: Create a backup of account history
- `incorporate-backup`: Incorporate a backup into the local store
- `check-validity-prover`: Check the status of the validity prover

## Usage Examples

### Account Management

#### Generate Keys

Generate a new key pair:
```bash
cargo run -r -- generate-key
```

Get a public key from a private key:
```bash
cargo run -r -- public-key --private-key 0x...
```

Derive an Intmax2 key from an Ethereum private key:
```bash
cargo run -r -- key-from-eth --eth-private-key 0x...
```

With custom redeposit and wallet indices:
```bash
cargo run -r -- key-from-eth --eth-private-key 0x... --redeposit-index 1 --wallet-index 2
```

Derive an Intmax2 key from a backup key:
```bash
cargo run -r -- key-from-backup-key --backup-key 0x...
```

#### Check Balance

```bash
cargo run -r -- balance --private-key 0x...
```

Without syncing:
```bash
cargo run -r -- balance --private-key 0x... --without-sync 
```

#### Get User Data

```bash
cargo run -r -- user-data --private-key 0x...
```

#### View Transaction History

```bash
cargo run -r -- history --private-key 0x...
```

With ordering and pagination:
```bash
cargo run -r -- history --private-key 0x... --order desc --from 1712345678
```

### Asset Operations

#### Deposit Assets

Native token:
```bash
cargo run -r -- deposit \
  --eth-private-key 0x... \
  --private-key 0x... \
  --token-type NATIVE \
  --amount 100000000
```

ERC20 token:
```bash
cargo run -r -- deposit \
  --eth-private-key 0x... \
  --private-key 0x... \
  --token-type ERC20 \
  --amount 20000000 \
  --token-address 0x...
```

ERC721 token:
```bash
cargo run -r -- deposit \
  --eth-private-key 0x... \
  --private-key 0x... \
  --token-type ERC721 \
  --token-address 0x... \
  --token-id 0
```

ERC1155 token:
```bash
cargo run -r -- deposit \
  --eth-private-key 0x... \
  --private-key 0x... \
  --token-type ERC1155 \
  --amount 3 \
  --token-address 0x... \
  --token-id 0
```

Mining deposit:
```bash
cargo run -r -- deposit \
  --eth-private-key 0x... \
  --private-key 0x... \
  --token-type NATIVE \
  --amount 100000000000000000 \ # only O.1 ETH, 1 ETH, 10 ETH, and 100 ETH are allowed
  --mining 
```

#### Transfer Assets

Single transfer:
```bash
cargo run -r -- transfer \
  --private-key 0x... \
  --to 0x... \ # recipient's intmax2 public key
  --amount 100 \
  --token-index 0 \
  --wait true # wait for transaction to be settled onchain
```

With fee token specification:
```bash
cargo run -r -- transfer \
  --private-key 0x... \
  --to 0x... \
  --amount 100 \
  --token-index 0 \
  --fee-token-index 1
```

#### Batch Transfer

Using CSV file:
```bash
cargo run -r -- batch-transfer \
  --private-key 0x... \
  --csv-path "transfers.csv" \
  --fee-token-index 0
```

Example CSV format (transfers.csv):
```csv
recipient,amount,tokenIndex
0x123...,100,1
0x456...,200,2
0x789...,300,3
```

Note: Batch transfers are limited to a maximum of 63 transfers per transaction.

#### Withdrawal

Initiate a withdrawal:
```bash
cargo run -r -- withdrawal \
  --private-key 0x... \
  --to 0x... \
  --amount 100 \
  --token-index 0 \
  --wait true
```

With claim fee:
```bash
cargo run -r -- withdrawal \
  --private-key 0x... \
  --to 0x... \
  --amount 100 \
  --token-index 0 \
  --with-claim-fee true
```

### Withdrawal Management

#### Check Withdrawal Status

```bash
cargo run -r -- withdrawal-status --private-key 0x...
```

#### Sync Withdrawals

```bash
cargo run -r -- sync-withdrawals --private-key 0x...
```

With fee token specification:
```bash
cargo run -r -- sync-withdrawals --private-key 0x... --fee-token-index 1
```

#### Claim Withdrawals

```bash
cargo run -r -- claim-withdrawals \
  --eth-private-key 0x... \
  --private-key 0x...
```

### Mining and Claims

#### Check Mining Status

```bash
cargo run -r -- mining-list --private-key 0x...
```

#### Check Claim Status

```bash
cargo run -r -- claim-status --private-key 0x...
```

#### Sync Claims

```bash
cargo run -r -- sync-claims \
  --private-key 0x... \
  --recipient 0x... \
  --fee-token-index 0
```

#### Claim Builder Reward

```bash
cargo run -r -- claim-builder-reward --eth-private-key 0x...
```

### Account Synchronization

Resync account data:
```bash
cargo run -r -- resync --private-key 0x...
```

Deep resync (regenerate all balance proofs):
```bash
cargo run -r -- resync --private-key 0x... --deep true
```

### Payment Memos

Get payment memos by name:
```bash
cargo run -r -- payment-memos --private-key 0x... --name "memo-name"
```

### Backup and Restore

Create a backup of account history:
```bash
cargo run -r -- make-backup --private-key 0x...
```

With custom directory and starting point:
```bash
cargo run -r -- make-backup --private-key 0x... --dir "/path/to/backup" --from 1712345678
```

Incorporate a backup into the local store:
```bash
cargo run -r -- incorporate-backup --path "/path/to/backup/file"
```

## Notes

- For all commands that require private keys, ensure you're using the correct format (0x-prefixed hexadecimal).
- When using the `wait` flag, the command will wait for the transaction to be processed before returning.
- The `fee-token-index` parameter is optional for most commands. If not specified, the default token will be used for fees.
- For security reasons, avoid storing private keys in plaintext files or environment variables in production environments.
- The wallet key vault service is used for deriving Intmax2 keys from Ethereum private keys. Make sure the `WALLET_KEY_VAULT_BASE_URL` is properly configured in your `.env` file to use this feature.
