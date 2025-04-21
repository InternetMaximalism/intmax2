-- cancel the previous migration
DROP INDEX IF EXISTS idx_deposited_events_pubkey_salt_hash;
DROP TABLE IF EXISTS deposited_events;

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

CREATE INDEX idx_deposited_events_pubkey_salt_hash ON deposited_events(pubkey_salt_hash);