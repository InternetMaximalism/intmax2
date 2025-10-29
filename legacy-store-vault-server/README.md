# Legacy Store Vault Server

Legacy Store Vault Server is a simplified data storage service for the INTMAX2 protocol that provides secure data backup, retrieval, and transfer capabilities between users. Unlike the regular Store Vault Server, this version stores data directly in the database instead of using S3 as a backend, making it easier to set up and ideal for local development and testing.

## Architecture Overview

Legacy Store Vault Server serves the same primary roles as the regular Store Vault Server:

1. **User Data Storage & Retrieval**: Users can store their own state data as backups and retrieve them when needed
2. **Inter-User Data Transfer**: Acts as a mailbox for users to send data to other users

### Key Differences from Store Vault Server

- **Direct Database Storage**: Data is stored directly in PostgreSQL database tables instead of S3
- **Simplified Setup**: No AWS/CloudFront configuration required
- **Local Development Focus**: Optimized for development and testing environments
- **Same API Interface**: Maintains compatibility with the regular Store Vault Server API

### Data Types

#### Snapshot Data

- **Purpose**: Single-file updates with state management
- **Control**: Uses optimistic locking for conflict resolution
- **Characteristics**:
  - One record per user per topic
  - Updates replace previous versions
  - Atomic operations with rollback support

#### Historical Data

- **Purpose**: Append-only data storage
- **Control**: No locking mechanism (append-only)
- **Characteristics**:
  - Immutable once stored
  - Time-ordered sequence
  - Batch operations supported

### Data Storage Architecture

All data follows the logical path structure: `{topic}/{pubkey}/{digest}`

- **pubkey**: User identifier (top-level partition)
- **topic**: Data type/category classifier
- **digest**: Content hash (unique file identifier)

## API Endpoints

### Snapshot Data APIs

```mermaid
sequenceDiagram
    participant Client
    participant Server
    participant DB

    Note over Client,DB: Snapshot Data Flow (Direct DB Storage)

    Client->>Server: POST /save-snapshot
    Server->>DB: Check current digest (optimistic lock)
    Server->>DB: Validate prev_digest
    Server->>DB: Insert/Update snapshot data
    Server-->>Client: Confirm success

    Client->>Server: POST /get-snapshot
    Server->>DB: Query snapshot data
    Server-->>Client: Return data directly
```

### Historical Data APIs

```mermaid
sequenceDiagram
    participant Client
    participant Server
    participant DB

    Note over Client,DB: Historical Data Flow (Direct DB Storage)

    Client->>Server: POST /save-data-batch
    Server->>DB: Bulk insert historical data
    Server-->>Client: Return digest list

    Client->>Server: POST /get-data-batch
    Server->>DB: Query by digests
    Server-->>Client: Return data with metadata

    Client->>Server: POST /get-data-sequence
    Server->>DB: Query with pagination
    Server-->>Client: Return data with cursor
```

## Database Schema

### Snapshot Data Table

```sql
CREATE TABLE snapshot_data (
    pubkey VARCHAR(66) NOT NULL,      -- User public key
    topic VARCHAR(255) NOT NULL,      -- Data topic/category
    digest VARCHAR(66) NOT NULL,      -- Content hash
    data BYTEA NOT NULL,              -- Actual data content
    timestamp BIGINT NOT NULL,        -- Creation/update timestamp
    UNIQUE (pubkey, topic)            -- One snapshot per user per topic
);
```

### Historical Data Table

```sql
CREATE TABLE historical_data (
    digest VARCHAR(66) PRIMARY KEY,   -- Content hash (unique)
    pubkey VARCHAR(66) NOT NULL,      -- User public key
    topic VARCHAR(255) NOT NULL,      -- Data topic/category
    data BYTEA NOT NULL,              -- Actual data content
    timestamp BIGINT NOT NULL         -- Creation timestamp
);
```

## Security & Access Control

### Permission Types

- **SingleAuthWrite/SingleOpenWrite**: Single-state writes (snapshots)
- **AuthWrite/OpenWrite**: Historical data writes
- **AuthRead/OpenRead**: Read permissions

