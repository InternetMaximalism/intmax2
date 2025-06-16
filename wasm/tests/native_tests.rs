#![cfg(target_arch = "wasm32")]

use intmax2_interfaces::utils::random::default_rng;
use intmax2_wasm_lib::native::{
    calc_simple_aggregated_pubkey, encrypt_message, extract_address_aux_info,
    generate_integrated_address, is_valid_intmax_address, sign_message, verify_signature,
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

#[wasm_bindgen_test]
fn test_is_valid_intmax_address() {
    // Test with valid devnet address
    let valid_address = "X8GjPwLr5ZiX85RJpZ1in6VzsAYsoHGbzMGfDKh1ovRw5Yc83zJmvnR6cKC6xRN5g2jM6MMxstnApa1T7wLMESFUVT3GemZ";
    assert!(
        validate_intmax_address(valid_address),
        "Valid address should pass validation"
    );

    // Test with invalid addresses
    assert!(
        !validate_intmax_address("invalid_address"),
        "Invalid address should fail validation"
    );
}

#[wasm_bindgen_test]
fn test_extract_address_aux_info() {
    // Test with standard address
    let standard_address = "X8GjPwLr5ZiX85RJpZ1in6VzsAYsoHGbzMGfDKh1ovRw5Yc83zJmvnR6cKC6xRN5g2jM6MMxstnApa1T7wLMESFUVT3GemZ";
    let aux_info = extract_address_aux_info(standard_address)
        .expect("Should extract aux info from valid address");

    assert_eq!(aux_info.network, "devnet", "Network should be devnet");
    assert!(
        aux_info.payment_id.is_none(),
        "Standard address should have no payment ID"
    );

    // Test with integrated address (if we can create one)
    let payment_id = "0x1234567890abcdef";

    let integrated_address = generate_integrated_address(standard_address, payment_id)
        .expect("Should generate integrated address");

    let aux_info = extract_address_aux_info(&integrated_address)
        .expect("Should extract aux info from integrated address");
    assert_eq!(aux_info.network, "devnet", "Network should be devnet");
    assert!(
        aux_info.payment_id.unwrap() == payment_id,
        "Payment ID should match"
    );
}
