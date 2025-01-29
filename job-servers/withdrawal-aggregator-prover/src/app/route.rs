use actix_web::web;

use crate::server::{health, prover};

pub fn setup_routes(cfg: &mut web::ServiceConfig) {
    cfg.service((health::health_check,));
    cfg.service((
        prover::withdrawal::get_proof,
        prover::withdrawal::generate_proof,
    ));
    cfg.service((prover::wrapper::get_proof, prover::wrapper::generate_proof));
}
