use crate::server::{health, prover};
use actix_web::web;

pub fn setup_routes(cfg: &mut web::ServiceConfig) {
    cfg.service((health::health_check,));
    cfg.service((
        prover::withdrawal::get_proof,
        prover::withdrawal::get_proofs,
        prover::withdrawal::generate_proof,
    ));
    cfg.service((
        prover::wrapper::get_proof,
        prover::wrapper::get_proofs,
        prover::wrapper::generate_proof,
    ));
}
