use std::{fs, io::Read, panic, sync::Arc, thread::sleep, time::Duration};
// For redis
use std::{
    net::TcpListener,
    process::{Command, Output, Stdio},
};

use alloy::providers::{mock::Asserter, ProviderBuilder};
use dotenvy::dotenv;
use intmax2_client_sdk::external_api::contract::utils::NormalProvider;
use server_common::db::DbPool;
use sqlx::query;

use crate::{
    app::{validator::MockBlockHashValidator, withdrawal_server::WithdrawalServer},
    Env,
};

pub fn run_withdrawal_docker(port: u16, container_name: &str) -> Output {
    let port_arg = format!("{port}:5432");

    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            container_name,
            "--hostname",
            "--postgres",
            "-e",
            "POSTGRES_USER=postgres",
            "-e",
            "POSTGRES_PASSWORD=password",
            "-e",
            "POSTGRES_DB=maindb",
            "-p",
            &port_arg,
            "postgres:16.6",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Error during Redis container startup");

    output
}

pub fn create_databases(container_name: &str) {
    let commands = ["CREATE DATABASE event;", "CREATE DATABASE withdrawal;"];

    for sql_cmd in commands {
        let status = Command::new("docker")
            .args([
                "exec",
                "-i", // No TTY needed; `-it` is for interactive terminal; `-i` is enough here
                container_name,
                "psql",
                "-U",
                "postgres",
                "-d",
                "maindb",
                "-c",
                sql_cmd,
            ])
            .status()
            .expect("Failed to execute docker exec");

        assert!(status.success(), "Couldn't run {sql_cmd}");
    }
}

pub async fn setup_migration(pool: &DbPool) {
    create_tables(pool, "./migrations/20250523164255_initial.up.sql").await;
    create_tables(pool, "./migrations/20250624100406_delete-uuid.up.sql").await;
}

pub async fn create_tables(pool: &DbPool, file_path: &str) {
    // Open and read file
    let mut file =
        fs::File::open(file_path).unwrap_or_else(|e| panic!("Failed to open SQL file: {e}"));
    let mut sql_content = String::new();
    file.read_to_string(&mut sql_content)
        .unwrap_or_else(|e| panic!("Failed to read SQL file: {e}"));

    // Execute the SQL content
    for statement in sql_content.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            query(trimmed)
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("Failed to execute SQL: {e}"));
        }
    }
}

pub fn stop_withdrawal_docker(container_name: &str) -> Output {
    let output = Command::new("docker")
        .args(["stop", container_name])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Error during Redis container stopping");

    output
}

pub fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to address")
        .local_addr()
        .unwrap()
        .port()
}

pub fn assert_and_stop<F: FnOnce() + panic::UnwindSafe>(cont_name: &str, f: F) {
    let res = panic::catch_unwind(f);

    if let Err(panic_info) = res {
        stop_withdrawal_docker(cont_name);
        panic::resume_unwind(panic_info);
    }
}

pub fn get_provider() -> NormalProvider {
    let provider_asserter = Asserter::new();
    ProviderBuilder::default()
        .with_gas_estimation()
        .with_simple_nonce_management()
        .fetch_chain_id()
        .connect_mocked_client(provider_asserter)
}

pub async fn start_mock_withdrawal_server(cont_name: &str) -> anyhow::Result<WithdrawalServer> {
    let port = find_free_port();

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    assert_and_stop(cont_name, || create_databases(cont_name));

    dotenv().ok();
    let mut env: Env = envy::from_env().expect("Failed to parse env");
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let validator = Arc::new(MockBlockHashValidator);
    let server = WithdrawalServer::new_with_validator(&env, get_provider(), validator).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    Ok(server)
}
