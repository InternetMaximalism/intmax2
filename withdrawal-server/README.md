# Withdrawal Server

The Withdrawal Server is a core service in the INTMAX2 network that handles user withdrawal and mining claim requests. It operates on port 9003 and serves as the entry point for users to request withdrawals from the L2 network to L1 Ethereum, as well as to claim mining rewards.

## Overview

The Withdrawal Server's primary responsibility is to receive, validate, and store user requests for withdrawals and claims. It performs ZKP (Zero-Knowledge Proof) validation and fee payment validation before storing the requests in the database. The server itself does not process the actual withdrawals or claims - this is handled by separate aggregator jobs that update the status of requests asynchronously.

### Key Responsibilities

1. **Request Reception**: Accept withdrawal and mining claim requests from users
2. **ZKP Validation**: Verify zero-knowledge proofs for withdrawal and claim requests
3. **Fee Validation**: Validate fee payments and prevent double-spending
4. **Data Storage**: Store validated requests in PostgreSQL database
5. **Status Tracking**: Provide APIs to query request status and history

**Note**: The Withdrawal Server only handles the initial request processing and storage. Subsequent processing (relaying to L1, status updates) is performed by external aggregator jobs.

## Architecture

```mermaid
graph TB
    User[Users] --> WS[Withdrawal Server :9003]
    WS --> DB[(PostgreSQL)]
    WS --> SV[Store Vault Server]
    WS --> VP[Validity Prover]
    WS --> RC[Rollup Contract]
    WS --> WC[Withdrawal Contract]

    subgraph "External Processing"
        AJ[Aggregator Jobs] --> DB
        AJ --> L1[L1 Ethereum]
    end

    subgraph "Withdrawal Server Components"
        WS --> API[API Routes]
        WS --> Val[Validators]
        WS --> FH[Fee Handler]
        WS --> DBO[DB Operations]
    end
```

## API Endpoints

### GET /withdrawal-fee

Returns withdrawal fee information and beneficiary details.

**Response:**

```json
{
  "beneficiary": "intmax1...",
  "direct_withdrawal_fee": [{ "token_index": 0, "amount": "100" }],
  "claimable_withdrawal_fee": [{ "token_index": 0, "amount": "10" }]
}
```

### GET /claim-fee

Returns claim fee information and beneficiary details.

**Response:**

```json
{
  "beneficiary": "intmax1...",
  "fee": [{ "token_index": 0, "amount": "100" }]
}
```

### POST /request-withdrawal

Submits a withdrawal request with ZKP validation.

**Request:**

```json
{
  "inner": {
    "single_withdrawal_proof": {
      /* ZKP proof data */
    },
    "fee_token_index": 0,
    "fee_transfer_digests": ["0x..."]
  },
  "auth": {
    "pubkey": "0x...",
    "signature": "0x..."
  }
}
```

**Response:**

```json
{
  "fee_result": "Success" // or "InsufficientFee", "InvalidFee", etc.
}
```

### POST /request-claim

Submits a mining claim request with ZKP validation.

**Request:**

```json
{
  "inner": {
    "single_claim_proof": {
      /* ZKP proof data */
    },
    "fee_token_index": 0,
    "fee_transfer_digests": ["0x..."]
  },
  "auth": {
    "pubkey": "0x...",
    "signature": "0x..."
  }
}
```

**Response:**

```json
{
  "fee_result": "Success"
}
```

### POST /get-withdrawal-info

Retrieves withdrawal history for a specific user.

**Request:**

```json
{
  "inner": {
    "cursor": {
      "cursor": 1640995200,
      "limit": 50,
      "order": "Desc"
    }
  },
  "auth": {
    "pubkey": "0x...",
    "signature": "0x..."
  }
}
```

**Response:**

```json
{
  "withdrawal_info": [
    {
      "status": "Success",
      "contract_withdrawal": {
        "recipient": "0x...",
        "token_index": 0,
        "amount": "1000000000000000000",
        "nullifier": "0x..."
      },
      "l1_tx_hash": "0x...",
      "requested_at": 1640995200
    }
  ],
  "cursor_response": {
    "next_cursor": 1640995100,
    "has_more": true,
    "total_count": 25
  }
}
```

### POST /get-claim-info

Retrieves claim history for a specific user.

**Request:**

```json
{
  "inner": {
    "cursor": {
      "cursor": 1640995200,
      "limit": 50,
      "order": "Desc"
    }
  },
  "auth": {
    "pubkey": "0x...",
    "signature": "0x..."
  }
}
```

