use crate::app::s3_store_vault::S3StoreVault;

pub struct State {
    pub s3_store_vault: S3StoreVault,
}

impl State {
    pub fn new(s3_store_vault: S3StoreVault) -> Self {
        Self { s3_store_vault }
    }
}
