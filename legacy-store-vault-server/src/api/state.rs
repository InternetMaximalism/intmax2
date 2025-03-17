use crate::app::store_vault_server::StoreVaultServer;

pub struct State {
    pub store_vault_server: StoreVaultServer,
}

impl State {
    pub fn new(store_vault_server: StoreVaultServer) -> Self {
        Self { store_vault_server }
    }
}
