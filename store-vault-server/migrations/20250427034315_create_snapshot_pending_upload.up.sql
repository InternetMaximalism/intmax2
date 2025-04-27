CREATE TABLE IF NOT EXISTS s3_snapshot_pending_uploads (
    digest VARCHAR(66) PRIMARY KEY,
    pubkey VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    timestamp BIGINT NOT NULL
);