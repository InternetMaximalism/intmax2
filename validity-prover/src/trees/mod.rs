use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter, Layer as _,
};

pub mod deposit_hash;
pub mod merkle_tree;
pub mod update;
pub mod utils;

pub fn setup_test() -> String {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_filter(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into())),
        )
        .try_init()
        .unwrap();
    std::env::var("DATABASE_URL").unwrap()
}
