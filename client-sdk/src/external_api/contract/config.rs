#[derive(Clone, Debug)]
pub struct EventConfig {
    pub max_event_entries: usize,
    // The range of blocks to be targeted for event retrieval in one go
    pub step_block_range: u64,
    // The maximum range of blocks to be targeted for event retrieval
    pub max_block_range: u64,
}
