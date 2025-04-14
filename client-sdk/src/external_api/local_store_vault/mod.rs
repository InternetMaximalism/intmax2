// topic: &str, pubkey: U256, digest:
// canonical file path: topic/pubkey/digest.dat
// file content: data
// canonical metadata file path: topic/pubkey/metadata.csv
// metadata csv file structure: digest,timestamp
// diff csv file structure: topic,pubkey,digest,timestamp,data

pub mod diff_data_client;
pub mod error;
pub mod local_data_client;
// pub mod local_store_vault;
pub mod metadata_client;
