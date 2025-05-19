# Validity Prover

## Preparation

Create `.env` file. You need to specify Alchemy API key in `L2_RPC_URL`. 
```
cp .env.example .env
```

Install sqlx-cli. 

```bash
cargo install sqlx-cli
```

Launch database (if you haven't already).
```
docker run --name postgres -e POSTGRES_PASSWORD=password -p 5432:5432 -d postgres
```

## Starting the Node

```
sqlx database setup && cargo run -r
```

## Database Schema

The following is the consolidated migration that represents the final database schema:

```sql
-- Observer tables
CREATE TABLE IF NOT EXISTS observer_block_sync_eth_block_num (
    singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
    block_sync_eth_block_num BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS observer_deposit_sync_eth_block_num (
   singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
   deposit_sync_eth_block_num BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS observer_l1_deposit_sync_eth_block_num (
   singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
   l1_deposit_sync_eth_block_num BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS full_blocks (
    block_number INTEGER PRIMARY KEY,
    eth_block_number BIGINT NOT NULL,
    eth_tx_index BIGINT NOT NULL,
    full_block BYTEA NOT NULL
);

CREATE TABLE IF NOT EXISTS deposit_leaf_events (
    deposit_index INTEGER PRIMARY KEY,
    deposit_hash BYTEA NOT NULL,
    eth_block_number BIGINT NOT NULL,
    eth_tx_index BIGINT NOT NULL
);

-- Validity prover tables
CREATE TABLE IF NOT EXISTS validity_state (
   block_number INTEGER NOT NULL PRIMARY KEY,
   validity_witness BYTEA NOT NULL
);

CREATE TABLE IF NOT EXISTS tx_tree_roots (
    tx_tree_root BYTEA PRIMARY KEY,
    block_number INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS validity_proofs (
    block_number INTEGER PRIMARY KEY,
    proof BYTEA NOT NULL
);

-- Prover coordinator tables
CREATE TABLE IF NOT EXISTS prover_tasks (
    block_number INTEGER PRIMARY KEY,
    assigned BOOLEAN NOT NULL,
    assigned_at TIMESTAMP,
    last_heartbeat TIMESTAMP,
    completed BOOLEAN NOT NULL,
    completed_at TIMESTAMP,
    transition_proof BYTEA 
);

--- Merkle tree tables
CREATE TABLE IF NOT EXISTS hash_nodes (
    timestamp_value bigint NOT NULL,
    tag int NOT NULL,
    bit_path bytea NOT NULL,
    hash_value bytea NOT NULL,
    PRIMARY KEY (timestamp_value, tag, bit_path)
);

CREATE TABLE IF NOT EXISTS leaves (
    timestamp_value bigint NOT NULL,
    tag int NOT NULL,
    position bigint NOT NULL,
    leaf_hash bytea NOT NULL,
    leaf bytea NOT NULL,
    PRIMARY KEY (timestamp_value, tag, position)
);

CREATE TABLE IF NOT EXISTS leaves_len (
    timestamp_value bigint NOT NULL,
    tag int NOT NULL,
    len int NOT NULL,
    PRIMARY KEY (timestamp_value, tag)
);

CREATE TABLE IF NOT EXISTS indexed_leaves (
    timestamp_value bigint NOT NULL,
    tag int NOT NULL,
    position bigint NOT NULL,
    leaf_hash bytea NOT NULL,
    next_index bigint NOT NULL,
    key NUMERIC(78, 0) NOT NULL,
    next_key NUMERIC(78, 0) NOT NULL,
    value bigint NOT NULL,
    PRIMARY KEY (timestamp_value, tag, position)
);

-- L1 Deposit and Settings tables
CREATE TABLE IF NOT EXISTS settings (
    singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
    rollup_contract_address VARCHAR(42) NOT NULL,
    liquidity_contract_address VARCHAR(42) NOT NULL
);

CREATE TABLE IF NOT EXISTS deposited_events (
    deposit_id BIGINT PRIMARY KEY,
    depositor VARCHAR(42) NOT NULL,
    pubkey_salt_hash VARCHAR(66) NOT NULL,
    token_index BIGINT NOT NULL,
    amount VARCHAR(66) NOT NULL,
    is_eligible BOOLEAN NOT NULL,
    deposited_at BIGINT NOT NULL,
    deposit_hash VARCHAR(66) NOT NULL,
    tx_hash VARCHAR(66) NOT NULL,
    eth_block_number BIGINT NOT NULL,
    eth_tx_index BIGINT NOT NULL
);

--- Indexes
CREATE INDEX idx_deposit_leaf_events_deposit_hash ON deposit_leaf_events(deposit_hash);
CREATE INDEX idx_deposit_leaf_events_block_tx ON deposit_leaf_events(eth_block_number, eth_tx_index);
CREATE INDEX idx_deposited_events_pubkey_salt_hash ON deposited_events(pubkey_salt_hash);
CREATE INDEX idx_full_blocks_block_tx ON full_blocks(eth_block_number, eth_tx_index);
CREATE INDEX idx_hash_nodes_lookup ON hash_nodes (bit_path, tag, timestamp_value DESC);
CREATE INDEX idx_leaves_len_lookup ON leaves_len (tag, timestamp_value DESC);
CREATE INDEX idx_indexed_leaves_lookup ON indexed_leaves (position, tag, timestamp_value DESC);
CREATE INDEX idx_indexed_leaves_timestamp ON indexed_leaves (timestamp_value DESC, tag);
CREATE INDEX idx_prover_tasks_assigned_status ON prover_tasks (assigned, completed);
CREATE INDEX idx_tx_tree_roots_block_number ON tx_tree_roots (block_number);
```

This consolidated migration represents the final state after applying all migrations. It can be used to set up the database schema from scratch without having to apply each migration individually.
