-- Backup Script

-- Constants
\set BLOCK_OFFSET 1000
\set BACKUP_TAG_1 11
\set BACKUP_TAG_2 12
\set BACKUP_TAG_3 13
\set ORIGINAL_TAG_1 1
\set ORIGINAL_TAG_2 2
\set ORIGINAL_TAG_3 3

BEGIN;

\echo '--- Calculating new cutoff value ---'

-- Determine current cutoff from DB, default to 0 if record not found
CREATE TEMP TABLE _temp_current_cutoff_val AS
SELECT COALESCE((SELECT block_number FROM cutoff WHERE singleton_key = TRUE LIMIT 1), 0) AS val;

-- Get current max block number from validity_state
CREATE TEMP TABLE _temp_current_max_block AS
SELECT MAX(block_number) as val FROM validity_state;

-- Calculate new cutoff value
CREATE TEMP TABLE _temp_new_cutoff_calculated AS
SELECT
    GREATEST(
        COALESCE((SELECT val FROM _temp_current_max_block), 0) - :BLOCK_OFFSET,
        (SELECT val FROM _temp_current_cutoff_val)
    ) AS val;

-- Final new cutoff value, with fallbacks
CREATE TEMP TABLE _temp_final_new_cutoff AS
SELECT COALESCE((SELECT val FROM _temp_new_cutoff_calculated), (SELECT val FROM _temp_current_cutoff_val), 0) AS val;

-- Set psql variable for the final new cutoff value
SELECT val AS final_new_cutoff_value FROM _temp_final_new_cutoff \gset

\echo '--- Setting/Updating cutoff value in cutoff table ---'
-- Upsert logic: Update the row if singleton_key=TRUE exists, otherwise insert it.
INSERT INTO cutoff (singleton_key, block_number)
VALUES (TRUE, :'final_new_cutoff_value')
ON CONFLICT (singleton_key) DO UPDATE
SET block_number = EXCLUDED.block_number;

\echo 'Cutoff value in cutoff table ensured to be:':final_new_cutoff_value

\echo '--- Copying data up to new cutoff ---'
-- The psql variable 'final_new_cutoff_value' is already set

-- hash_nodes: ORIGINAL_TAG_1 -> BACKUP_TAG_1
\echo 'Copying hash_nodes: ' :'ORIGINAL_TAG_1' ' -> ' :'BACKUP_TAG_1'
INSERT INTO hash_nodes (tag, timestamp, bit_path, hash_value)
SELECT :'BACKUP_TAG_1', timestamp, bit_path, hash_value
FROM hash_nodes
WHERE tag = :'ORIGINAL_TAG_1' AND timestamp <= :'final_new_cutoff_value' -- Using the psql variable
ON CONFLICT (tag, timestamp, bit_path) DO NOTHING;

-- hash_nodes: ORIGINAL_TAG_2 -> BACKUP_TAG_2
\echo 'Copying hash_nodes: ' :'ORIGINAL_TAG_2' ' -> ' :'BACKUP_TAG_2'
INSERT INTO hash_nodes (tag, timestamp, bit_path, hash_value)
SELECT :'BACKUP_TAG_2', timestamp, bit_path, hash_value
FROM hash_nodes
WHERE tag = :'ORIGINAL_TAG_2' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp, bit_path) DO NOTHING;

-- hash_nodes: ORIGINAL_TAG_3 -> BACKUP_TAG_3
\echo 'Copying hash_nodes: ' :'ORIGINAL_TAG_3' ' -> ' :'BACKUP_TAG_3'
INSERT INTO hash_nodes (tag, timestamp, bit_path, hash_value)
SELECT :'BACKUP_TAG_3', timestamp, bit_path, hash_value
FROM hash_nodes
WHERE tag = :'ORIGINAL_TAG_3' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp, bit_path) DO NOTHING;

-- leaves: ORIGINAL_TAG_1 -> BACKUP_TAG_1
\echo 'Copying leaves: ' :'ORIGINAL_TAG_1' ' -> ' :'BACKUP_TAG_1'
INSERT INTO leaves (tag, timestamp, position, leaf_hash, leaf)
SELECT :'BACKUP_TAG_1', timestamp, position, leaf_hash, leaf
FROM leaves
WHERE tag = :'ORIGINAL_TAG_1' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp, position) DO NOTHING;

-- leaves: ORIGINAL_TAG_2 -> BACKUP_TAG_2
\echo 'Copying leaves: ' :'ORIGINAL_TAG_2' ' -> ' :'BACKUP_TAG_2'
INSERT INTO leaves (tag, timestamp, position, leaf_hash, leaf)
SELECT :'BACKUP_TAG_2', timestamp, position, leaf_hash, leaf
FROM leaves
WHERE tag = :'ORIGINAL_TAG_2' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp, position) DO NOTHING;

-- leaves: ORIGINAL_TAG_3 -> BACKUP_TAG_3
\echo 'Copying leaves: ' :'ORIGINAL_TAG_3' ' -> ' :'BACKUP_TAG_3'
INSERT INTO leaves (tag, timestamp, position, leaf_hash, leaf)
SELECT :'BACKUP_TAG_3', timestamp, position, leaf_hash, leaf
FROM leaves
WHERE tag = :'ORIGINAL_TAG_3' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp, position) DO NOTHING;

-- leaves_len: ORIGINAL_TAG_1 -> BACKUP_TAG_1
\echo 'Copying leaves_len: ' :'ORIGINAL_TAG_1' ' -> ' :'BACKUP_TAG_1'
INSERT INTO leaves_len (tag, timestamp, len)
SELECT :'BACKUP_TAG_1', timestamp, len
FROM leaves_len
WHERE tag = :'ORIGINAL_TAG_1' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp) DO NOTHING;

-- leaves_len: ORIGINAL_TAG_2 -> BACKUP_TAG_2
\echo 'Copying leaves_len: ' :'ORIGINAL_TAG_2' ' -> ' :'BACKUP_TAG_2'
INSERT INTO leaves_len (tag, timestamp, len)
SELECT :'BACKUP_TAG_2', timestamp, len
FROM leaves_len
WHERE tag = :'ORIGINAL_TAG_2' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp) DO NOTHING;

-- leaves_len: ORIGINAL_TAG_3 -> BACKUP_TAG_3
\echo 'Copying leaves_len: ' :'ORIGINAL_TAG_3' ' -> ' :'BACKUP_TAG_3'
INSERT INTO leaves_len (tag, timestamp, len)
SELECT :'BACKUP_TAG_3', timestamp, len
FROM leaves_len
WHERE tag = :'ORIGINAL_TAG_3' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, timestamp) DO NOTHING;

-- indexed_leaves: ORIGINAL_TAG_1 -> BACKUP_TAG_1
\echo 'Copying indexed_leaves: ' :'ORIGINAL_TAG_1' ' -> ' :'BACKUP_TAG_1'
INSERT INTO indexed_leaves (tag, timestamp, position, leaf_hash, next_index, key, next_key, value)
SELECT :'BACKUP_TAG_1', timestamp, position, leaf_hash, next_index, key, next_key, value
FROM indexed_leaves
WHERE tag = :'ORIGINAL_TAG_1' AND timestamp <= :'final_new_cutoff_value'
ON CONFLICT (tag, position, timestamp) DO NOTHING;

-- Drop temporary tables
DROP TABLE _temp_current_cutoff_val;
DROP TABLE _temp_current_max_block;
DROP TABLE _temp_new_cutoff_calculated;
DROP TABLE _temp_final_new_cutoff;

\echo '--- Backup complete ---'

COMMIT;