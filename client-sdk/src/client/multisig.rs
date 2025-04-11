use ark_bn254::G2Affine;
use ark_ec::CurveGroup;
use intmax2_interfaces::data::encryption::bls::v1::multisig::calc_simple_aggregated_pubkey;
use intmax2_zkp::{
    common::signature::{
        key_set::KeySet,
        sign_tools::{sign_message, verify_signature},
    },
    ethereum_types::u256::U256,
};
use std::ops::Add;

#[derive(Debug, Clone)]
pub struct Step1Response {
    pub client_pubkey: U256,
    pub message: Vec<u8>,
}

pub fn multi_signature_interaction_step1(client_key: KeySet, message: &[u8]) -> Step1Response {
    Step1Response {
        client_pubkey: client_key.pubkey,
        message: message.to_vec(),
    }
}

#[derive(Clone, Copy)]
pub struct Step2Response {
    pub server_signature: G2Affine,
    pub server_pubkey: U256,
}

pub fn multi_signature_interaction_step2(
    server_key: KeySet,
    step1_response: &Step1Response,
) -> Step2Response {
    let server_signature: G2Affine = sign_message(server_key.privkey, &step1_response.message);

    Step2Response {
        server_signature,
        server_pubkey: server_key.pubkey,
    }
}

#[derive(Clone, Copy)]
pub struct Step3Response {
    pub aggregated_signature: G2Affine,
    pub aggregated_pubkey: U256,
}

pub fn multi_signature_interaction_step3(
    client_key: KeySet,
    step1_response: &Step1Response,
    step2_response: &Step2Response,
) -> anyhow::Result<Step3Response> {
    verify_signature(
        step2_response.server_signature,
        step2_response.server_pubkey,
        &step1_response.message,
    )?;

    let signers = vec![client_key.pubkey, step2_response.server_pubkey];
    let (aggregated_pubkey, y_parity) = calc_simple_aggregated_pubkey(&signers)?;

    let client_signature = sign_message(client_key.privkey, &step1_response.message);

    let mut aggregated_signature = client_signature
        .add(step2_response.server_signature)
        .into_affine();
    if y_parity {
        aggregated_signature.y = -aggregated_signature.y;
    }

    assert!(aggregated_signature.is_on_curve());

    verify_signature(
        aggregated_signature,
        aggregated_pubkey,
        &step1_response.message,
    )?;

    Ok(Step3Response {
        aggregated_signature,
        aggregated_pubkey,
    })
}

#[cfg(test)]
mod test {
    use intmax2_zkp::common::signature::{key_set::KeySet, sign_tools::verify_signature};

    use crate::client::multisig::{
        multi_signature_interaction_step1, multi_signature_interaction_step2,
        multi_signature_interaction_step3,
    };

    #[test]
    fn test_multi_signature_interaction() {
        let mut rng = rand::thread_rng();
        let client_key = KeySet::rand(&mut rng);
        let server_key = KeySet::rand(&mut rng);

        let message = b"Hello, world!";
        let step1_response = multi_signature_interaction_step1(client_key, message); // client side
        let step2_response = multi_signature_interaction_step2(server_key, &step1_response); // server side
        let step3_response =
            multi_signature_interaction_step3(client_key, &step1_response, &step2_response)
                .unwrap(); // client side

        println!("aggregated_pubkey: {:?}", step3_response.aggregated_pubkey);
        println!(
            "aggregated_signature: {:?}",
            step3_response.aggregated_signature
        );

        // Verify the signature
        verify_signature(
            step3_response.aggregated_signature,
            step3_response.aggregated_pubkey,
            &message[..],
        )
        .unwrap();
    }
}
