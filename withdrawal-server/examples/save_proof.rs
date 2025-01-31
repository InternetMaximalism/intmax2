use base64::{prelude::BASE64_STANDARD, Engine as _};
use server_common::db::{DbPool, DbPoolConfig};
use withdrawal_server::Env;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let env = envy::from_env::<Env>().unwrap();
    let pool = DbPool::from_config(&DbPoolConfig {
        max_connections: env.database_max_connections,
        idle_timeout: env.database_timeout,
        url: env.database_url.to_string(),
    })
    .await
    .unwrap();

    let withdrawal_result = sqlx::query!(
        r#"
    SELECT 
        uuid,
        single_withdrawal_proof,
        created_at
    FROM withdrawals
    WHERE single_withdrawal_proof IS NOT NULL
    LIMIT 1
    "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let single_withdrawal_proof = withdrawal_result
        .single_withdrawal_proof
        .expect("single_withdrawal_proof is NULL");
    let single_withdrawal_proof_base64 = BASE64_STANDARD.encode(single_withdrawal_proof);

    let claim_result = sqlx::query!(
        r#"
    SELECT 
        uuid,
        single_claim_proof,
        created_at
    FROM claims
    WHERE single_claim_proof IS NOT NULL
    LIMIT 1
    "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let single_claim_proof = claim_result
        .single_claim_proof
        .expect("single_claim_proof is NULL");

    let single_claim_proof_base64 = BASE64_STANDARD.encode(single_claim_proof);

    // save to files
    std::fs::create_dir_all("../job-servers/aggregator-prover/test_data").unwrap();
    std::fs::write(
        "../job-servers/aggregator-prover/test_data/single_withdrawal_proof.txt",
        single_withdrawal_proof_base64,
    )
    .unwrap();
    std::fs::write(
        "../job-servers/aggregator-prover/test_data/single_claim_proof.txt",
        single_claim_proof_base64,
    )
    .unwrap();
}
