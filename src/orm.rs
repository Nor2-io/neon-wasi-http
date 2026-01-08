use crate::{Client, QueryResponse, TransactionResponse};
use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub trait NeonTable: Serialize + Sized {
    fn table_name() -> &'static str;

    fn pk_column_name() -> &'static str;

    fn pk_value(&self) -> serde_json::Value;

    fn to_sql_insert_transaction(&self) -> (String, Vec<Value>);

    fn to_sql_update(&self) -> (String, Vec<Value>);

    fn to_sql_delete(&self) -> (String, Vec<Value>);

    fn select_as_json_sql() -> String;

    fn on_conflict_sql() -> &'static str { "" }
}

#[derive(Default, Serialize)]
pub struct OrmBuilder {
    statements: Vec<(String, Vec<serde_json::Value>)>,
}

impl OrmBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: crate::orm::NeonTable>(mut self, item: T) -> Self {
        let (sql, params) = item.to_sql_insert_transaction();
        self.statements.push((sql, params));
        self
    }

    pub fn update<T: crate::orm::NeonTable>(mut self, item: T) -> Self {
        let (sql, params) = item.to_sql_update();
        self.statements.push((sql, params));
        self
    }

    pub fn delete<T: crate::orm::NeonTable>(mut self, item: T) -> Self {
        let (sql, params) = item.to_sql_delete();
        self.statements.push((sql, params));
        self
    }

    pub async fn select<T: crate::orm::NeonTable + DeserializeOwned>(
        client: &Client,
        where_clause: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<T>> {
        if where_clause.is_empty() || !where_clause.trim().to_lowercase().starts_with("where ") {
            return Err(anyhow::anyhow!(
                "Where_clause needs to start with WHERE, `{where_clause}`"
            ));
        }

        //let mut results = Vec::new();
        match client
            .execute_orm_raw_query(serde_json::json!({ "query": format!("{} {where_clause}",T::select_as_json_sql()), "params": params }))
            .await? {
            QueryResponse::Ok(mut query_result) => {
                Ok(query_result.deserialize_multiple_orm()?)
            }
            QueryResponse::Err(neon_error) => Err(neon_error.into()),
        }

        //Ok(results)
    }

    pub async fn execute(self, connection: &Client) -> Result<()> {
        connection.execute_orm(self.build()).await
    }

    pub async fn execute_raw(self, connection: &Client) -> Result<TransactionResponse> {
        connection.execute_orm_raw(self.build()).await
    }

    pub fn build(self) -> serde_json::Value {
        serde_json::json!({
            "queries": self.statements.iter().map(|(sql, params)| {
                serde_json::json!({ "query": sql, "params": params })
            }).collect::<Vec<_>>() // Collect the generated objects into a Vec
        })
    }
}
