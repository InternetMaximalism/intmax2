-- truncate tree related tables
TRUNCATE TABLE validity_state;
TRUNCATE TABLE hash_nodes;
TRUNCATE TABLE leaves;
TRUNCATE TABLE leaves_len;

CREATE TABLE IF NOT EXISTS indexed_leaves (
    timestamp_value bigint NOT NULL,
    tag int NOT NULL,
    position bigint NOT NULL,
    leaf_hash bytea NOT NULL,
    next_index bigint NOT NULL,
    key NUMERIC(78, 0) NOT NULL,
    next_key NUMERIC(78, 0) NOT NULL,
    value bigint NOT NULL,
    PRIMARY KEY (timestamp_value, tag, position)
);

CREATE INDEX idx_indexed_leaves_lookup ON indexed_leaves (position, tag, timestamp_value DESC);
CREATE INDEX idx_indexed_leaves_timestamp ON indexed_leaves (timestamp_value DESC, tag);
