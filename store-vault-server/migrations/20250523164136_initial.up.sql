CREATE TABLE IF NOT EXISTS s3_snapshot_data (
    pubkey VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    digest VARCHAR(66) NOT NULL,
    "timestamp" BIGINT NOT NULL,
    UNIQUE (pubkey, topic)
);

CREATE TABLE IF NOT EXISTS s3_historical_data (
    digest VARCHAR(66) PRIMARY KEY,
    pubkey VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    upload_finished BOOLEAN NOT NULL,
    "timestamp" BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS s3_snapshot_pending_uploads (
    digest VARCHAR(66) PRIMARY KEY,
    pubkey VARCHAR(66) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    "timestamp" BIGINT NOT NULL
);

-- Index for ascending sequence queries (pagination).
CREATE INDEX IF NOT EXISTS idx_s3_historical_data_sequence_asc
ON s3_historical_data (pubkey, topic, "timestamp" ASC, digest ASC);

-- Index for descending sequence queries (pagination).
CREATE INDEX IF NOT EXISTS idx_s3_historical_data_sequence_desc
ON s3_historical_data (pubkey, topic, "timestamp" DESC, digest DESC);

-- Partial index for quickly finding unfinished uploads for cleanup.
CREATE INDEX IF NOT EXISTS idx_s3_historical_data_unfinished
ON s3_historical_data (digest) WHERE upload_finished = false;

-- Index for finding pending uploads by timestamp, useful for cleanup.
CREATE INDEX IF NOT EXISTS idx_s3_snapshot_pending_uploads_timestamp
ON s3_snapshot_pending_uploads ("timestamp");