**Response:**

```json
{
  "claim_info": [
    {
      "status": "Success",
      "claim": {
        "recipient": "0x...",
        "token_index": 0,
        "amount": "1000000000000000000",
        "nullifier": "0x...",
        "block_number": 12345,
        "block_hash": "0x..."
      },
      "submit_claim_proof_tx_hash": "0x...",
      "l1_tx_hash": "0x...",
      "requested_at": 1640995200
    }
  ],
  "cursor_response": {
    "next_cursor": 1640995100,
    "has_more": true,
    "total_count": 10
  }
}
```

### GET /get-withdrawal-info-by-recipient

Retrieves withdrawal information by recipient address (public endpoint).

**Query Parameters:**

- `recipient`: Ethereum address (0x...)
- `cursor`: Optional timestamp cursor
- `limit`: Optional limit (default: 100)
- `order`: "Asc" or "Desc" (default: "Desc")

**Response:**

```json
{
  "withdrawal_info": [
    /* same as get-withdrawal-info */
  ],
  "cursor_response": {
    /* same as get-withdrawal-info */
  }
}
```

## Database Schema

### Tables

#### `withdrawals`

Stores withdrawal requests and their processing status.

| Column                    | Type              | Description                                              |
| ------------------------- | ----------------- | -------------------------------------------------------- |
| `withdrawal_hash`         | CHAR(66)          | Primary key - hash of withdrawal data                    |
| `status`                  | withdrawal_status | Current processing status                                |
| `pubkey`                  | CHAR(66)          | User's public key                                        |
| `recipient`               | CHAR(42)          | Ethereum recipient address                               |
| `contract_withdrawal`     | JSONB             | Withdrawal details (recipient, token, amount, nullifier) |
| `single_withdrawal_proof` | BYTEA             | Compressed ZKP proof                                     |
| `l1_tx_hash`              | CHAR(66)          | L1 transaction hash (set by aggregator)                  |
| `created_at`              | TIMESTAMPTZ       | Request timestamp                                        |

#### `claims`

Stores mining claim requests and their processing status.

| Column                       | Type         | Description                                          |
| ---------------------------- | ------------ | ---------------------------------------------------- |
| `nullifier`                  | CHAR(66)     | Primary key - unique claim identifier                |
| `status`                     | claim_status | Current processing status                            |
| `pubkey`                     | CHAR(66)     | User's public key                                    |
| `recipient`                  | CHAR(42)     | Ethereum recipient address                           |
| `claim`                      | JSONB        | Claim details (recipient, token, amount, block info) |
| `single_claim_proof`         | BYTEA        | Compressed ZKP proof                                 |
| `withdrawal_hash`            | CHAR(66)     | Associated withdrawal hash (if applicable)           |
| `contract_withdrawal`        | JSONB        | Associated withdrawal data (if applicable)           |
| `submit_claim_proof_tx_hash` | CHAR(66)     | Claim proof submission tx hash                       |
| `l1_tx_hash`                 | CHAR(66)     | L1 transaction hash (set by aggregator)              |
| `created_at`                 | TIMESTAMPTZ  | Request timestamp                                    |

#### `used_payments`

Tracks spent fee payments to prevent double-spending.

| Column       | Type        | Description                     |
| ------------ | ----------- | ------------------------------- |
| `nullifier`  | CHAR(66)    | Primary key - payment nullifier |
| `transfer`   | JSONB       | Transfer details                |
| `created_at` | TIMESTAMPTZ | Payment timestamp               |

### Status Enums

#### Withdrawal Status Flow

```
requested → relayed → success
         ↘ need_claim → (claim process)
         ↘ failed
```

- **`requested`**: Initial state after validation
- **`relayed`**: Submitted to L1 contract (by aggregator)
- **`success`**: Successfully processed on L1
- **`need_claim`**: Requires manual claim process
- **`failed`**: Processing failed

#### Claim Status Flow

```
requested → verified → relayed → success
                    ↘ failed
```

- **`requested`**: Initial state after validation
- **`verified`**: ZKP verified (by aggregator)
- **`relayed`**: Submitted to L1 contract (by aggregator)
- **`success`**: Successfully processed on L1
- **`failed`**: Processing failed

## Request Processing Flow

### Withdrawal Request Processing

