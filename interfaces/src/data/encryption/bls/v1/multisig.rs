use ark_bn254::{G1Affine, G1Projective};
use ark_std::Zero as _;
use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::u256::U256};
use plonky2_bn254::fields::{recover::RecoverFromX, sgn::Sgn};

use super::{
    message::EncryptedMessage,
    utils::{aggregate_ecdh_x, ecdh_xy},
};

pub use alloy_primitives::bytes::BytesMut;

pub fn decrypt_bls_interaction(
    server_key: KeySet,
    client_key: KeySet,
    encrypted_data: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let step1_response = decrypt_bls_interaction_step1(client_key, encrypted_data); // client
    let step2_response = decrypt_bls_interaction_step2(server_key, &step1_response)?; // server
    let step3_response =
        decrypt_bls_interaction_step3(client_key, &step1_response, &step2_response)?; // client
    let decrypted_data = step3_response.message;

    Ok(decrypted_data)
}

pub fn decrypt_bls_preprocess(encrypted_data: &[u8]) -> anyhow::Result<&[u8]> {
    let (version_data, encrypted_data) = encrypted_data.split_at(1);
    let version = version_data[0];
    if version != 1 {
        anyhow::bail!("Unsupported version");
    }

    Ok(encrypted_data)
}

#[derive(Debug, Clone)]
pub struct MultiEciesStep1Response {
    pub encrypted_data: Vec<u8>,
    pub client_pubkey: U256,
}

pub fn decrypt_bls_interaction_step1(
    client_key: KeySet,
    encrypted_data: &[u8],
) -> MultiEciesStep1Response {
    MultiEciesStep1Response {
        encrypted_data: encrypted_data.to_vec(),
        client_pubkey: client_key.pubkey,
    }
}

#[derive(Debug, Clone)]
pub struct MultiEciesStep2Response {
    pub server_ecdh_share: (U256, bool),
    pub server_pubkey: U256,
}

pub fn decrypt_bls_interaction_step2(
    server_key: KeySet,
    step1_response: &MultiEciesStep1Response,
) -> anyhow::Result<MultiEciesStep2Response> {
    let encrypted_data = decrypt_bls_preprocess(&step1_response.encrypted_data)?;
    let mut encrypted_data = encrypted_data.to_vec();

    // parse the encrypted message from bytes
    let encrypted_message = EncryptedMessage::parse(&mut encrypted_data)?;

    let server_ecdh_share = ecdh_xy(
        &encrypted_message.get_public_key(),
        &server_key.privkey_fr(),
    );

    Ok(MultiEciesStep2Response {
        server_ecdh_share,
        server_pubkey: server_key.pubkey,
    })
}

#[derive(Debug, Clone)]
pub struct MultiEciesStep3Response {
    pub message: Vec<u8>,
}

pub fn decrypt_bls_interaction_step3(
    client_key: KeySet,
    step1_response: &MultiEciesStep1Response,
    step2_response: &MultiEciesStep2Response,
) -> anyhow::Result<MultiEciesStep3Response> {
    let encrypted_data = decrypt_bls_preprocess(&step1_response.encrypted_data)?;
    let mut encrypted_data = encrypted_data.to_vec();

    // parse the encrypted message from bytes
    let encrypted_message = EncryptedMessage::parse(&mut encrypted_data)?;

    let client_ecdh_share = ecdh_xy(
        &encrypted_message.get_public_key(),
        &client_key.privkey_fr(),
    );

    let aggregated_ecdh_share =
        aggregate_ecdh_x(&[client_ecdh_share, step2_response.server_ecdh_share]);

    // derive keys from the secret key and the encrypted message
    let keys = encrypted_message.derive_keys_with_ecdh(aggregated_ecdh_share);

    // check message integrity and decrypt the message
    let decrypted_data = encrypted_message.check_and_decrypt(keys)?;

    Ok(MultiEciesStep3Response {
        message: decrypted_data.to_vec(),
    })
}

pub fn calc_simple_aggregated_pubkey(signers: &[U256]) -> anyhow::Result<(U256, bool)> {
    let mut aggregated_pubkey = G1Projective::zero();
    for signer in signers {
        let pubkey = G1Affine::recover_from_x((*signer).into());
        aggregated_pubkey += pubkey;
    }

    if aggregated_pubkey.is_zero() {
        return Err(anyhow::anyhow!("Invalid aggregated pubkey"));
    }

    let pubkey: G1Affine = aggregated_pubkey.into();

    Ok((U256::from(pubkey.x), pubkey.y.sgn()))
}

#[cfg(test)]
mod test {
    use intmax2_zkp::{common::signature_content::key_set::KeySet, ethereum_types::u256::U256};

    use crate::data::encryption::bls::v1::multisig::{
        calc_simple_aggregated_pubkey, decrypt_bls_interaction,
    };

    use super::super::algorithm::{encrypt_bls, BytesMut, EciesSender};

    #[test]
    fn test_e2e_encryption_interaction() {
        let mut rand = rand::thread_rng();
        let server_key = KeySet::rand(&mut rand);
        let client_key = KeySet::rand(&mut rand);
        let (pubkey, _) =
            calc_simple_aggregated_pubkey(&[server_key.pubkey, client_key.pubkey]).unwrap();

        let data = b"hello world";
        println!("data: {:?}", data.to_vec());

        let encrypted_data = encrypt_bls(pubkey, data);
        println!("encrypted data: {:?}", encrypted_data);

        let decrypted_data =
            decrypt_bls_interaction(server_key, client_key, &encrypted_data).unwrap();
        println!("decrypted data by multisig: {:?}", decrypted_data);

        assert_eq!(data.to_vec(), decrypted_data);
    }

    fn encrypt_with_unsupported_version(pubkey: U256, data: &[u8]) -> Vec<u8> {
        let sender = EciesSender::new(pubkey);
        let mut encrypted_data = BytesMut::new();
        sender.encrypt_message(data, &mut encrypted_data);

        let version = 2u8;
        let mut version_data = BytesMut::new();
        version_data.extend_from_slice(&version.to_be_bytes());
        version_data.unsplit(encrypted_data);

        version_data.to_vec()
    }

    #[test]
    fn test_e2e_encryption_interaction_with_unsupported_version() {
        let mut rand = rand::thread_rng();
        let server_key = KeySet::rand(&mut rand);
        let client_key = KeySet::rand(&mut rand);
        let (pubkey, _) =
            calc_simple_aggregated_pubkey(&[server_key.pubkey, client_key.pubkey]).unwrap();

        let data = b"hello world";

        let encrypted_data = encrypt_with_unsupported_version(pubkey, data);

        let decrypted_data = decrypt_bls_interaction(server_key, client_key, &encrypted_data);

        match decrypted_data {
            Ok(_) => panic!("Decryption should fail"),
            Err(e) => {
                assert_eq!(e.to_string(), "Unsupported version");
            }
        }
    }
}
