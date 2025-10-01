mod deserializer;
mod neon_response;
pub mod orm;
mod query_builder;
mod request;
mod transaction_builder;

use anyhow::{Context, Result};
pub use neon_response::{QueryResponse, QueryResult, TransactionResponse, TransactionResult};
pub use query_builder::{Query, QueryBuilder};
use request::post;

pub use sql_macro::*;
pub use transaction_builder::{Transaction, TransactionBuilder};

pub struct Client {
    pub(crate) host: String,
    pub(crate) connection_string: String,
    pub(crate) https: bool,
    #[cfg(target_os = "wasi")]
    pub client: wstd::http::Client,
    #[cfg(not(target_os = "wasi"))]
    pub client: reqwest::Client,
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
            client: Default::default(),
            https: true,
        })
    }

    /// Only for temporary tests.
    pub fn new_local(connection_string: &str) -> Result<Self> {
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
            client: Default::default(),
            https: false,
        })
    }

    /// Execute a SQL query
    pub async fn execute(&self, query: Query) -> Result<()> {
        self.execute_raw(query).await?;
        Ok(())
    }

    /// Execute a SQL query and return the raw response
    pub async fn execute_raw(&self, sql: Query) -> Result<QueryResponse> {
        let url = if self.https {
            format!("https://{}/sql", self.host)
        } else {
            format!("http://{}/sql", self.host)
        };

        post(self, &url, sql).await
    }

    /// Execute a SQL transaction
    pub async fn execute_transaction(&self, transaction: Transaction) -> Result<()> {
        self.execute_transaction_raw(transaction).await?;
        Ok(())
    }

    /// Execute a SQL transaction and return the raw response
    pub async fn execute_transaction_raw(&self, sql: Transaction) -> Result<TransactionResponse> {
        let url = if self.https {
            format!("https://{}/sql", self.host)
        } else {
            format!("http://{}/sql", self.host)
        };

        post(self, &url, sql).await
    }

    pub(crate) async fn execute_orm(&self, transaction: serde_json::Value) -> Result<()> {
        self.execute_orm_raw(transaction).await?;
        Ok(())
    }

    pub(crate) async fn execute_orm_raw(
        &self,
        sql: serde_json::Value,
    ) -> Result<TransactionResponse> {
        let url = if self.https {
            format!("https://{}/sql", self.host)
        } else {
            format!("http://{}/sql", self.host)
        };

        post(self, &url, sql).await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, NeonTable, Clone)]
    #[neon_table(table_name = "test", pk = "id", crate_path = "crate")]
    pub struct TestTable {
        pub id: Option<u64>,
        pub name: Option<String>,
        pub description: Option<String>,
        #[neon_table(is_related)]
        pub history: Vec<TestHistory>,
    }

    #[derive(Debug, Serialize, Deserialize, NeonTable, Clone)]
    #[neon_table(table_name = "test_history", pk = "id", crate_path = "crate")]
    pub struct TestHistory {
        pub id: Option<u64>,
        pub test_id: Option<u64>,
        pub state: TestHistoryState,
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(tag = "type", content = "data")]
    pub enum TestHistoryState {
        Active,
        Closed { closed_at: String },
        Value { int_value: i64 },
    }

    #[wstd::test]
    pub async fn test() -> Result<()> {
        let client = Client::new("<CONNECT_STRING>")?;

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

    #[test]
    pub fn test_orm_insert_generation() {
        let item = TestTable {
            id: None,
            name: Some("Test Name".to_string()),
            description: None,
            history: vec![
                TestHistory {
                    id: None,
                    test_id: None,
                    state: TestHistoryState::Active,
                },
                TestHistory {
                    id: None,
                    test_id: None,
                    state: TestHistoryState::Closed {
                        closed_at: "Yesterday".to_string(),
                    },
                },
                TestHistory {
                    id: None,
                    test_id: None,
                    state: TestHistoryState::Value { int_value: 12 },
                },
            ],
        };

        let payload = orm::OrmBuilder::new().insert(item).build();
        let expected_sql = "WITH inserted_parent AS (INSERT INTO test (name) VALUES ($1) RETURNING id) INSERT INTO test_history (test_id, state) VALUES ((SELECT id FROM inserted_parent), $2), ((SELECT id FROM inserted_parent), $3), ((SELECT id FROM inserted_parent), $4)";

        let expected_params = serde_json::json!([
            "Test Name",
            {"type": "Active",},
            {"data":  {"closed_at": "Yesterday",},"type": "Closed",},
            {"data":  {"int_value": 12,},"type": "Value",},
        ]);
        assert_eq!(payload["statements"][0]["query"], expected_sql);
        assert_eq!(payload["statements"][0]["params"], expected_params);
    }

    #[test]
    fn test_orm_update_generation() {
        let item = TestTable {
            id: Some(42),
            name: Some("Updated Test Name".to_string()),
            description: None,
            history: vec![],
        };

        let payload = orm::OrmBuilder::new().update(item).build();

        let expected_sql = "UPDATE test SET name = $1 WHERE id = $2";
        let expected_params = serde_json::json!(["Updated Test Name", 42]);

        let statements = &payload["statements"];
        assert_eq!(statements[0]["query"].as_str().unwrap(), expected_sql);
        assert_eq!(statements[0]["params"], expected_params);
    }

    #[test]
    fn test_orm_delete_generation() {
        let item = TestTable {
            id: Some(42),
            name: None,
            description: None,
            history: vec![],
        };

        let payload = orm::OrmBuilder::new().delete(item).build();

        let expected_sql = "DELETE FROM test WHERE id = $1";
        let expected_params = serde_json::json!([42]);

        let statements = &payload["statements"];
        assert_eq!(statements[0]["query"].as_str().unwrap(), expected_sql);
        assert_eq!(statements[0]["params"], expected_params);
    }
}
