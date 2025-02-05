-- Observer tables
CREATE TABLE IF NOT EXISTS observer_block_sync_eth_block_num (
    singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
    block_sync_eth_block_num BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS observer_deposit_sync_eth_block_num (
   singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
   deposit_sync_eth_block_num BIGINT NOT NULL
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

--- Observer indexes
CREATE INDEX idx_deposit_leaf_events_deposit_hash ON deposit_leaf_events(deposit_hash);
CREATE INDEX idx_deposit_leaf_events_block_tx ON deposit_leaf_events(eth_block_number, eth_tx_index);
CREATE INDEX idx_full_blocks_block_tx ON full_blocks(eth_block_number, eth_tx_index);

--- Merkle tree indexes
CREATE INDEX idx_hash_nodes_timestamp ON hash_nodes (timestamp_value DESC, tag);
CREATE INDEX idx_hash_nodes_lookup ON hash_nodes (bit_path, tag, timestamp_value DESC);
CREATE INDEX idx_leaves_lookup ON leaves (position, tag, timestamp_value DESC);
CREATE INDEX idx_leaves_timestamp ON leaves (timestamp_value DESC, tag);
CREATE INDEX idx_leaves_len_lookup ON leaves_len (tag, timestamp_value DESC);