### Authentication Flow

```mermaid
graph LR
    A[Client Request] --> B[Signature Verification]
    B --> C[Extract Pubkey]
    C --> D[Validate Topic Rights]
    D --> E[Check Auth Permissions]
    E --> F[Process Request]

    D --> G[SingleAuthWrite: pubkey must match]
    D --> H[AuthWrite: pubkey must match]
    D --> I[OpenWrite: any pubkey allowed]

    style F fill:#90EE90
    style G fill:#87CEEB
    style H fill:#87CEEB
    style I fill:#87CEEB
```

## Configuration

### Environment Variables

Create a `.env` file in your project root and set the following variables:

```bash
# Server Configuration
PORT=9000

# Environment
ENV=local

# Database Configuration
DATABASE_URL="postgres://postgres:password@localhost/legacy_store_vault_server"
DATABASE_MAX_CONNECTIONS=10
DATABASE_TIMEOUT=10  # seconds
```

## Setup Instructions

### 1. Database Setup

**Install PostgreSQL:**

```bash
# macOS
brew install postgresql
brew services start postgresql

# Ubuntu/Debian
sudo apt-get install postgresql postgresql-contrib
sudo systemctl start postgresql
```

**Create Database:**

```bash
# Connect to PostgreSQL
psql -U postgres

# Create database
CREATE DATABASE legacy_store_vault_server;

# Create user (optional)
CREATE USER store_vault_user WITH PASSWORD 'password';
GRANT ALL PRIVILEGES ON DATABASE legacy_store_vault_server TO store_vault_user;
```

### 2. Environment Configuration

Copy the example environment file:

```bash
cp .env.example .env
```

Update the `.env` file with your database configuration:

```bash
DATABASE_URL="postgres://username:password@localhost/legacy_store_vault_server"
```

### 3. Database Migration

Run the database migrations to create the required tables:

```bash
# Install sqlx-cli if not already installed
cargo install sqlx-cli

# Run migrations
sqlx migrate run --database-url "postgres://username:password@localhost/legacy_store_vault_server"
```

## Running the Server

### Development Mode

```bash
cargo run
```

### Release Mode

```bash
cargo run --release
```

The server will start on the port specified in your `.env` file (default: 9000).

## API Usage Examples

### Save Snapshot

```bash
curl -X POST http://localhost:9000/save-snapshot \
  -H "Content-Type: application/json" \
  -d '{
    "inner": {
      "topic": "user_state",
      "pubkey": "0x1234...",
      "prev_digest": null,
      "data": [1, 2, 3, 4]
    },
    "auth": {
      "pubkey": "0x1234...",
      "signature": "0xabcd..."
    }
  }'
```

### Get Snapshot

```bash
curl -X POST http://localhost:9000/get-snapshot \
  -H "Content-Type: application/json" \
  -d '{
    "inner": {
      "topic": "user_state",
      "pubkey": "0x1234..."
    },
    "auth": {
      "pubkey": "0x1234...",
      "signature": "0xabcd..."
    }
  }'
```

### Save Historical Data Batch

```bash
curl -X POST http://localhost:9000/save-data-batch \
  -H "Content-Type: application/json" \
  -d '{
    "inner": {
      "data": [
        {
          "topic": "transactions",
          "pubkey": "0x1234...",
          "data": [1, 2, 3, 4]
        }
      ]
    },
    "auth": {
      "pubkey": "0x1234...",
      "signature": "0xabcd..."
    }
  }'
```

## Error Handling

- **Lock Errors**: Optimistic lock failures on snapshot updates
- **Validation Errors**: Permission and data integrity checks
- **Database Errors**: Connection and query failures
- **Authentication Errors**: Invalid signatures or permissions

## Testing

Run the test suite:

```bash
cargo test
```

The tests include:

- Snapshot data operations with optimistic locking
- Historical data batch operations
- Data sequence retrieval with pagination
- Error handling scenarios
