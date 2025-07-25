# Validity Prover

The Validity Prover is a critical service in the INTMAX2 network that monitors L1 Liquidity and L2 Rollup contracts, maintains state merkle trees, and generates validity proofs for on-chain information. It operates on port 9002 and provides cryptographic proof generation capabilities for blockchain state verification.

## Overview

The Validity Prover collects events and transactions from Liquidity and Rollup contracts to maintain synchronized state trees including block hash merkle tree, account merkle tree, and deposit merkle tree. It generates validity proofs that verify the correctness of on-chain information and provides this data to users and other services.

### Key Features

- **Event Monitoring**: Tracks L1 Liquidity and L2 Rollup contract events
- **State Tree Management**: Maintains block hash, account, and deposit merkle trees
- **Validity Proof Generation**: Creates zero-knowledge proofs for on-chain state verification
- **Database Storage**: Persistent storage for blockchain state and proofs
- **Worker Architecture**: Separate worker process for proof generation

## Architecture

```mermaid
graph TB
    L1[L1 Liquidity Contract] --> VP[Validity Prover :9002]
    L2[L2 Rollup Contract] --> VP

    subgraph "Validity Prover Components"
        VP --> API[API Server]
        VP --> Observer[Observer API]
        VP --> Trees[Merkle Trees]
        VP --> DB[(PostgreSQL)]
    end

    subgraph "State Trees"
        Trees --> BlockTree[Block Hash Tree]
        Trees --> AccountTree[Account Tree]
        Trees --> DepositTree[Deposit Tree]
    end

    subgraph "Worker Process"
        VPW[Validity Prover Worker] --> Redis[(Redis)]
        VPW --> ZKP[ZKP Generation]
    end

    VP --> Redis
    Redis --> VPW

    Client[Clients] --> API
    BlockBuilder[Block Builder] --> API
    BalanceProver[Balance Prover] --> API
```

## API Endpoints

The Validity Prover provides comprehensive API endpoints for accessing blockchain state and proofs:

### Block Information

#### GET /block-number

Returns the latest processed block number.

**Response:**

```json
{
  "block_number": 12345
}
```

#### GET /validity-proof-block-number

Returns the latest block number for which validity proofs are available.

**Response:**

```json
{
  "block_number": 12340
}
```

### Account Information

#### GET /get-account-info

Retrieves account information for a specific public key.

**Query Parameters:**

- `pubkey`: Public key to query

**Response:**

```json
{
  "account_id": 123,
  "account_index": 456
}
```

#### POST /get-account-info-batch

Batch retrieval of account information for multiple public keys.

**Request:**

```json
{
  "pubkeys": ["0x...", "0x..."]
}
```

**Response:**

```json
{
  "account_infos": [
    { "account_id": 123, "account_index": 456 },
    { "account_id": 124, "account_index": 457 }
  ]
}
```

**Batch Limit:** Maximum `MAX_BATCH_SIZE` requests per batch

### Deposit Information

#### GET /next-deposit-index

Returns the next available deposit index.

**Response:**

```json
{
  "deposit_index": 789
}
```

#### GET /last-deposit-id

Returns the last processed deposit ID.

**Response:**

```json
{
  "deposit_id": 456
}
```

#### GET /latest-included-deposit-index

Returns the latest deposit index included in blocks.

**Response:**

```json
{
  "deposit_index": 788
}
```

#### GET /get-deposit-info

Retrieves deposit information for a specific pubkey salt hash.

**Query Parameters:**

- `pubkey_salt_hash`: Hash to query

**Response:**

```json
{
  "deposit_id": 123,
  "token_index": 0,
  "block_number": 12340
}
```

#### POST /get-deposit-info-batch

Batch retrieval of deposit information.

**Request:**

```json
{
  "pubkey_salt_hashes": ["0x...", "0x..."]
}
```

**Response:**

```json
{
  "deposit_infos": [
    { "deposit_id": 123, "token_index": 0, "block_number": 12340 },
    { "deposit_id": 124, "token_index": 1, "block_number": 12341 }
  ]
}
```

### Transaction Information

#### GET /get-block-number-by-tx-tree-root

Returns block number for a given transaction tree root.

