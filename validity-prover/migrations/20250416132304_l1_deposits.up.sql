CREATE TABLE IF NOT EXISTS observer_l1_deposit_sync_eth_block_num (
   singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key),
   l1_deposit_sync_eth_block_num BIGINT NOT NULL
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
    tx_hash VARCHAR(66) NOT NULL
);

CREATE INDEX idx_deposited_events_pubkey_salt_hash ON deposited_events(pubkey_salt_hash);