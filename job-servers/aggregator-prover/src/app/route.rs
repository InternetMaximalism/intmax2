use actix_web::web;

use crate::server::{
    health,
    prover::{claim, claim_wrapper, withdrawal, withdrawal_wrapper},
};

pub fn setup_routes(cfg: &mut web::ServiceConfig) {
    cfg.service((health::health_check,));
    cfg.service((
        withdrawal::get_proof,
        withdrawal::generate_proof,
        claim::get_proof,
        claim::generate_proof,
    ));
    cfg.service((
        withdrawal_wrapper::get_proof,
        withdrawal_wrapper::generate_proof,
        claim_wrapper::get_proof,
        claim_wrapper::generate_proof,
    ));
}