**Query Parameters:**

- `tx_tree_root`: Transaction tree root hash

**Response:**

```json
{
  "block_number": 12345
}
```

#### POST /get-block-number-by-tx-tree-root-batch

Batch retrieval of block numbers by transaction tree roots.

**Request:**

```json
{
  "tx_tree_roots": ["0x...", "0x..."]
}
```

**Response:**

```json
{
  "block_numbers": [12345, 12346]
}
```

### Witness and Proof Generation

#### GET /get-update-witness

Retrieves update witness data for balance proof generation.

**Query Parameters:**

- `account_id`: Account ID
- `block_number`: Target block number

**Response:**

```json
{
  "validity_witness": {
    /* witness data */
  },
  "update_witness": {
    /* update witness data */
  }
}
```

#### GET /get-validity-witness

Retrieves validity witness for proof generation.

**Query Parameters:**

- `block_number`: Target block number

**Response:**

```json
{
  "validity_witness": {
    /* validity witness data */
  }
}
```

#### GET /get-validity-proof

Retrieves generated validity proof for a specific block.

**Query Parameters:**

- `block_number`: Target block number

**Response:**

```json
{
  "validity_proof": {
    /* compressed proof data */
  }
}
```

#### GET /get-validity-pis

Retrieves validity public inputs for a specific block.

**Query Parameters:**

- `block_number`: Target block number

**Response:**

```json
{
  /* ValidityPublicInputs structure */
}
```

### Merkle Proofs

#### GET /get-block-merkle-proof

Retrieves merkle proof for block inclusion.

**Query Parameters:**

- `block_number`: Target block number

**Response:**

```json
{
  "merkle_proof": {
    /* block merkle proof */
  }
}
```

#### GET /get-deposit-merkle-proof

Retrieves merkle proof for deposit inclusion.

**Query Parameters:**

- `deposit_index`: Deposit index

**Response:**

```json
{
  "merkle_proof": {
    /* deposit merkle proof */
  }
}
```

## State Tree Management

### Tree Structure

The Validity Prover maintains three critical merkle trees:

#### 1. **Block Hash Tree** (`BLOCK_HASH_TREE_HEIGHT`)

- **Purpose**: Tracks block hashes for block inclusion proofs
- **Updates**: On each new block submission
- **Database Tag**: `BLOCK_DB_TAG` (2)

#### 2. **Account Tree** (`ACCOUNT_TREE_HEIGHT`)

- **Purpose**: Manages account registrations and state
- **Updates**: When users register or update account state
- **Database Tag**: `ACCOUNT_DB_TAG` (1)
- **Backup**: `ACCOUNT_BACKUP_DB_TAG` (11)

#### 3. **Deposit Tree** (`DEPOSIT_TREE_HEIGHT`)

- **Purpose**: Tracks deposit inclusions and processing
- **Updates**: On deposit events from Liquidity contract
- **Database Tag**: `DEPOSIT_DB_TAG` (3)

### Tree Implementation

```rust
// Incremental Merkle Tree (for append-only operations)
type SqlIncrementalMerkleTree = SqlIncrementalMerkleTree<F, C, D>;

// Indexed Merkle Tree (for indexed updates)
type SqlIndexedMerkleTree = SqlIndexedMerkleTree<F, C, D>;
```

**Key Features:**

- **SQL-backed Storage**: Trees stored in PostgreSQL for persistence
- **Incremental Updates**: Efficient tree updates without full recalculation
- **Proof Generation**: On-demand merkle proof generation
- **Backup Support**: Account tree backup functionality

## Validity Prover Worker

The Validity Prover Worker is a separate service that handles computationally intensive proof generation:

### Worker Architecture

```rust
pub struct Worker {
    config: Config,
    transition_processor: Arc<ValidityTransitionProcessor<F, C, D>>,
    manager: Arc<TaskManager<TransitionProofTask, TransitionProofTaskResult>>,
    worker_id: String,
    running_tasks: Arc<RwLock<HashSet<u32>>>,
}
```

### Task Processing Flow

