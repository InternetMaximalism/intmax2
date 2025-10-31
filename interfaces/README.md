# INTMAX2 Interfaces

The `intmax2-interfaces` crate provides common interface definitions, data structures, and utilities used across the INTMAX2 network. This crate serves as the foundation for communication between different services and components in the INTMAX2 ecosystem.

## Overview

This crate contains:

- **API Interfaces**: Trait definitions for service communication
- **Data Structures**: Common data types and serialization formats
- **Utilities**: Cryptographic utilities, key management, and helper functions
- **Circuit Data**: Pre-compiled circuit verification data

## API Interfaces

The `api` module defines trait-based interfaces for service communication, enabling loose coupling and testability.

### Service Interfaces

#### Validity Prover Interface
```rust
#[async_trait(?Send)]
pub trait ValidityProverClientInterface: Sync + Send {
    async fn get_block_number(&self) -> Result<u32, ServerError>;
    async fn get_validity_proof_block_number(&self) -> Result<u32, ServerError>;
    async fn get_deposit_info(&self, pubkey_salt_hash: Bytes32) -> Result<Option<DepositInfo>, ServerError>;
    async fn get_account_info(&self, pubkey: U256) -> Result<AccountInfo, ServerError>;
    // ... additional methods
}
```

#### Block Builder Interface
- Block construction and validation
- Transaction processing
- State transition management

#### Balance Prover Interface
- Balance proof generation
- Account state verification
- Merkle proof creation

#### Store Vault Server Interface
- Data storage and retrieval
- Backup management
- Data synchronization

#### Withdrawal Server Interface
- Withdrawal request processing
- L2 to L1 bridge operations
- Withdrawal proof generation

#### Wallet Key Vault Interface
- Key management and storage
- Cryptographic operations
- Secure key derivation

### Common Types

#### Account Information
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_id: Option<u64>,
    pub block_number: u32,
    pub last_block_number: u32,
}
```

#### Deposit Information
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositInfo {
    pub deposit_id: u64,
    pub token_index: u32,
    pub deposit_hash: Bytes32,
    pub block_number: Option<u32>,
    pub deposit_index: Option<u32>,
    pub l1_deposit_tx_hash: Bytes32,
}
```

#### Proof Task Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionProofTask {
    pub block_number: u32,
    pub prev_validity_pis: ValidityPublicInputs,
    pub validity_witness: ValidityWitness,
}
```

## Data Structures

The `data` module provides comprehensive data types for the INTMAX2 protocol.

### Core Data Types

#### Transfer Data
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferData {
    pub sender_proof_set_ephemeral_key: PrivateKey,
    pub sender_proof_set: Option<SenderProofSet>,
    pub sender: PublicKeyPair,
    pub extra_data: ExtraData,
    pub tx: Tx,
    pub tx_index: u32,
    pub tx_merkle_proof: TxMerkleProof,
    pub tx_tree_root: Bytes32,
    pub transfer: Transfer,
    pub transfer_index: u32,
    pub transfer_merkle_proof: TransferMerkleProof,
}
```

#### User Data
- User account information
- Balance and state data
- Transaction history

#### Transaction Data
- Transaction structure and validation
- Merkle proof inclusion
- State transition data

#### Deposit Data
- L1 to L2 deposit information
- Deposit processing status
- Cross-chain verification data

### Data Categories

#### Validation
- Data validation traits and implementations
- Consistency checking
- Integrity verification

#### Encryption
- **BLS Encryption**: Multi-signature and threshold encryption
- **RSA Encryption**: Traditional public key encryption
- **Versioned Encryption**: Backward-compatible encryption schemes

#### Metadata
- Transaction metadata
- Block metadata
- User metadata

#### Proof Compression
- ZK proof compression algorithms
- Efficient proof serialization
- Batch proof optimization

## Utilities

The `utils` module provides essential cryptographic and system utilities.

### Cryptographic Utilities

#### Key Management
```rust
pub struct PublicKeyPair {
    pub view: PublicKey,
    pub spend: PublicKey,
}

pub struct PrivateKey(pub U256);
pub struct PublicKey(pub U256);
```

#### Signature Operations
- Digital signature creation and verification
- Multi-signature schemes
- Signature aggregation

#### Address Generation
- Account address derivation
- Deterministic address generation
- Address validation

### System Utilities

#### Circuit Verifiers
- Pre-compiled circuit verification data
- Efficient proof verification
- Circuit parameter management

#### Network Utilities
- Network configuration
- Chain identification
- RPC endpoint management

#### Serialization
- Efficient binary serialization
- JSON serialization for APIs
- Cross-platform compatibility

#### Random Number Generation
- Cryptographically secure randomness
- Deterministic random generation
- Entropy management

#### Fee Calculation
- Transaction fee estimation
- Gas price calculation
- Fee optimization strategies

#### Payment ID
- Unique payment identification
- Payment tracking
- Transaction correlation

## Circuit Data

The `circuit_data` directory contains pre-compiled verification data for various ZK circuits:

### Available Circuits

