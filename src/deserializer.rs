use crate::QueryResult;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

impl QueryResult {
    pub fn deserialize<T>(&mut self) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let Some(row) = self.rows.pop() else {
            return Ok(None);
        };

        if self.row_count != 1 {
            anyhow::bail!("Expected 1 row, got {}", self.row_count);
        }

        let res = serde_json::from_value(row).context("Failed to deserialize row")?;

        Ok(Some(res))
    }

    pub fn deserialize_multiple<T>(&mut self) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut out: Vec<T> = Vec::new();
        while let Some(row) = self.rows.pop() {
            out.push(serde_json::from_value(row).context("Failed to deserialize row")?);
        }

        Ok(out)
    }
}
