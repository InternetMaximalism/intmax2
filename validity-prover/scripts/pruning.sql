-- Pruning Script

-- Original tags to be pruned
\set ORIGINAL_TAG_1 1
\set ORIGINAL_TAG_2 2
\set ORIGINAL_TAG_3 3

BEGIN;

\echo '--- Fetching cutoff value for pruning ---'

-- Determine current cutoff from DB, default to 0 if record not found
CREATE TEMP TABLE _temp_pruning_cutoff_val AS
SELECT COALESCE((SELECT block_number FROM cutoff WHERE singleton_key = TRUE LIMIT 1), 0) as val;

-- Set psql variable for the pruning cutoff value
SELECT val AS timestamp_cutoff FROM _temp_pruning_cutoff_val \gset

\echo '--- Starting pruning (Cutoff: ' :'timestamp_cutoff' ' ) ---'

-- Pruning hash_nodes
\echo 'Pruning hash_nodes...'
WITH latest_within_cutoff AS (
    SELECT
        tag,
        bit_path,
        MAX(timestamp) AS max_ts
    FROM hash_nodes
    WHERE timestamp <= :'timestamp_cutoff' -- Using the psql variable
      AND tag IN (:'ORIGINAL_TAG_1', :'ORIGINAL_TAG_2', :'ORIGINAL_TAG_3')
    GROUP BY tag, bit_path
)
DELETE FROM hash_nodes h
USING latest_within_cutoff l
WHERE h.tag = l.tag
  AND h.bit_path = l.bit_path
  AND h.timestamp <= :'timestamp_cutoff'
  AND h.timestamp < l.max_ts;

-- Pruning leaves
\echo 'Pruning leaves...'
WITH latest_within_cutoff AS (
    SELECT
        tag,
        position,
        MAX(timestamp) AS max_ts
    FROM leaves
    WHERE timestamp <= :'timestamp_cutoff'
      AND tag IN (:'ORIGINAL_TAG_1', :'ORIGINAL_TAG_2', :'ORIGINAL_TAG_3')
    GROUP BY tag, position
)
DELETE FROM leaves l_del
USING latest_within_cutoff l_max
WHERE l_del.tag = l_max.tag
  AND l_del.position = l_max.position
  AND l_del.timestamp <= :'timestamp_cutoff'
  AND l_del.timestamp < l_max.max_ts;

-- Pruning leaves_len
\echo 'Pruning leaves_len...'
WITH latest_within_cutoff AS (
    SELECT
        tag,
        MAX(timestamp) AS max_ts
    FROM leaves_len
    WHERE timestamp <= :'timestamp_cutoff'
      AND tag IN (:'ORIGINAL_TAG_1', :'ORIGINAL_TAG_2', :'ORIGINAL_TAG_3')
    GROUP BY tag
)
DELETE FROM leaves_len ll
USING latest_within_cutoff l_max
WHERE ll.tag = l_max.tag
  AND ll.timestamp <= :'timestamp_cutoff'
  AND ll.timestamp < l_max.max_ts;

-- Pruning indexed_leaves (Pruning for tag 1 only, as per original logic)
\echo 'Pruning indexed_leaves...'
WITH latest_within_cutoff AS (
    SELECT
        tag,
        position,
        MAX(timestamp) AS max_ts
    FROM indexed_leaves
    WHERE timestamp <= :'timestamp_cutoff'
      AND tag IN (:'ORIGINAL_TAG_1')
    GROUP BY tag, position
)
DELETE FROM indexed_leaves il
USING latest_within_cutoff l_max
WHERE il.tag = l_max.tag
  AND il.position = l_max.position
  AND il.timestamp <= :'timestamp_cutoff'
  AND il.timestamp < l_max.max_ts;

-- Drop temporary table
DROP TABLE _temp_pruning_cutoff_val;

\echo '--- Pruning complete ---'

COMMIT;