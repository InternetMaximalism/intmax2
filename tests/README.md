# Intmax2 Test CLI

This directory contains a test CLI tool for Intmax2 that allows you to run various test scenarios to verify the functionality of the Intmax2 system.

## Overview

The test CLI provides several commands to test different aspects of the Intmax2 system:

- **BridgeLoop**: Tests the bridge functionality by continuously depositing and withdrawing funds between Ethereum and Intmax2.
- **TransferLoop**: Tests the transfer functionality by continuously sending self-transfers within Intmax2.
- **MiningLoop**: Tests the mining functionality by continuously depositing, mining, claiming rewards, and withdrawing.
- **Info**: Displays information about the current state of an account on both Ethereum and Intmax2.

## Configuration Parameters

### Deposit Configuration
- `DEPOSIT_SYNC_CHECK_INTERVAL`: Interval in seconds to check if a deposit has been synced to the validity prover
- `DEPOSIT_SYNC_CHECK_RETRIES`: Number of retries for checking deposit sync
- `DEPOSIT_RELAY_CHECK_INTERVAL`: Interval in seconds to check if a deposit has been relayed to L2
- `DEPOSIT_RELAY_CHECK_RETRIES`: Number of retries for checking deposit relay

### Withdrawal Configuration
- `WITHDRAWAL_CHECK_INTERVAL`: Interval in seconds to check withdrawal status
- `WITHDRAWAL_CHECK_RETRIES`: Number of retries for checking withdrawal status

### Mining Configuration
- `MINING_INFO_CHECK_INTERVAL`: Interval in seconds to check mining info
- `MINING_INFO_CHECK_RETRIES`: Number of retries for checking mining info
- `CLAIM_CHECK_WAIT_TIME`: Time in seconds to wait before checking claim status
- `CLAIM_CHECK_INTERVAL`: Interval in seconds to check claim status
- `CLAIM_CHECK_RETRIES`: Number of retries for checking claim status

### Transaction Configuration
- `BLOCK_BUILDER_QUERY_WAIT_TIME`: Time in seconds to wait before querying the block builder
- `BLOCK_SYNC_MARGIN`: Margin in seconds to add to block expiry time
- `TX_STATUS_CHECK_INTERVAL`: Interval in seconds to check transaction status
- `TX_RESEND_INTERVAL`: Interval in seconds to wait before resending a transaction
- `TX_RESEND_RETRIES`: Number of retries for resending a transaction

### Loop Configuration
- `BRIDGE_LOOP_ETH_WAIT_TIME`: Time in seconds to wait between withdrawal and deposit in bridge loop
- `BRIDGE_LOOP_INTMAX_WAIT_TIME`: Time in seconds to wait between deposit and withdrawal in bridge loop
- `TRANSFER_LOOP_WAIT_TIME`: Time in seconds to wait between transfers in transfer loop
- `MINING_LOOP_ETH_WAIT_TIME`: Time in seconds to wait between mining operations

## Usage

### Display Account Information

```bash
cargo run -- info --eth-private-key 0xYOUR_PRIVATE_KEY
```

### Run Bridge Loop Test

This test continuously deposits and withdraws funds between Ethereum and Intmax2.

```bash
cargo run -- bridge-loop --eth-private-key 0xYOUR_PRIVATE_KEY
```

To start the bridge loop from withdrawal instead of deposit:

```bash
cargo run -- bridge-loop --eth-private-key 0xYOUR_PRIVATE_KEY --from-withdrawal
```

### Run Transfer Loop Test

This test continuously sends self-transfers within Intmax2.

```bash
cargo run -- transfer-loop --eth-private-key 0xYOUR_PRIVATE_KEY
```

### Run Mining Loop Test

This test continuously deposits, mines, claims rewards, and withdraws.

```bash
cargo run -- mining-loop --eth-private-key 0xYOUR_PRIVATE_KEY
```

## Test Flow Details

### Bridge Loop

1. Deposits ETH from Ethereum to Intmax2
2. Waits for the deposit to be synced to the validity prover
3. Waits for the deposit to be relayed to L2
4. Syncs the balance on Intmax2
5. Waits for the configured time (`BRIDGE_LOOP_INTMAX_WAIT_TIME`)
6. Withdraws the funds from Intmax2 to Ethereum
7. Waits for the withdrawal to be processed
8. Checks that the withdrawal is reflected in the Ethereum balance
9. Waits for the configured time (`BRIDGE_LOOP_ETH_WAIT_TIME`)
10. Repeats the process

### Transfer Loop

1. Checks if there is sufficient balance on Intmax2
2. Creates a self-transfer transaction
3. Sends the transaction
4. Waits for the transaction to be confirmed
5. Syncs the balance on Intmax2
6. Waits for the configured time (`TRANSFER_LOOP_WAIT_TIME`)
7. Repeats the process

### Mining Loop

1. Deposits a fixed amount (0.1 ETH) from Ethereum to Intmax2
2. Waits for the deposit to be synced and relayed
3. Checks mining info and waits for maturity
4. Waits for the mining status to become claimable
5. Initiates a withdrawal
6. Syncs claims
7. Checks claim status until successful
8. Repeats the process