```mermaid
sequenceDiagram
    participant VP as Validity Prover
    participant Redis as Redis Queue
    participant Worker as VP Worker
    participant ZKP as ZKP Processor

    VP->>Redis: Enqueue TransitionProofTask
    Note right of VP: block_number, prev_validity_pis, validity_witness

    Worker->>Redis: Poll for tasks
    Redis->>Worker: Assign task

    Worker->>ZKP: Generate validity proof
    Note right of Worker: spawn_blocking for CPU-intensive work
    ZKP-->>Worker: Proof generated

    Worker->>Redis: Submit TransitionProofTaskResult
    VP->>Redis: Retrieve completed proof
```

### Task Structure

```rust
// Input task for proof generation
pub struct TransitionProofTask {
    pub block_number: u32,
    pub prev_validity_pis: ValidityPublicInputs,
    pub validity_witness: ValidityWitness,
}

// Result after proof generation
pub struct TransitionProofTaskResult {
    pub block_number: u32,
    pub proof: Option<ProofWithPublicInputs<F, C, D>>,
}
```

### Worker Configuration

- **Polling Interval**: `TASK_POLLING_INTERVAL` (1 second)
- **Restart Wait**: `RESTART_WAIT_INTERVAL` (30 seconds)
- **Parallel Processing**: Configurable `num_process`
- **Heartbeat**: Configurable heartbeat interval for task management

## Event Processing

### Contract Event Monitoring

The Validity Prover monitors the following contract events:

#### Liquidity Contract Events

- **Deposit Events**: New deposits from L1
- **Token Registration**: New token additions
- **Configuration Updates**: Contract parameter changes

#### Rollup Contract Events

- **Block Submissions**: New L2 blocks
- **State Updates**: Account and deposit tree updates
- **Withdrawal Requests**: L2 to L1 withdrawal initiation

### Data Synchronization

```rust
// Observer API handles contract event processing
pub struct ObserverApi {
    // Event monitoring and processing logic
}
```

**Key Components:**

1. **Rate Manager**: Manages API call rates to avoid rate limiting
2. **Leader Election**: Ensures only one instance processes events
3. **Setting Consistency**: Validates configuration consistency
4. **Observer Graph**: Processes The Graph protocol data

## Database Schema

### Core Tables

The Validity Prover uses PostgreSQL with the following key tables:

#### Merkle Tree Nodes

- **Incremental Trees**: Stores tree nodes for block/deposit trees
- **Indexed Trees**: Stores indexed tree nodes for account tree
- **Node Hashes**: Cached hash computations

#### State Tracking

- **Block Information**: Block numbers, hashes, and timestamps
- **Account Data**: Account registrations and indices
- **Deposit Data**: Deposit information and processing status

### Migration Support

Database migrations are managed through `migrations/` directory:

- `20250521081620_initial.up.sql`: Initial schema creation
- `20250602024544_backup.up.sql`: Backup functionality

## Configuration

### Environment Variables

Key configuration parameters:

```bash
# Server Configuration
PORT=9002
DATABASE_URL=postgresql://user:pass@localhost:5432/validity_prover
REDIS_URL=redis://localhost:6379

# Contract Configuration
L1_RPC_URL=<ethereum-rpc-url>
L2_RPC_URL=<scroll-rpc-url>
LIQUIDITY_CONTRACT_ADDRESS=<contract-address>
ROLLUP_CONTRACT_ADDRESS=<contract-address>

# Worker Configuration
NUM_PROCESS=4
HEARTBEAT_INTERVAL=30
TASK_TTL=300

# Tree Configuration
ACCOUNT_TREE_HEIGHT=32
BLOCK_HASH_TREE_HEIGHT=32
DEPOSIT_TREE_HEIGHT=32
```

## Performance Considerations

### Optimization Strategies

1. **Batch Processing**: API endpoints support batch operations for efficiency
2. **Async Processing**: Worker architecture separates proof generation
3. **Database Indexing**: Optimized queries for tree operations
4. **Caching**: In-memory caching for frequently accessed data

### Scalability Features

- **Horizontal Scaling**: Multiple worker instances supported
- **Load Balancing**: Redis-based task distribution
- **Database Optimization**: SQL-based tree storage with indexing
- **Rate Limiting**: Built-in rate management for external API calls
