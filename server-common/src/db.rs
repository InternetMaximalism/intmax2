use futures_core::Stream;
use sqlx::{
    pool::PoolConnection,
    postgres::{PgPoolOptions, PgQueryResult, PgRow, PgStatement, PgTypeInfo},
    Describe, Either, Execute, Executor, PgPool, Postgres, Transaction,
};
use std::{future::Future, pin::Pin, time::Duration};
use tracing::instrument;

// NOTE: separate PgPool and TracingPool with feature-flag in future work
pub type DbPool = TracingPool;

#[derive(Clone)]
pub struct DbPoolConfig {
    pub url: String,
    pub max_connections: u32,
    pub idle_timeout: u64,
}

impl DbPool {
    pub async fn from_config(config: &DbPoolConfig) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .idle_timeout(Duration::from_secs(config.idle_timeout))
            .connect(&config.url)
            .await?;

        Ok(TracingPool::new(pool))
    }
}

#[derive(Clone, Debug)]
pub struct TracingPool {
    pool: PgPool,
}

impl TracingPool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type QueryStream<'e> =
    Pin<Box<dyn Stream<Item = Result<Either<PgQueryResult, PgRow>, sqlx::Error>> + Send + 'e>>;

type DescribeFuture<'e, DB> =
    Pin<Box<dyn Future<Output = Result<Describe<DB>, sqlx::Error>> + Send + 'e>>;

impl<'c> Executor<'c> for &'c TracingPool {
    type Database = Postgres;

    #[instrument(name = "sql", fields(query = %query.sql()),, skip(self, query))]
    fn execute<'e, 'q: 'e, Q>(
        self,
        query: Q,
    ) -> Pin<Box<dyn Future<Output = Result<PgQueryResult, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
        Q: Send + 'q + sqlx::Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.execute(query))
    }

    #[instrument(name = "sql", fields(query = %query.sql()),, skip(self, query))]
    fn fetch_many<'e, 'q: 'e, E>(self, query: E) -> QueryStream<'e>
    where
        'c: 'e,
        E: 'q + Send + Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.fetch_many(query))
    }

    #[instrument(name = "sql", fields(query = %query.sql()),, skip(self, query))]
    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PgRow>, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
        E: 'q + Send + Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.fetch_optional(query))
    }

    #[instrument(name = "sql_prepare", skip(self, sql, parameters))]
    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [PgTypeInfo],
    ) -> Pin<Box<dyn Future<Output = Result<PgStatement<'q>, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
    {
        Box::pin(self.pool.prepare_with(sql, parameters))
    }

    #[instrument(name = "sql_describe", skip(self, sql))]
    fn describe<'e, 'q: 'e>(self, sql: &'q str) -> DescribeFuture<'e, Self::Database>
    where
        'c: 'e,
    {
        Box::pin(self.pool.describe(sql))
    }
}

impl<'c> Executor<'c> for TracingPool {
    type Database = Postgres;

    #[instrument(name = "sql", fields(query = %query.sql()),, skip(self, query))]
    fn execute<'e, 'q: 'e, Q>(
        self,
        query: Q,
    ) -> Pin<Box<dyn Future<Output = Result<PgQueryResult, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
        Q: Send + 'q + sqlx::Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.execute(query))
    }

    #[instrument(name = "sql", fields(query = %query.sql()),, skip(self, query))]
    fn fetch_many<'e, 'q: 'e, E>(self, query: E) -> QueryStream<'e>
    where
        'c: 'e,
        E: 'q + Send + Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.fetch_many(query))
    }

    #[instrument(name = "sql", fields(query = %query.sql()), skip(self, query))]
    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PgRow>, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
        E: 'q + Send + Execute<'q, Self::Database>,
    {
        let operation = sql_summary(&query);
        let _span = tracing::debug_span!(
            "db",
            "operation" = operation,
            "otel.name" = %format!("DB: {}", operation),
             "otel.kind" = "client",
        );
        Box::pin(self.pool.fetch_optional(query))
    }

    #[instrument(name = "sql_prepare", skip(self, sql, parameters))]
    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [PgTypeInfo],
    ) -> Pin<Box<dyn Future<Output = Result<PgStatement<'q>, sqlx::Error>> + Send + 'e>>
    where
        'c: 'e,
    {
        Box::pin(self.pool.prepare_with(sql, parameters))
    }

    #[instrument(name = "sql_describe", skip(self, sql))]
    fn describe<'e, 'q: 'e>(self, sql: &'q str) -> DescribeFuture<'e, Self::Database>
    where
        'c: 'e,
    {
        Box::pin(self.pool.describe(sql))
    }
}

impl TracingPool {
    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>, sqlx::Error> {
        self.pool.acquire().await
    }
}

fn sql_summary<'q, Q: sqlx::Execute<'q, Postgres>>(query: &Q) -> String {
    let sql = query.sql();
    let first_word = sql
        .split_whitespace()
        .next()
        .unwrap_or("UNKNOWN")
        .to_uppercase();

    match first_word.as_str() {
        "SELECT" => format!("SELECT FROM {}", get_table_name(sql)),
        "INSERT" => format!("INSERT INTO {}", get_table_name(sql)),
        "UPDATE" => format!("UPDATE {}", get_table_name(sql)),
        "DELETE" => format!("DELETE FROM {}", get_table_name(sql)),
        _ => first_word,
    }
}

fn get_table_name(sql: &str) -> &str {
    sql.split_whitespace()
        .skip_while(|w| !matches!(*w, "FROM" | "INTO" | "UPDATE"))
        .nth(1)
        .unwrap_or("unknown_table")
}
