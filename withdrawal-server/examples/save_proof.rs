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

    let result = sqlx::query!(
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
    let single_withdrawal_proof = result
        .single_withdrawal_proof
        .expect("single_withdrawal_proof is NULL");

    // save to files
    std::fs::create_dir_all("../job-servers/aggregator-prover/test_data").unwrap();
    std::fs::write(
        "../job-servers/aggregator-prover/test_data/single_withdrawal_proof.bin",
        single_withdrawal_proof,
    )
    .unwrap();
}
