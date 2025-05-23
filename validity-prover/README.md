# Validity Prover

## Preparation

Create `.env` file. You need to specify Alchemy API key in `L2_RPC_URL`. 
```
cp .env.example .env
```

Install sqlx-cli. 

```bash
cargo install sqlx-cli
```

Launch database (if you haven't already).
```
docker run --name postgres -e POSTGRES_PASSWORD=password -p 5432:5432 -d postgres
```

## Starting the Node

```
sqlx database setup && cargo run -r
```
