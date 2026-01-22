use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{Client, QueryResponse};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub query: String,
    pub params: Vec<serde_json::Value>,
}

pub struct QueryBuilder {
    query: String,
    params: Vec<serde_json::Value>,
}

impl QueryBuilder {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
            params: Vec::new(),
        }
    }

    pub fn set_sql(mut self, sql: &str) -> Self {
        self.query = sql.to_string();
        self
    }

    pub fn bind<T: Serialize>(mut self, value: T) -> Self {
        let val = serde_json::to_value(&value).unwrap();
        if !val.is_null() {
            if let serde_json::Value::Object(_) | serde_json::Value::Array(_) = &val {
                let json_string = serde_json::to_string(&val).unwrap();
                self.params.push(serde_json::Value::String(json_string));
            } else {
                self.params.push(val);
            }
        } else {
            self.params.push(serde_json::Value::Null);
        }

        self
    }

    pub fn build(self) -> Query {
        self.into()
    }

    pub async fn execute(self, connection: &Client) -> Result<()> {
        connection.execute(self.build()).await
    }

    pub async fn execute_raw(self, connection: &Client, is_select: bool) -> Result<QueryResponse> {
        connection.execute_raw(self.build(), is_select).await
    }

    pub async fn fetch_one<T>(self, conn: &Client) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.execute_raw(conn, true).await? {
            QueryResponse::Ok(mut query_response) => Ok(query_response.deserialize()?),
            QueryResponse::Err(neon_error) => bail!(neon_error),
        }
    }

    pub async fn fetch_all<T>(self, conn: &Client) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        match self.execute_raw(conn, true).await? {
            QueryResponse::Ok(mut query_response) => Ok(query_response.deserialize_multiple()?),
            QueryResponse::Err(neon_error) => bail!(neon_error),
        }
    }
}

impl From<QueryBuilder> for Query {
    fn from(value: QueryBuilder) -> Self {
        Self {
            query: value.query,
            params: value.params,
        }
    }
}
