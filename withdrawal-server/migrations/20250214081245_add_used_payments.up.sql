CREATE TABLE IF NOT EXISTS used_payments (
    nullifier CHAR(66) PRIMARY KEY,
    transfer jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);