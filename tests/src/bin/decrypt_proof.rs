use ethers::utils::hex;
use intmax2_interfaces::{
    api::private_zkp_server::types::ProofResultWithError, data::encryption::BlsEncryption,
};

fn main() {
    let hex_data = std::fs::read_to_string(
        "../decrypted_data-0x278939ce08b7cfd42402b39e2fa3bfe3a7da2ce15abf0eef59a94d6c99791f1f.txt",
    )
    .unwrap();
    let decrypted_data = hex::decode(hex_data).unwrap();

    let data = ProofResultWithError::from_bytes(&decrypted_data).expect("13");
    log::info!("error: {:?}", data.error);
    log::info!(
        "Decrypted data: {:?}",
        data.proof.expect("15").public_inputs
    );
}
