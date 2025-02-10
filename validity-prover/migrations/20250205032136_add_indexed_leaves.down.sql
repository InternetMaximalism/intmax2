-- truncate tree related tables
TRUNCATE TABLE validity_state;
TRUNCATE TABLE hash_nodes;
TRUNCATE TABLE leaves;
TRUNCATE TABLE leaves_len;

DROP TABLE IF EXISTS indexed_leaves;