- **`balance_verifier_circuit_data.bin`**: Balance proof verification
- **`validity_verifier_circuit_data.bin`**: State validity verification
- **`transition_verifier_circuit_data.bin`**: State transition verification
- **`single_claim_verifier_circuit_data.bin`**: Single claim verification
- **`faster_single_claim_verifier_circuit_data.bin`**: Optimized single claim verification
- **`single_withdrawal_verifier_circuit_data.bin`**: Withdrawal verification
- **`spent_verifier_circuit_data.bin`**: Spent proof verification

### Circuit Integration

```rust
use intmax2_interfaces::utils::circuit_verifiers::CircuitVerifiers;

// Load circuit verification data
let verifiers = CircuitVerifiers::load();
let transition_vd = verifiers.get_transition_vd();
```

## Error Handling

### Common Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Internal server error: {0}")]
    InternalError(String),
    
    #[error("Invalid request: {0}")]
    BadRequest(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}
```

### Encryption Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum BlsEncryptionError {
    #[error("Unsupported encryption version: {0}")]
    UnsupportedVersion(u8),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),
}
```

## Configuration

### Dependencies

The crate relies on several key dependencies:

```toml
[dependencies]
plonky2 = { workspace = true }           # ZK proof system
intmax2-zkp = { workspace = true }       # INTMAX2 ZK primitives
alloy = { workspace = true }             # Ethereum types
serde = { workspace = true }             # Serialization
tokio = { workspace = true }             # Async runtime
ark-ec = { workspace = true }            # Elliptic curve cryptography
```

### Feature Flags

#### WASM Support
```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
js-sys = "0.3"
```

The crate includes conditional compilation for WebAssembly targets, enabling browser-based applications.

## Usage Examples

### Service Client Implementation

```rust
use intmax2_interfaces::api::validity_prover::interface::ValidityProverClientInterface;
use intmax2_interfaces::api::error::ServerError;

struct ValidityProverClient {
    base_url: String,
    client: reqwest::Client,
}

#[async_trait(?Send)]
impl ValidityProverClientInterface for ValidityProverClient {
    async fn get_block_number(&self) -> Result<u32, ServerError> {
        let response = self.client
            .get(&format!("{}/block-number", self.base_url))
            .send()
            .await?;
        
        let block_info: BlockNumberResponse = response.json().await?;
        Ok(block_info.block_number)
    }
    
    // ... implement other methods
}
```

### Data Validation

```rust
use intmax2_interfaces::data::validation::Validation;
use intmax2_interfaces::data::transfer_data::TransferData;

fn validate_transfer(transfer_data: &TransferData) -> anyhow::Result<()> {
    transfer_data.validate()?;
    println!("Transfer data is valid");
    Ok(())
}
```

### Encryption Operations

```rust
use intmax2_interfaces::data::encryption::BlsEncryption;
use intmax2_interfaces::data::transfer_data::TransferData;

fn decrypt_transfer_data(encrypted_bytes: &[u8], version: u8) -> Result<TransferData, BlsEncryptionError> {
    TransferData::from_bytes(encrypted_bytes, version)
}
```

## Best Practices

### Interface Implementation

1. **Async Traits**: Use `#[async_trait(?Send)]` for service interfaces
2. **Error Handling**: Implement comprehensive error types
3. **Serialization**: Use consistent serde annotations
4. **Validation**: Implement validation traits for data integrity

### Data Structure Design

1. **Versioning**: Support backward-compatible data formats
2. **Validation**: Include validation logic in data structures
3. **Encryption**: Use appropriate encryption schemes for sensitive data
4. **Compression**: Implement efficient serialization for large data

### Utility Usage

1. **Key Management**: Use secure key derivation and storage
2. **Randomness**: Use cryptographically secure random number generation
3. **Circuit Integration**: Leverage pre-compiled circuit data for efficiency
4. **Error Propagation**: Use proper error handling throughout the stack

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transfer_data_validation() {
        let transfer_data = create_test_transfer_data();
        assert!(transfer_data.validate().is_ok());
    }
    
    #[tokio::test]
    async fn test_validity_prover_interface() {
        let client = MockValidityProverClient::new();
        let block_number = client.get_block_number().await.unwrap();
        assert!(block_number > 0);
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_service_integration() {
    let validity_prover = ValidityProverClient::new("http://localhost:9002");
    let block_builder = BlockBuilderClient::new("http://localhost:9001");
    
    let latest_block = validity_prover.get_block_number().await.unwrap();
    let builder_block = block_builder.get_latest_block().await.unwrap();
    
    assert_eq!(latest_block, builder_block);
}
```

## Contributing

When contributing to the interfaces crate:

1. **Maintain Compatibility**: Ensure backward compatibility for existing interfaces
2. **Documentation**: Document all public APIs and data structures
3. **Testing**: Include comprehensive tests for new functionality
4. **Versioning**: Use appropriate versioning for breaking changes
5. **Performance**: Consider performance implications of interface changes

## Security Considerations

1. **Key Management**: Never expose private keys in interfaces
2. **Data Validation**: Always validate input data
3. **Encryption**: Use appropriate encryption for sensitive data
4. **Error Information**: Avoid leaking sensitive information in error messages
5. **Circuit Security**: Ensure circuit data integrity and authenticity
