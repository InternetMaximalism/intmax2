use intmax2_zkp::{
    common::deposit::Deposit,
    ethereum_types::bytes32::Bytes32,
    utils::{leafable::Leafable, leafable_hasher::KeccakLeafableHasher},
};
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepositHash(pub Bytes32);

impl Leafable for DepositHash {
    type LeafableHasher = KeccakLeafableHasher;

    fn empty_leaf() -> Self {
        DepositHash(Deposit::default().hash())
    }

    fn hash(&self) -> Bytes32 {
        self.0
    }
}
