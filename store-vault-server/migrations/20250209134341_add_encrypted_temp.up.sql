CREATE TABLE IF NOT EXISTS encrypted_temp (
    pubkey VARCHAR(66) PRIMARY KEY,
    encrypted_data BYTEA NOT NULL,
    timestamp BIGINT NOT NULL
);
