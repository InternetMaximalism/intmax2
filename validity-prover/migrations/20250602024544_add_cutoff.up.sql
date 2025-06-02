CREATE TABLE IF NOT EXISTS cutoff (
    singleton_key BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_key = TRUE),
    block_number INTEGER NOT NULL
);
