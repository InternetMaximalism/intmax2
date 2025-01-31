# Aggregator-prover

## Development

```sh
# env
cp .env.example .env

## APIs

```sh
PROVER_URL=http://localhost:8080

# heath heck
http $PROVER_URL/health 
```

### Withdrawal

```sh
# generate proof
curl -X POST -d '{ "id": "1", "singleWithdrawalProof": "'$(cat test_data/single_withdrawal_proof.txt)'", "prevWithdrawalProof": null }' -H "Content-Type: application/json" $PROVER_URL/proof/withdrawal | jq

# generate proof
curl -X POST -d '{ "id": "2", "singleWithdrawalProof": "'$(cat test_data/single_withdrawal_proof.txt)'", "prevWithdrawalProof": "'$(cat test_data/withdrawal_proof.txt)'" }' -H "Content-Type: application/json" $PROVER_URL/proof/withdrawal | jq
```

#### get proof

```
curl $PROVER_URL/proof/withdrawal/1 | jq
```

Response

```json
{
  "success": true,
  "proof": {
    "proof": "AAA=",
    "withdrawal": {
      "recipient": "0xd267c67f2a1c9b754a27c8e27d32758641e8434a",
      "tokenIndex": 0,
      "amount": "1000",
      "nullifier": "0x7717e1ae50be08d94ee5ae5c8c1a314619f1255921bce4ac642ba4f4d97dfe67",
      "blockHash": "0x0597f8beb025cbe314ecce32c822a785d1914e0500f8321a1594b0833e54b0c2",
      "blockNumber": 2
    }
  },
  errorMessage: null
}
```

### Withdrawal Wrapper

```sh
# generate proof
curl -X POST -d '{ "id": "1", "withdrawalAggregator": "0x420a5b76e11e80d97c7eb3a0b16ac7b70672b8c2", "withdrawalProof": "'$(cat test_data/withdrawal_proof.txt)'" }' -H "Content-Type: application/json" $PROVER_URL/proof/wrapper/withdrawal | jq

# get proof
curl $PROVER_URL/proof/wrapper/withdrawal/1 | jq


### Claim

```sh
# generate proof
curl -X POST -d '{ "id": "1", "singleClaimProof": "'$(cat test_data/single_claim_proof.txt)'", "prevClaimProof": null }' -H "Content-Type: application/json" $PROVER_URL/proof/claim | jq

# generate proof
curl -X POST -d '{ "id": "2", "singleClaimProof": "'$(cat test_data/single_claim_proof.txt)'", "prevClaimProof": "'$(cat test_data/claim_proof.txt)'" }' -H "Content-Type: application/json" $PROVER_URL/proof/claim | jq
```

#### get proof

```
curl $PROVER_URL/proof/claim/1 | jq
```

Response

```json
{
  "success": true,
  "proof": {
    "proof": "AAA=",
    "claim": {
      "recipient": "0xd267c67f2a1c9b754a27c8e27d32758641e8434a",
      "amount": "1000",
      "nullifier": "0x7717e1ae50be08d94ee5ae5c8c1a314619f1255921bce4ac642ba4f4d97dfe67",
      "blockHash": "0x0597f8beb025cbe314ecce32c822a785d1914e0500f8321a1594b0833e54b0c2",
      "blockNumber": 2
    }
  },
  errorMessage: null
}
```

### Claim Wrapper

```sh
# generate proof
curl -X POST -d '{ "id": "1", "claimAggregator": "0x420a5b76e11e80d97c7eb3a0b16ac7b70672b8c2", "claimProof": "'$(cat test_data/claim_proof.txt)'" }' -H "Content-Type: application/json" $PROVER_URL/proof/wrapper/claim | jq

# get proof
curl $PROVER_URL/proof/wrapper/claim/1 | jq


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