# Validity Prover

The Validity Prover is a service in the INTMAX2 network that monitors L1 Liquidity and L2 Rollup contracts, maintains state merkle trees, and generates validity proofs for on-chain information. It operates on port 9002 and provides ZKP generation capabilities for blockchain state verification.

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

## Data Flow and Event Processing

### Overview

The Validity Prover implements a comprehensive data flow system that monitors blockchain events, stores them in database tables, and updates merkle trees to generate validity proofs. The system uses a multi-stage pipeline with robust synchronization mechanisms.

### Event Types and Sources

The system monitors three types of events from two different contracts:

#### L1 Liquidity Contract Events

- **Deposited Events**: User deposits from L1 to L2
  - **Source**: Liquidity Contract on L1
  - **Storage**: `deposited_events` table
  - **Fields**: `deposit_id`, `depositor`, `pubkey_salt_hash`, `token_index`, `amount`, `is_eligible`, `deposited_at`, `deposit_hash`, `tx_hash`, `eth_block_number`, `eth_tx_index`

#### L2 Rollup Contract Events

- **DepositLeafInserted Events**: Deposit inclusion confirmations

  - **Source**: Rollup Contract on L2
  - **Storage**: `deposit_leaf_events` table
  - **Fields**: `deposit_index`, `deposit_hash`, `eth_block_number`, `eth_tx_index`

- **BlockPosted Events**: New L2 block submissions
  - **Source**: Rollup Contract on L2
  - **Storage**: `full_blocks` table
  - **Fields**: `block_number`, `full_block` (serialized), `eth_block_number`, `eth_tx_index`

### Observer Synchronization Mechanism

#### Checkpoint-Based Sync

The observer uses a checkpoint system to track synchronization progress:

```rust
pub struct CheckPointStore {
    // Tracks last processed Ethereum block number for each event type
}
```

**Checkpoint Storage**: `event_sync_eth_block` table

- **Purpose**: Stores the last processed Ethereum block number for each event type
- **Recovery**: Enables resumption from the last known good state after failures

#### Sync Process Flow

```mermaid
sequenceDiagram
    participant O as Observer
    participant CP as CheckPoint Store
    participant L1 as L1 Contract
    participant L2 as L2 Contract
    participant DB as Database

    loop Every sync_interval
        O->>CP: Get last checkpoint
        O->>L1/L2: Fetch events from checkpoint
        O->>O: Validate event sequence
        O->>DB: Store events in batch
        O->>CP: Update checkpoint
    end
```

#### Event Synchronization Details

**1. Event Gap Detection**

```rust
// Ensures no events are missed in the sequence
if first.deposit_id != expected_next_event_id {
    return Err(ObserverError::EventGapDetected {
        expected_next_event_id,
        got_event_id: first.deposit_id,
    });
}
```

**2. Batch Processing**

- Events are fetched in configurable block intervals (`observer_event_block_interval`)
- Maximum query attempts per sync cycle (`observer_max_query_times`)
- Automatic retry with exponential backoff on failures

**3. Leader Election**

- Only one observer instance processes events at a time
- Prevents duplicate event processing in multi-instance deployments
- Uses Redis-based distributed locking

### Data Flow Pipeline

#### Stage 1: Event Collection and Storage

```mermaid
graph LR
    L1[L1 Liquidity Contract] -->|Deposited Events| DE[deposited_events table]
    L2[L2 Rollup Contract] -->|DepositLeafInserted| DLE[deposit_leaf_events table]
    L2 -->|BlockPosted| FB[full_blocks table]
```

#### Stage 2: Validity Witness Generation

The `sync_validity_witness` process transforms stored events into validity witnesses:

```rust
// For each new block
let validity_witness = update_trees(
    &block_witness,
    block_number as u64,
    &self.account_tree,
    &self.block_tree,
).await?;
```

**Process Steps:**

1. **Deposit Processing**: Retrieves deposits between blocks from `deposit_leaf_events`
2. **Tree Updates**: Updates account, block, and deposit merkle trees
3. **Witness Generation**: Creates `ValidityWitness` containing all necessary proofs
4. **Storage**: Saves witness to `validity_state` table

#### Stage 3: Merkle Tree Updates

**Account Tree Updates** (Tag 1):

- **Registration Blocks**: New account registrations with membership proofs
- **Non Registration Blocks**: Account state updates with inclusion proofs

**Block Hash Tree Updates** (Tag 2):

- Appends new block hashes for inclusion proofs
- Maintains chronological block history

**Deposit Tree Updates** (Tag 3):

- Processes deposit events in order
- Validates deposit tree root consistency

#### Stage 4: Proof Generation

```mermaid
graph TB
    VW[Validity Witness] --> TPT[TransitionProofTask]
    TPT --> Redis[Redis Queue]
    Redis --> Worker[Validity Prover Worker]
    Worker --> ZKP[ZK Proof Generation]
    ZKP --> VP[Validity Proof]
    VP --> DB[(validity_proofs table)]
```

### Error Handling and Recovery

#### Automatic Recovery Mechanisms

**1. Checkpoint Reset**

```rust
// Resets to last known good state on errors
async fn reset_check_point(&self, event_type: EventType, reason: &str)
```

