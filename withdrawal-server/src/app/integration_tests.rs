use alloy::primitives::Address;
use intmax2_zkp::ethereum_types::u256::U256;
use serde_json::json;
use std::{str::FromStr, thread::sleep, time::Duration};

use crate::{
    app::{
        status::{SqlClaimStatus, SqlWithdrawalStatus},
        test_helpers::{
            assert_and_stop, create_databases, find_free_port, get_provider, run_withdrawal_docker,
            setup_migration, stop_withdrawal_docker,
        },
        withdrawal_server::WithdrawalServer,
    },
    Env,
};
use intmax2_interfaces::api::{
    store_vault_server::types::CursorOrder, withdrawal_server::types::TimestampCursor,
};
use intmax2_zkp::ethereum_types::u32limb_trait::U32LimbTrait;

fn get_example_env() -> Env {
    Env {
        port: 9003,
        database_url: "postgres://postgres:password@localhost:5432/withdrawal".to_string(),
        database_max_connections: 10,
        database_timeout: 10,

        store_vault_server_base_url: "http://localhost:9000".to_string(),
        use_s3: Some(true),
        validity_prover_base_url: "http://localhost:9002".to_string(),

        l2_rpc_url: "http://127.0.0.1:8545".to_string(),
        rollup_contract_address: Address::from_str(
            "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512",
        )
        .unwrap(),
        withdrawal_contract_address: Address::from_str(
            "0x8a791620dd6260079bf849dc5567adc3f2fdc318",
        )
        .unwrap(),
        is_faster_mining: true,
        withdrawal_beneficiary_view_pair:"viewpair/0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447/0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447".parse().unwrap(),
        claim_beneficiary_view_pair: "viewpair/0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447/0x1a1ef1bc29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447".parse().unwrap(),
        direct_withdrawal_fee: Some("0:100".parse().unwrap()),
        claimable_withdrawal_fee: Some("0:10".parse().unwrap()),
        claim_fee: Some("0:100".parse().unwrap()),
    }
}

