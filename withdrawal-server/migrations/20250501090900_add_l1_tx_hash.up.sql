ALTER TABLE withdrawals
ADD COLUMN l1_tx_hash CHAR(66);

ALTER TABLE claims
ADD COLUMN l1_tx_hash CHAR(66);