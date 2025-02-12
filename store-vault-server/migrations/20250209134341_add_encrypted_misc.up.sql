CREATE TABLE IF NOT EXISTS encrypted_misc (
    uuid TEXT PRIMARY KEY,
    topic VARCHAR(66) NOT NULL,
    pubkey VARCHAR(66) NOT NULL,
    encrypted_data BYTEA NOT NULL,
    timestamp BIGINT NOT NULL
);
