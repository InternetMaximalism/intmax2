# Aggregator-prover

## Development

```sh
# env
cp .env.example .env

## APIs

```sh
WITHDRAWAL_PROVER_URL=http://localhost:8080

# heath heck
http $WITHDRAWAL_PROVER_URL/health 
```

### Withdrawal

```sh
# generate proof
curl -X POST -d '{ "id": "1", "singleWithdrawalProof": "'$(base64 --input test_data/single_withdrawal_proof.bin)'", "prevWithdrawalProof": null }' -H "Content-Type: application/json" $WITHDRAWAL_PROVER_URL/proof/withdrawal | jq

# generate proof
curl -X POST -d '{ "id": "2", "singleWithdrawalProof": "'$(base64 --input test_data/single_withdrawal_proof.bin)'", "prevWithdrawalProof": '$(cat test_data/prev_withdrawal_proof.json)' }' -H "Content-Type: application/json" $WITHDRAWAL_PROVER_URL/proof/withdrawal | jq
```

#### get proof

```
curl $WITHDRAWAL_PROVER_URL/proof/withdrawal/1 | jq
```

Response

```json
{
  "success": true,
  "proof": {
    "proof": "AAA=",
    "withdrawal": {
      "recipient": "0xec34f2c34a6ff1c4c739d0d420a64b639c00c399",
      "tokenIndex": 0,
      "amount": "10",
      "nullifier": "0x0df256a220ebc2e8289792fbed2658b213a72b2623868596a32514e01d94f999",
      "blockHash": "0xc5ee7e8ea7b4934a38cd2e81b4a04b48719b9a7f7af050319e36302ca3e2eea6",
      "blockNumber": 3
    }
  },
  errorMessage: null
}
```

#### get proofs
```
curl "$WITHDRAWAL_PROVER_URL/proofs/withdrawal?ids[]=1&ids[]=2" | jq
```

Response

```json
{
  "success": true,
  "proofs": [
    {
      "id": "3",
      "proof": {
        "proof": "TAAA...AAA=",
        "withdrawal": {
          "recipient": "0xec34f2c34a6ff1c4c739d0d420a64b639c00c399",
          "tokenIndex": 0,
          "amount": "10",
          "nullifier": "0x0df256a220ebc2e8289792fbed2658b213a72b2623868596a32514e01d94f999",
          "blockHash": "0xc5ee7e8ea7b4934a38cd2e81b4a04b48719b9a7f7af050319e36302ca3e2eea6",
          "blockNumber": 3
        }
      }
    }
  ],
  "error_message": null
}
```

### Withdrawal Wrapper

```sh
# generate proof
curl -X POST -d '{ "id": "1", "withdrawalAggregator": "0x420a5b76e11e80d97c7eb3a0b16ac7b70672b8c2", "withdrawalProof": "'$(base64 --input data/withdrawal_proof.bin)'" }' -H "Content-Type: application/json" $WITHDRAWAL_PROVER_URL/proof/wrapper | jq

# get proof
curl $WITHDRAWAL_PROVER_URL/proof/wrapper/1 | jq

# get proofs
curl "$WITHDRAWAL_PROVER_URL/proofs/wrapper?ids[]=1&ids[]=2" | jq
```

## Docker

```sh
docker run -d \
  --name prover-redis \
  --hostname redis \
  --restart always \
  -p 6379:6379 \
  -v redisdata:/data \
  redis:7.2.3 \
  /bin/sh -c "redis-server --requirepass password"
```