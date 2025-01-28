CREATE TYPE withdrawal_status AS ENUM (
    'requested',
    'relayed',
    'success',
    'need_claim',
    'failed'
);

CREATE TYPE claim_status AS ENUM (
    'requested',
    'verified',
    'relayed',
    'success',
    'failed'
);


CREATE TABLE IF NOT EXISTS withdrawals (
    uuid TEXT NOT NULL,
    status withdrawal_status NOT NULL DEFAULT 'requested',
    pubkey CHAR(66) NOT NULL,
    recipient CHAR(42) NOT NULL,
    withdrawal_hash CHAR(66) NOT NULL,
    contract_withdrawal jsonb NOT NULL,
    single_withdrawal_proof bytea,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (uuid)
);

CREATE TABLE IF NOT EXISTS claims (
    uuid TEXT NOT NULL,
    status claim_status NOT NULL DEFAULT 'requested',
    pubkey CHAR(66) NOT NULL,
    recipient CHAR(42) NOT NULL,
    nullifier CHAR(66) NOT NULL,
    claim jsonb NOT NULL,
    single_claim_proof bytea,
    withdrawal_hash CHAR(66),
    contract_withdrawal jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (uuid)
);

CREATE INDEX idx_withdrawals_pubkey ON withdrawals(pubkey);
CREATE INDEX idx_withdrawals_recipient ON withdrawals(recipient);