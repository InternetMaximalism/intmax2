CREATE TABLE IF NOT EXISTS s3_snapshot_data (
    pubkey VARCHAR(66) PRIMARY KEY,
    digest VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    timestamp BIGINT NOT NULL,
    UNIQUE (pubkey, topic)
);

CREATE TABLE IF NOT EXISTS s3_historical_data (
    digest VARCHAR(66) PRIMARY KEY,
    pubkey VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    upload_finished BOOLEAN NOT NULL,
    timestamp BIGINT NOT NULL
);