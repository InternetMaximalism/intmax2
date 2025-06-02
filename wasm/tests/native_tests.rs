#![cfg(target_arch = "wasm32")]

use intmax2_interfaces::utils::random::default_rng;
use intmax2_wasm_lib::native::{
    calc_simple_aggregated_pubkey, encrypt_message, sign_message, verify_signature,
};
use intmax2_zkp::{
    common::signature_content::key_set::KeySet, ethereum_types::u32limb_trait::U32LimbTrait,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!();

const TEST_MESSAGE: &[u8] = b"Hello, zk-world!";

#[wasm_bindgen_test]
async fn test_sign_and_verify_message() {
    let mut rng = default_rng();
    let key = KeySet::rand(&mut rng);

    let sig = sign_message(&key.privkey.to_hex(), TEST_MESSAGE)
        .await
        .expect("Should sign");
    let is_valid = verify_signature(&sig, &key.pubkey.to_hex(), TEST_MESSAGE)
        .await
        .expect("Should verify");

    assert!(is_valid, "Signature should be verified");
}

#[wasm_bindgen_test]
fn test_encrypt_message() {
    let pubkey = "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef";
    let ciphertext = encrypt_message(pubkey, TEST_MESSAGE);
    assert!(
        !ciphertext.is_empty(),
        "Enycrypted message should not be empty"
    );
}

#[wasm_bindgen_test]
fn test_calc_aggregated_pubkey() {
    let mut rng = default_rng();
    let server_key = KeySet::rand(&mut rng);
    let client_key = KeySet::rand(&mut rng);

    let signers = vec![server_key.pubkey.to_hex(), client_key.pubkey.to_hex()];
    let aggregated =
        calc_simple_aggregated_pubkey(signers).expect("Aggregated pubkey should be calculated");
    assert!(!aggregated.is_empty());
}
