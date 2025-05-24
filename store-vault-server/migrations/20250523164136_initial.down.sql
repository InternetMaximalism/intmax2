-- Drop Indexes
DROP INDEX IF EXISTS idx_s3_snapshot_pending_uploads_timestamp;
DROP INDEX IF EXISTS idx_s3_historical_data_unfinished;
DROP INDEX IF EXISTS idx_s3_historical_data_sequence_desc;
DROP INDEX IF EXISTS idx_s3_historical_data_sequence_asc;

-- Drop Tables
DROP TABLE IF EXISTS s3_snapshot_pending_uploads;
DROP TABLE IF EXISTS s3_historical_data;
DROP TABLE IF EXISTS s3_snapshot_data;