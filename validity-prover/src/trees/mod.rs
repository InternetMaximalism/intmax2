use intmax2_interfaces::utils::random::default_rng;
use rand::Rng as _;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter, Layer as _,
};

pub mod deposit_hash;
pub mod merkle_tree;
pub mod update;

pub fn setup_test() -> String {
    dotenvy::dotenv().ok();

    // Initialize tracing subscriber if not already initialized
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_filter(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into())),
        )
        .try_init();

    std::env::var("DATABASE_URL").unwrap()
}

pub fn generate_random_tag() -> u32 {
    let mut rng = default_rng();
    rng.gen_range(0..1 << 24)
}

pub async fn create_partitions_for_test(
    pool: &sqlx::Pool<sqlx::Postgres>,
    tag: u32,
) -> anyhow::Result<()> {
    // generate hash_nodes partition
    let query =
        format!("CREATE TABLE hash_nodes_tag_{tag} PARTITION OF hash_nodes FOR VALUES IN ({tag})",);
    sqlx::query(&query).execute(pool).await?;

    // generate leaves_len partition
    let query =
        format!("CREATE TABLE leaves_len_tag_{tag} PARTITION OF leaves_len FOR VALUES IN ({tag})",);
    sqlx::query(&query).execute(pool).await?;

    // generate leaves partition
    let query = format!("CREATE TABLE leaves_tag_{tag} PARTITION OF leaves FOR VALUES IN ({tag})");
    sqlx::query(&query).execute(pool).await?;

    // generate indexed_leaves partition
    let query = format!(
        "CREATE TABLE indexed_leaves_tag_{tag} PARTITION OF indexed_leaves FOR VALUES IN ({tag})"
    );
    sqlx::query(&query).execute(pool).await?;
    Ok(())
}
