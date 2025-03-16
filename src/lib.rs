mod deserializer;
mod neon_response;
mod query_builder;
mod request;
mod transaction_builder;

use anyhow::{Context, Result};
pub use neon_response::{QueryResponse, QueryResult, TransactionResponse, TransactionResult};
pub use query_builder::{Query, QueryBuilder};
use request::post;
pub use transaction_builder::{Transaction, TransactionBuilder};

pub struct Client {
    pub(crate) host: String,
    pub(crate) connection_string: String,
    pub client: wstd::http::Client,
}

impl Client {
    /// Returns and error if NEON_CONNECTION_STRING isn't set.
    pub fn new_from_env() -> Result<Self> {
        let connection_string = std::env::var("NEON_CONNECTION_STRING")
            .context("ENV \"NEON_CONNECTION_STRING\" isn't set")?;

        Self::new(&connection_string)
    }

    pub fn new(connection_string: &str) -> Result<Self> {
        let host = connection_string
            .split('@')
            .last()
            .context("Invalid connection string, missing credentials")?;
        let host = host
            .split('/')
            .next()
            .context("Invalid connection string, missing db path")?;

        Ok(Self {
            host: host.to_owned(),
            connection_string: connection_string.to_owned(),
            client: wstd::http::Client::new(),
        })
    }

    /// Execute a SQL query
    pub async fn execute(&self, query: Query) -> Result<()> {
        self.execute_raw(query).await?;
        Ok(())
    }

    /// Execute a SQL query and return the raw response
    pub async fn execute_raw(&self, sql: Query) -> Result<QueryResponse> {
        let url = format!("https://{}/sql", self.host);

        post(self, &url, sql).await
    }

    /// Execute a SQL transaction
    pub async fn execute_transaction(&self, transaction: Transaction) -> Result<()> {
        self.execute_transaction_raw(transaction).await?;
        Ok(())
    }

    /// Execute a SQL transaction and return the raw response
    pub async fn execute_transaction_raw(&self, sql: Transaction) -> Result<TransactionResponse> {
        let url = format!("https://{}/sql", self.host);

        post(self, &url, sql).await
    }
}

#[wstd::test]
pub async fn test() -> Result<()> {
    let client = Client::new("<SOME_CONNECTION_STRING>")?;

    QueryBuilder::new("SELECT * FROM playing_with_neon")
        .execute_raw(&client)
        .await?;

    TransactionBuilder::new()
        .add(QueryBuilder::new("SELECT * FROM playing_with_neon").build())
        .add(QueryBuilder::new("SELECT * FROM playing_with_neon").build())
        .execute_raw(&client)
        .await?;

    Ok(())
}
