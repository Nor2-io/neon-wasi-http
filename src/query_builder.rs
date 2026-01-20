use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{Client, QueryResponse};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub query: String,
    pub params: Vec<String>,
}

pub struct QueryBuilder {
    query: String,
    params: Vec<String>,
}

impl QueryBuilder {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
            params: Vec::new(),
        }
    }

    pub fn bind<T: ToString>(mut self, value: T) -> Self {
        self.params.push(value.to_string());
        self
    }

    pub fn build(self) -> Query {
        self.into()
    }

    pub async fn execute(self, connection: &Client) -> Result<()> {
        connection.execute(self.build()).await
    }

    pub async fn execute_raw(self, connection: &Client) -> Result<QueryResponse> {
        connection.execute_raw(self.build()).await
    }

    pub async fn fetch_one<T>(self, conn: &Client) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        match self.execute_raw(conn).await? {
            QueryResponse::Ok(mut query_response) => Ok(query_response.deserialize()?),
            QueryResponse::Err(neon_error) => bail!(neon_error),
        }
    }

    pub async fn fetch_all<T>(self, conn: &Client) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        match self.execute_raw(conn).await? {
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
