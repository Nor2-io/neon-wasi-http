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
    pub(crate) connection_string: String,
    pub(crate) url: String,
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
        let protocol = if host == "db.localtest.me:4444" {
            "http".to_string()
        } else {
            "https".to_string()
        };

        Ok(Self {
            connection_string: connection_string.to_owned(),
            client: Default::default(),
            url: format!("{protocol}://{host}/sql"),
        })
    }

    /// Execute a SQL query
    pub async fn execute(&self, query: Query) -> Result<()> {
        self.execute_raw(query).await?;
        Ok(())
    }

    /// Execute a SQL query and return the raw response
    pub async fn execute_raw(&self, sql: Query) -> Result<QueryResponse> {
        post(self, &self.url, sql).await
    }

    /// Execute a SQL transaction
    pub async fn execute_transaction(&self, transaction: Transaction) -> Result<()> {
        self.execute_transaction_raw(transaction).await?;
        Ok(())
    }

    /// Execute a SQL transaction and return the raw response
    pub async fn execute_transaction_raw(&self, sql: Transaction) -> Result<TransactionResponse> {
        post(self, &self.url, sql).await
    }

    pub(crate) async fn execute_orm(&self, transaction: serde_json::Value) -> Result<()> {
        self.execute_orm_raw(transaction).await?;
        Ok(())
    }

    pub(crate) async fn execute_orm_raw(
        &self,
        sql: serde_json::Value,
    ) -> Result<TransactionResponse> {
        post(self, &self.url, sql).await
    }

    pub(crate) async fn execute_orm_raw_query(
        &self,
        sql: serde_json::Value,
    ) -> Result<QueryResponse> {
        post(self, &self.url, sql).await
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
        #[neon_table(is_related)]
        pub data: Option<TestData>,
    }

    #[derive(Debug, Serialize, Deserialize, NeonTable, Clone)]
    #[neon_table(table_name = "test_history", pk = "id", crate_path = "crate")]
    pub struct TestHistory {
        pub id: Option<u64>,
        pub test_id: Option<u64>,
        pub state: TestHistoryState,
    }

    #[derive(Debug, Serialize, Deserialize, NeonTable, Clone)]
    #[neon_table(
        table_name = "test_data",
        pk = "id",
        crate_path = "crate",
        on_conflict = "state"
    )]
    pub struct TestData {
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
            data: Some(TestData {
                id: None,
                test_id: None,
                state: TestHistoryState::Active,
            }),
        };

        let payload = orm::OrmBuilder::new().insert(item).build();

        println!("Payload-Query: => {}", payload["queries"][0]["query"]);
        println!("Payload-params: => {}", payload["queries"][0]["params"]);

        let expected_sql = "WITH inserted_parent AS (INSERT INTO test (name) VALUES ($1) RETURNING id) , child_history_0_0 AS (INSERT INTO test_history (test_id, state) VALUES ((SELECT id FROM inserted_parent), $2)  RETURNING id) , child_history_1_0 AS (INSERT INTO test_history (test_id, state) VALUES ((SELECT id FROM inserted_parent), $3)  RETURNING id) , child_history_2_0 AS (INSERT INTO test_history (test_id, state) VALUES ((SELECT id FROM inserted_parent), $4)  RETURNING id) , child_data_0_0 AS (INSERT INTO test_data (test_id, state) VALUES ((SELECT id FROM inserted_parent), $5)  ON CONFLICT (state) DO NOTHING RETURNING id) SELECT id FROM inserted_parent";

        let expected_params = serde_json::json!([
            "Test Name",
            "{\"type\":\"Active\"}",
            "{\"data\":{\"closed_at\":\"Yesterday\"},\"type\":\"Closed\"}",
            "{\"data\":{\"int_value\":12},\"type\":\"Value\"}",
            "{\"type\":\"Active\"}",
        ]);

        assert_eq!(payload["queries"][0]["query"], expected_sql);
        assert_eq!(payload["queries"][0]["params"], expected_params);
    }

    #[test]
    pub fn test_orm_insert_with_empty_child_generation() {
        let item = TestTable {
            id: None,
            name: Some("Test Name".to_string()),
            description: None,
            history: Vec::new(),
            data: None,
        };

        let payload = orm::OrmBuilder::new().insert(item).build();

        println!("Payload-Query: => {}", payload["queries"][0]["query"]);
        println!("Payload-params: => {}", payload["queries"][0]["params"]);

        let expected_sql = "INSERT INTO test (name) VALUES ($1)";

        let expected_params = serde_json::json!(["Test Name"]);

        assert_eq!(payload["queries"][0]["query"], expected_sql);
        assert_eq!(payload["queries"][0]["params"], expected_params);
    }

    #[test]
    fn test_orm_update_generation() {
        let item = TestTable {
            id: Some(42),
            name: Some("Updated Test Name".to_string()),
            description: None,
            history: vec![],
            data: None,
        };

        let payload = orm::OrmBuilder::new().update(item).build();

        let expected_sql = "UPDATE test SET name = $1 WHERE id = $2";
        let expected_params = serde_json::json!(["Updated Test Name", 42]);

        let queries = &payload["queries"];
        assert_eq!(queries[0]["query"].as_str().unwrap(), expected_sql);
        assert_eq!(queries[0]["params"], expected_params);
    }

    #[test]
    fn test_orm_delete_generation() {
        use crate::orm::NeonTable;
        let apa = TestTable::select_as_json_sql();
        println!("{apa}");
    }
}