```mermaid
sequenceDiagram
    participant User
    participant WS as Withdrawal Server
    participant VP as Validity Prover
    participant RC as Rollup Contract
    participant DB as Database

    User->>WS: POST /request-withdrawal
    WS->>WS: Verify ZKP proof
    WS->>RC: Validate block hash existence
    WS->>WS: Validate fee payment
    WS->>DB: Check for duplicate withdrawal_hash
    alt No duplicate
        WS->>DB: Insert withdrawal record (status: requested)
        WS-->>User: Return success
    else Duplicate exists
        WS-->>User: Return success (idempotent)
    end

    Note over WS: Aggregator jobs handle subsequent processing
```

### Claim Request Processing

```mermaid
sequenceDiagram
    participant User
    participant WS as Withdrawal Server
    participant VP as Validity Prover
    participant RC as Rollup Contract
    participant DB as Database

    User->>WS: POST /request-claim
    WS->>WS: Verify ZKP proof
    WS->>RC: Validate block hash existence
    WS->>WS: Validate fee payment
    WS->>DB: Check for duplicate nullifier
    alt No duplicate
        WS->>DB: Insert claim record (status: requested)
        WS-->>User: Return success
    else Duplicate exists
        WS-->>User: Return success (idempotent)
    end

    Note over WS: Aggregator jobs handle subsequent processing
```

## Validation Logic

### ZKP Validation

- **Withdrawal Proofs**: Verified using single withdrawal circuit verifier
- **Claim Proofs**: Verified using claim circuit verifier (supports faster mining mode)
- **Block Hash Validation**: Ensures referenced block exists on L2

### Fee Validation

- **Fee Calculation**: Based on token type (direct vs claimable withdrawal)
- **Payment Verification**: Validates fee transfer proofs against Store Vault
- **Double-Spend Prevention**: Checks nullifiers against `used_payments` table
- **Atomic Processing**: Fee payments are recorded atomically with request storage

### Duplicate Prevention

- **Withdrawals**: Prevented by unique `withdrawal_hash` constraint
- **Claims**: Prevented by unique `nullifier` constraint
- **Fee Payments**: Prevented by unique `nullifier` constraint in `used_payments`

## Environment Configuration

Key environment variables (see `.env.example`):

```bash
# Server Configuration
PORT=9003

# Database Configuration
DATABASE_URL=postgres://postgres:password@localhost:5432/withdrawal
DATABASE_MAX_CONNECTIONS=10
DATABASE_TIMEOUT=10

# Fee Configuration
WITHDRAWAL_BENEFICIARY_VIEW_PAIR=viewpair/0x.../0x...
CLAIM_BENEFICIARY_VIEW_PAIR=viewpair/0x.../0x...
DIRECT_WITHDRAWAL_FEE="0:100"      # token_index:amount
CLAIMABLE_WITHDRAWAL_FEE="0:10"    # token_index:amount
CLAIM_FEE="0:100"                  # token_index:amount

# Circuit Configuration
IS_FASTER_MINING=true              # Use faster mining circuit for claims

# Service Dependencies
L2_RPC_URL=http://127.0.0.1:8545
STORE_VAULT_SERVER_BASE_URL=http://localhost:9000
USE_S3=false                       # Use S3 or local store vault
VALIDITY_PROVER_BASE_URL=http://localhost:9002
ROLLUP_CONTRACT_ADDRESS=0xe7f1725e7734ce288f8367e1bb143e90bb3f0512
WITHDRAWAL_CONTRACT_ADDRESS=0x8a791620dd6260079bf849dc5567adc3f2fdc318
```

## Integration with Other Services

### Store Vault Server

- Validates fee transfer proofs
- Provides access to user balance and transfer data
- Supports both S3-based and server-based implementations

### Validity Prover

- Provides block hash validation
- Ensures referenced blocks exist in the L2 chain

### Rollup Contract

- Source of truth for block hash validation
- Provides nonce and block information

### Withdrawal Contract

- Defines direct withdrawal token indices
- Determines fee structure based on withdrawal type

## Error Handling

### Common Error Types

- **`SingleWithdrawalVerificationError`**: ZKP proof verification failed
- **`SingleClaimVerificationError`**: Claim proof verification failed
- **`InvalidFee`**: Fee calculation or validation error
- **`DuplicateNullifier`**: Attempt to reuse spent payment
- **`SerializationError`**: Data serialization/deserialization error

### Idempotent Operations

- Duplicate withdrawal requests (same `withdrawal_hash`) return success
- Duplicate claim requests (same `nullifier`) return success
- This ensures safe retry behavior for clients
