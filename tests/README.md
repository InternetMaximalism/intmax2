# Tests

## contract deployment

```sh
cargo test -r -p tests deploy_contracts -- --nocapture
```

## stress test

```sh
NUM_OF_RECIPIENTS=64 RECIPIENT_OFFSET=0 cargo test -r test_bulk_transfers -- --nocapture
```

```sh
NUM_OF_RECIPIENTS=128 cargo test -r test_sync_balance -- --nocapture
```

```sh
NUM_OF_RECIPIENTS=128 cargo test -r test_block_generation_included_many_senders -- --nocapture
```

## soak-test

- `CONCURRENT_LIMIT`: the initial number of INTMAX accounts
- `ETH_REFILL_OFFSET`: The offset of addresses to refill ETH before starting the transfer test
- `SERVER_URL`: the server URL [default: "localhost:8080"]
- `END`: the initial status of soak test [default: "false"]

- `PRIVATE_KEY`: INTMAX private key. You need to have at least 0.2 Gwei on INTMAX.
- `MASTER_MNEMONIC`: The mnemonic phrase used to generate the sender's and receiver's address.

Build config server.

```sh
cargo run -r --bin tests
```

Open another terminal and run test.

```sh
cargo run -r --bin soak-test
```

If you change the config, Run the following command:

```sh
curl -X POST http://localhost:8080/config \
-H "Content-Type: application/json" \
-d '{"concurrent_limit": 800, "end": "false"}'
```
