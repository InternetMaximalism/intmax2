use super::types::TxRequest;

pub mod config;
pub mod error;
pub mod memory_storage;

pub trait BuilderStorage {
    fn new(config: &config::StateConfig) -> Self;
    fn enqueue_tx_request(&self, is_registration: bool, tx_request: TxRequest);
    fn process_requests(&self, is_registration: bool);
    fn process_memo(&self, block_id: &str);
    
}
