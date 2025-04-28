use serde::{Deserialize, Serialize};

use crate::{Client, Query, TransactionResponse};
use anyhow::Result;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub queries: Vec<Query>,
}

//TODO: Add support for Transaction Options https://github.com/neondatabase/serverless/blob/e4d6b4bde81d56ea7597d7c43c97c3695647fc0e/export/httpQuery.ts#L233
#[derive(Default)]
pub struct TransactionBuilder {
    queries: Vec<Query>,
}

impl TransactionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, query: Query) -> Self {
        self.queries.push(query);
        self
    }

    pub fn merge(mut self, mut transaction: Transaction) -> Self {
        self.queries.append(&mut transaction.queries);
        self
    }

    pub fn build(self) -> Transaction {
        self.into()
    }

    pub async fn execute(self, connection: &Client) -> Result<()> {
        connection.execute_transaction(self.build()).await
    }

    pub async fn execute_raw(self, connection: &Client) -> Result<TransactionResponse> {
        connection.execute_transaction_raw(self.build()).await
    }
}

impl From<TransactionBuilder> for Transaction {
    fn from(value: TransactionBuilder) -> Self {
        Self {
            queries: value.queries,
        }
    }
}
