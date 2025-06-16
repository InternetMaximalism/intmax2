use intmax2_zkp::common::block_builder::UserSignature;

pub fn remove_duplicate_signatures(signatures: &mut Vec<UserSignature>) {
    let mut seen = std::collections::HashSet::with_capacity(signatures.len());
    signatures.retain(|signature| seen.insert(signature.pubkey));
}