#[tokio::test]
async fn test_getting_fee() {
    // We use a port different from the default one (5432)
    let port = find_free_port();
    let cont_name = "withdrawal-test-getting-fee";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    // 2.5 seconds should be enough for postgres container to be started to create databases
    sleep(Duration::from_millis(2500));
    assert_and_stop(cont_name, || create_databases(cont_name));

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    // Test get_claim_fee and get_withdrawal_fee
    {
        // Here and later I use is_some() || is_some() and not && as an additional check of initializing WithdrawalServer.
        // If only one variable is Some and another one is not, test will fail, so there is should be some error in WithdrawalServer new method.
        let claim_fee = server.get_claim_fee();
        if env.claim_fee.is_some() {
            let fee = env.claim_fee.unwrap().0;
            assert_and_stop(cont_name, || assert_eq!(claim_fee.fee.unwrap(), fee));
        }
        let withdrawal_fee = server.get_withdrawal_fee();
        if withdrawal_fee.direct_withdrawal_fee.is_some() {
            assert_and_stop(cont_name, || {
                assert_eq!(withdrawal_fee.direct_withdrawal_fee.unwrap().len(), 1)
            });
        }
    }

    // Test inserting and checking withdrawal and claim tables for needed hash
    {
        let pubkey_str =
            U256::from_hex("0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447")
                .unwrap();
        let recipient_str = "0xabc";
        let withdrawal_hash = "0xdeadbeef";
        let proof_bytes = vec![1u8, 2, 3, 4]; // Replace with actual proof if needed
        let claim_value = json!({
            "recipient": recipient_str,
            "amount": "1000",
            "token_index": 1,
            "block_number": 42,
            "block_hash": "0xblockhash",
            "nullifier": withdrawal_hash
        });

        // Check claims table for some withdrawal_hash record
        let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
                SELECT EXISTS(
                    SELECT 1 FROM withdrawals WHERE withdrawal_hash = $1
                )
                "#,
        )
        .bind(withdrawal_hash)
        .fetch_one(&server.pool)
        .await
        .expect("Failed to check existence of withdrawal_hash in claims table");

        assert_and_stop(cont_name, || {
            assert!(!exists.0, "Claim should not contain withdrawal_hash")
        });

        sqlx::query(
            r#"
                INSERT INTO withdrawals (
                    pubkey,
                    recipient,
                    withdrawal_hash,
                    single_withdrawal_proof,
                    contract_withdrawal,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
                "#,
        )
        .bind(pubkey_str.to_hex())
        .bind(recipient_str)
        .bind(withdrawal_hash)
        .bind(&proof_bytes)
        .bind(&claim_value)
        .bind(SqlWithdrawalStatus::Requested as SqlWithdrawalStatus)
        .execute(&server.pool)
        .await
        .expect("Failed to insert record into withdrawals table");

        let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
                SELECT EXISTS(
                    SELECT 1 FROM withdrawals WHERE withdrawal_hash = $1
                )
                "#,
        )
        .bind(withdrawal_hash)
        .fetch_one(&server.pool)
        .await
        .expect("Failed to check existence of withdrawal_hash in withdrawals table");

        assert_and_stop(cont_name, || {
            assert!(
                exists.0,
                "Withdrawals should contain withdrawal_hash after insertion"
            )
        });

        // Check claims table for some nullifier record
        let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
                SELECT EXISTS(
                    SELECT 1 FROM claims WHERE nullifier = $1
                )
                "#,
        )
        .bind(withdrawal_hash)
        .fetch_one(&server.pool)
        .await
        .expect("Failed to check existence of nullifier in claims table");

        assert_and_stop(cont_name, || {
            assert!(!exists.0, "Claim should not contain nullifier")
        });

        sqlx::query(
            r#"
                INSERT INTO claims (
                    pubkey,
                    recipient,
                    nullifier,
                    single_claim_proof,
                    claim,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::claim_status)
                "#,
        )
        .bind(pubkey_str.to_hex())
        .bind(recipient_str)
        .bind(withdrawal_hash)
        .bind(&proof_bytes)
        .bind(&claim_value)
        .bind(SqlClaimStatus::Requested as SqlClaimStatus)
        .execute(&server.pool)
        .await
        .expect("Failed to insert claim into database");

        let exists: (bool,) = sqlx::query_as::<_, (bool,)>(
            r#"
                SELECT EXISTS(
                    SELECT 1 FROM claims WHERE nullifier = $1
                )
                "#,
        )
        .bind(withdrawal_hash)
        .fetch_one(&server.pool)
        .await
        .expect("Failed to check existence of nullifier in claims table");

        assert_and_stop(cont_name, || {
            assert!(exists.0, "Claim should contain nullifier after insertion")
        });
    }

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_withdrawal_info_with_data() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-info-data";

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

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let pubkey =
        U256::from_hex("0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447")
            .unwrap();

    // Insert test data
    let withdrawal_hashes = [
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        "0x2345678901bcdef12345678901bcdef12345678901bcdef12345678901bcdef1",
        "0x3456789012cdef123456789012cdef123456789012cdef123456789012cdef12",
    ];
    let recipients = [
        "0x1234567890123456789012345678901234567890",
        "0x2345678901234567890123456789012345678901",
        "0x3456789012345678901234567890123456789012",
    ];
    let proof_bytes = vec![1u8, 2, 3, 4];

    for (i, (hash, recipient)) in withdrawal_hashes.iter().zip(recipients.iter()).enumerate() {
        let contract_withdrawal = json!({
            "recipient": recipient,
            "tokenIndex": i as u32,
            "amount": (1000 * (i + 1)).to_string(),
            "nullifier": hash
        });
        sqlx::query!(
            r#"
                INSERT INTO withdrawals (
                    pubkey,
                    recipient,
                    withdrawal_hash,
                    single_withdrawal_proof,
                    contract_withdrawal,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
                "#,
            pubkey.to_hex(),
            recipient,
            hash,
            proof_bytes,
            contract_withdrawal,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test withdrawal");
    }

    // Test with default limit
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: None,
    };

    let result = server.get_withdrawal_info(pubkey, cursor).await;
    assert!(result.is_ok(), "get_withdrawal_info should succeed");
    let (withdrawal_infos, cursor_response) = result.unwrap();
    assert_eq!(withdrawal_infos.len(), 3, "Should have 3 withdrawals");
    assert_eq!(cursor_response.total_count, 3, "Total count should be 3");
    assert!(!cursor_response.has_more, "Should not have more results");

    // Verify the data is correct
    assert_eq!(withdrawal_infos[0].contract_withdrawal.token_index, 2);
    assert_eq!(withdrawal_infos[1].contract_withdrawal.token_index, 1);
    assert_eq!(withdrawal_infos[2].contract_withdrawal.token_index, 0);

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_withdrawal_info_with_pagination() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-info-pagination";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    create_databases(cont_name);

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let pubkey =
        U256::from_hex("0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447")
            .unwrap();

    // Insert 5 test withdrawals
    let proof_bytes = vec![1u8, 2, 3, 4];
    let base_hashes = [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555555555555555555555555555",
    ];
    let base_recipients = [
        "0x1111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555",
    ];

    for i in 0..5 {
        let hash = base_hashes[i];
        let recipient = base_recipients[i];
        let contract_withdrawal = json!({
            "recipient": recipient,
            "tokenIndex": i as u32,
            "amount": (1000 * (i + 1)).to_string(),
            "nullifier": hash
        });

        sqlx::query!(
            r#"
                INSERT INTO withdrawals (
                    pubkey,
                    recipient,
                    withdrawal_hash,
                    single_withdrawal_proof,
                    contract_withdrawal,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
                "#,
            pubkey.to_hex(),
            recipient,
            hash,
            proof_bytes,
            contract_withdrawal,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test withdrawal");

        sleep(Duration::from_millis(10));
    }

    // Test with limit of 2
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: Some(2),
    };

    let result = server.get_withdrawal_info(pubkey, cursor).await;
    assert!(result.is_ok(), "get_withdrawal_info should succeed");
    let (withdrawal_infos, cursor_response) = result.unwrap();

    assert_eq!(
        withdrawal_infos.len(),
        2,
        "Should have 2 withdrawals due to limit"
    );
    assert_eq!(cursor_response.total_count, 5, "Total count should be 5");
    assert!(cursor_response.has_more, "Should have more results");
    assert!(
        cursor_response.next_cursor.is_some(),
        "Next cursor should be set"
    );

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_claim_info_with_data() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-claim-info-data";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    create_databases(cont_name);

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let pubkey =
        U256::from_hex("0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447")
            .unwrap();

    // Insert test claim data
    let nullifiers = [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333333333333333333333333333",
    ];
    let recipients = [
        "0x1234567890123456789012345678901234567890",
        "0x2345678901234567890123456789012345678901",
        "0x3456789012345678901234567890123456789012",
    ];
    let proof_bytes = vec![1u8, 2, 3, 4];

    for (i, (nullifier, recipient)) in nullifiers.iter().zip(recipients.iter()).enumerate() {
        let claim_value = json!({
            "recipient": recipient,
            "amount": (1000 * (i + 1)).to_string(),
            "blockNumber": 42 + i as u32,
            "blockHash": format!("0x{:064x}", (i + 1) as u64 * 0x1111111111111111u64),
            "nullifier": nullifier
        });

        sqlx::query!(
            r#"
                INSERT INTO claims (
                    pubkey,
                    recipient,
                    nullifier,
                    single_claim_proof,
                    claim,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::claim_status)
                "#,
            pubkey.to_hex(),
            recipient,
            nullifier,
            proof_bytes,
            claim_value,
            SqlClaimStatus::Requested as SqlClaimStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test claim");
    }

    // Test with default limit
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: None,
    };

    let result = server.get_claim_info(pubkey, cursor).await;
    assert!(result.is_ok(), "get_claim_info should succeed");
    let (claim_infos, cursor_response) = result.unwrap();
    assert_eq!(claim_infos.len(), 3, "Should have 3 claims");
    assert_eq!(cursor_response.total_count, 3, "Total count should be 3");
    assert!(!cursor_response.has_more, "Should not have more results");

    // Verify the data is correct (newest first due to DESC order)
    assert_eq!(claim_infos[0].claim.block_number, 44); // 42 + 2
    assert_eq!(claim_infos[1].claim.block_number, 43); // 42 + 1
    assert_eq!(claim_infos[2].claim.block_number, 42); // 42 + 0

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_claim_info_with_pagination() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-claim-info-pagination";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    create_databases(cont_name);

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let pubkey =
        U256::from_hex("0xdeadbeef29051c687773b8751961827400215d295e4ee2ef8754c7f831a3b447")
            .unwrap();

    // Insert 5 test claims
    let proof_bytes = vec![1u8, 2, 3, 4];
    let base_nullifiers = [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555555555555555555555555555",
    ];
    let base_recipients = [
        "0x1111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555",
    ];

    for i in 0..5 {
        let nullifier = base_nullifiers[i];
        let recipient = base_recipients[i];
        let claim_value = json!({
            "recipient": recipient,
            "amount": (1000 * (i + 1)).to_string(),
            "blockNumber": 42 + i as u32,
            "blockHash": format!("0x{:064x}", (i + 1) as u64 * 0x1111111111111111u64),
            "nullifier": nullifier
        });

        sqlx::query!(
            r#"
                INSERT INTO claims (
                    pubkey,
                    recipient,
                    nullifier,
                    single_claim_proof,
                    claim,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::claim_status)
                "#,
            pubkey.to_hex(),
            recipient,
            nullifier,
            proof_bytes,
            claim_value,
            SqlClaimStatus::Requested as SqlClaimStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test claim");

        sleep(Duration::from_millis(10));
    }

    // Test with limit of 2
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: Some(2),
    };

    let result = server.get_claim_info(pubkey, cursor).await;
    assert!(result.is_ok(), "get_claim_info should succeed");
    let (claim_infos, cursor_response) = result.unwrap();

    assert_eq!(claim_infos.len(), 2, "Should have 2 claims due to limit");
    assert_eq!(cursor_response.total_count, 5, "Total count should be 5");
    assert!(cursor_response.has_more, "Should have more results");
    assert!(
        cursor_response.next_cursor.is_some(),
        "Next cursor should be set"
    );

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_withdrawal_info_by_recipient_with_data() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-info-by-recipient-data";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    create_databases(cont_name);

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let target_recipient = intmax2_zkp::ethereum_types::address::Address::from_hex(
        "0x1234567890123456789012345678901234567890",
    )
    .unwrap();
    let other_recipient = intmax2_zkp::ethereum_types::address::Address::from_hex(
        "0x9876543210987654321098765432109876543210",
    )
    .unwrap();

    // Insert test data for target recipient (3 withdrawals)
    let withdrawal_hashes = [
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        "0x2345678901bcdef12345678901bcdef12345678901bcdef12345678901bcdef1",
        "0x3456789012cdef123456789012cdef123456789012cdef123456789012cdef12",
    ];
    let pubkeys = [
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ];
    let proof_bytes = vec![1u8, 2, 3, 4];

    for (i, (hash, pubkey)) in withdrawal_hashes.iter().zip(pubkeys.iter()).enumerate() {
        let contract_withdrawal = json!({
            "recipient": target_recipient.to_hex(),
            "tokenIndex": i as u32,
            "amount": (1000 * (i + 1)).to_string(),
            "nullifier": hash
        });

        sqlx::query!(
            r#"
                INSERT INTO withdrawals (
                    pubkey,
                    recipient,
                    withdrawal_hash,
                    single_withdrawal_proof,
                    contract_withdrawal,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
                "#,
            pubkey,
            target_recipient.to_hex(),
            hash,
            proof_bytes,
            contract_withdrawal,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test withdrawal");
    }

    // Insert 1 withdrawal for other recipient to verify filtering
    let other_contract_withdrawal = json!({
        "recipient": other_recipient.to_hex(),
        "tokenIndex": 99u32,
        "amount": "999999",
        "nullifier": "0x9999999999999999999999999999999999999999999999999999999999999999"
    });

    sqlx::query!(
        r#"
            INSERT INTO withdrawals (
                pubkey,
                recipient,
                withdrawal_hash,
                single_withdrawal_proof,
                contract_withdrawal,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
            "#,
        "0xdddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        other_recipient.to_hex(),
        "0x9999999999999999999999999999999999999999999999999999999999999999",
        proof_bytes,
        other_contract_withdrawal,
        SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
    )
    .execute(&server.pool)
    .await
    .expect("Failed to insert other recipient withdrawal");

    // Test with default limit for target recipient
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: None,
    };

    let result = server
        .get_withdrawal_info_by_recipient(target_recipient, cursor)
        .await;
    assert!(
        result.is_ok(),
        "get_withdrawal_info_by_recipient should succeed"
    );
    let (withdrawal_infos, cursor_response) = result.unwrap();
    assert_eq!(
        withdrawal_infos.len(),
        3,
        "Should have 3 withdrawals for target recipient"
    );
    assert_eq!(cursor_response.total_count, 3, "Total count should be 3");
    assert!(!cursor_response.has_more, "Should not have more results");

    // Verify the data is correct and filtered by recipient
    for withdrawal_info in &withdrawal_infos {
        assert_eq!(
            withdrawal_info.contract_withdrawal.recipient,
            target_recipient
        );
    }

    // Verify order (newest first due to DESC order)
    assert_eq!(withdrawal_infos[0].contract_withdrawal.token_index, 2);
    assert_eq!(withdrawal_infos[1].contract_withdrawal.token_index, 1);
    assert_eq!(withdrawal_infos[2].contract_withdrawal.token_index, 0);

    stop_withdrawal_docker(cont_name);
}

#[tokio::test]
async fn test_get_withdrawal_info_by_recipient_with_pagination() {
    let port = find_free_port();
    let cont_name = "withdrawal-test-get-info-by-recipient-pagination";

    stop_withdrawal_docker(cont_name);
    let output = run_withdrawal_docker(port, cont_name);
    assert!(
        output.status.success(),
        "Couldn't start {}: {}",
        cont_name,
        String::from_utf8_lossy(&output.stderr)
    );

    sleep(Duration::from_millis(2500));
    create_databases(cont_name);

    let mut env = get_example_env();
    env.database_url =
        format!("postgres://postgres:password@localhost:{port}/withdrawal").to_string();
    let server = WithdrawalServer::new(&env, get_provider()).await;

    if let Err(err) = &server {
        stop_withdrawal_docker(cont_name);
        panic!("Withdrawal Server initialization failed: {err:?}");
    }
    let server = server.unwrap();

    setup_migration(&server.pool).await;

    let target_recipient = intmax2_zkp::ethereum_types::address::Address::from_hex(
        "0x1234567890123456789012345678901234567890",
    )
    .unwrap();

    // Insert 5 test withdrawals for target recipient
    let proof_bytes = vec![1u8, 2, 3, 4];
    let base_hashes = [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555555555555555555555555555",
    ];
    let base_pubkeys = [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333333333333333333333333333",
        "0x4444444444444444444444444444444444444444444444444444444444444444",
        "0x5555555555555555555555555555555555555555555555555555555555555555",
    ];

    for i in 0..5 {
        let hash = base_hashes[i];
        let pubkey = base_pubkeys[i];
        let contract_withdrawal = json!({
            "recipient": target_recipient.to_hex(),
            "tokenIndex": i as u32,
            "amount": (1000 * (i + 1)).to_string(),
            "nullifier": hash
        });

        sqlx::query!(
            r#"
                INSERT INTO withdrawals (
                    pubkey,
                    recipient,
                    withdrawal_hash,
                    single_withdrawal_proof,
                    contract_withdrawal,
                    status
                )
                VALUES ($1, $2, $3, $4, $5, $6::withdrawal_status)
                "#,
            pubkey,
            target_recipient.to_hex(),
            hash,
            proof_bytes,
            contract_withdrawal,
            SqlWithdrawalStatus::Requested as SqlWithdrawalStatus
        )
        .execute(&server.pool)
        .await
        .expect("Failed to insert test withdrawal");

        sleep(Duration::from_millis(10));
    }

    // Test with limit of 2
    let cursor = TimestampCursor {
        cursor: None,
        order: CursorOrder::Desc,
        limit: Some(2),
    };

    let result = server
        .get_withdrawal_info_by_recipient(target_recipient, cursor)
        .await;
    assert!(
        result.is_ok(),
        "get_withdrawal_info_by_recipient should succeed"
    );
    let (withdrawal_infos, cursor_response) = result.unwrap();

    assert_eq!(
        withdrawal_infos.len(),
        2,
        "Should have 2 withdrawals due to limit"
    );
    assert_eq!(cursor_response.total_count, 5, "Total count should be 5");
    assert!(cursor_response.has_more, "Should have more results");
    assert!(
        cursor_response.next_cursor.is_some(),
        "Next cursor should be set"
    );

    // Verify all results are for the correct recipient
    for withdrawal_info in &withdrawal_infos {
        assert_eq!(
            withdrawal_info.contract_withdrawal.recipient,
            target_recipient
        );
    }

    stop_withdrawal_docker(cont_name);
}
