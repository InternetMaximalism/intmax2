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

```sh
curl -X POST http://localhost:8080/config \
-H "Content-Type: application/json" \
-d '{"tps": 50, "concurrency": 10, "duration": 60}'
```