**2. State Consistency Validation**

```rust
// Validates deposit tree root consistency
if full_block.block.deposit_tree_root != deposit_tree_root {
    self.reset_state().await?;
    return Err(ValidityProverError::DepositTreeRootMismatch);
}
```

**3. Restart Loops**

- Observer jobs automatically restart on failures
- Configurable error thresholds before stopping
- Rate limiting to prevent excessive API calls

#### Monitoring and Observability

**Rate Manager Integration**:

- Tracks sync progress with heartbeats
- Monitors error rates and failure patterns
- Implements circuit breaker patterns

**Leader Election**:

- Ensures single active observer instance
- Prevents race conditions in event processing
- Enables high availability deployments

## Database Schema

The Validity Prover uses PostgreSQL with a comprehensive schema designed for high-performance merkle tree operations and event processing.

### Core Tables

#### Event Tables

- `deposited_events`: L1 deposit information with fields like `deposit_id`, `depositor`, `pubkey_salt_hash`, `token_index`, `amount`, `is_eligible`, `deposited_at`, `deposit_hash`, `tx_hash`, `eth_block_number`, `eth_tx_index`
- `deposit_leaf_events`: L2 deposit confirmations with `deposit_index`, `deposit_hash`, `eth_block_number`, `eth_tx_index`
- `full_blocks`: Complete L2 block data with `block_number`, `full_block` (serialized), `eth_block_number`, `eth_tx_index`

#### State Tables

- `validity_state`: Generated validity witnesses stored per block number
- `validity_proofs`: Final ZK proofs for each validated block
- `tx_tree_roots`: Transaction tree root to block number mappings

#### Merkle Tree Tables (Partitioned by Tag)

- `hash_nodes`: Tree node hashes with partitioning by tag for performance
- `leaves`: Tree leaf data organized by tag, timestamp, and position
- `leaves_len`: Tree length tracking for each tag and timestamp
- `indexed_leaves`: Indexed tree data specifically for account tree operations

#### Synchronization Tables

- `event_sync_eth_block`: Observer checkpoints tracking last processed Ethereum block numbers
- `cutoff`: Backup/pruning cutoff points for historical data management

### Migration Support

Database migrations are managed through `migrations/` directory:

- `20250521081620_initial.up.sql`: Initial schema creation with all core tables
- `20250602024544_backup.up.sql`: Backup functionality and additional partitions

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

## Database Tag System

The Validity Prover uses a tag-based partitioning system for merkle tree data storage. Each table (`hash_nodes`, `leaves`, `leaves_len`, `indexed_leaves`) includes a `tag` column that categorizes data by tree type and purpose:

### Tag Assignments

```rust
const ACCOUNT_DB_TAG: u32 = 1;          // Account tree data
const BLOCK_DB_TAG: u32 = 2;            // Block hash tree data
const DEPOSIT_DB_TAG: u32 = 3;          // Deposit tree data
const ACCOUNT_BACKUP_DB_TAG: u32 = 11;  // Account tree backup
const BLOCK_BACKUP_DB_TAG: u32 = 12;    // Block hash tree backup
const DEPOSIT_BACKUP_DB_TAG: u32 = 13;  // Deposit tree backup
```

### Database Partitioning

PostgreSQL table partitioning is used to optimize query performance:

- **Primary Tables**: `hash_nodes_tag1`, `leaves_tag1`, etc. (tags 1-3)
- **Backup Tables**: `hash_nodes_tag11`, `leaves_tag11`, etc. (tags 11-13)

## Backup and Pruning

### Performance Challenges

As the database trees grow, query and update performance degrades due to PostgreSQL B-tree index limitations. B-tree indexes only encode the leading index columns (in this case, `position` or `bit_path`) in the tree path. When multiple timestamps exist for the same `position` or `bit_path`, timestamp queries become O(N), causing performance degradation.

### Pruning

**Purpose**: Remove old timestamp data to improve query and update performance.

**Process**: The pruning script (`scripts/pruning.sql`) removes historical data while preserving the latest state for each tree node:

- Keeps the most recent timestamp for each `(tag, bit_path)` combination in `hash_nodes`
- Keeps the most recent timestamp for each `(tag, position)` combination in `leaves`
- Keeps the most recent timestamp for each `tag` in `leaves_len`
- For `indexed_leaves`, only processes account tree data (tag 1)

### Backup

**Purpose**: Create copies of tree data before pruning to enable queries on historical timestamps.

**Process**: The backup script (`scripts/backup.sql`) copies data from primary tags (1-3) to backup tags (11-13):

1. **Cutoff Calculation**: Determines backup cutoff as `MAX(block_number) - BLOCK_OFFSET` (default offset: 1000 blocks)
2. **Data Copy**: Copies all data with timestamps ≤ cutoff from primary to backup tables
3. **Cutoff Storage**: Updates the `cutoff` table with the backup timestamp

### Script Usage

Both operations can be performed using the scripts in the `scripts/` directory:

```bash
# Create backup before pruning
psql -d validity_prover -f scripts/backup.sql

# Remove old data to improve performance
psql -d validity_prover -f scripts/pruning.sql
```

**Important**: Always run backup before pruning to preserve historical data access.
