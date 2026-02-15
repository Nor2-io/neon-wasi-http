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

        if let serde_json::Value::Object(object_row) = row {
            if let Some(obj) = object_row.get("jsonb_build_object") {
                if let Some(json_as_string) = obj.as_str() {
                    let item: T = serde_json::from_str(json_as_string).context(
                        "Failed to deserialize the inner JSON string into the target struct",
                    )?;
                    return Ok(Some(item));
                } else {
                    let res =
                        serde_json::from_value(obj.clone()).context("Failed to deserialize row")?;
                    return Ok(Some(res));
                }
            }
        }

        Ok(None)
    }

    pub fn deserialize_multiple<T>(&mut self) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut out: Vec<T> = Vec::new();
        while let Some(row) = self.rows.pop() {
            if let serde_json::Value::Object(object_row) = row {
                if let Some(obj) = object_row.get("jsonb_build_object") {
                    if let Some(json_as_string) = obj.as_str() {
                        let item: T = serde_json::from_str(json_as_string).context(
                            "Failed to deserialize the inner JSON string into the target struct",
                        )?;
                        out.push(item);
                    } else {
                        out.push(serde_json::from_value(obj.clone()).context(
                            "Failed to deserialize raw JSON object (the value was not a string)",
                        )?);
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn deserialize_orm<T>(&mut self) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let Some(row) = self.rows.pop() else {
            return Ok(None);
        };

        if self.row_count != 1 {
            anyhow::bail!("Expected 1 row, got {}", self.row_count);
        }

        if let serde_json::Value::Object(object_row) = row {
            if let Some(obj) = object_row.get("jsonb_build_object") {
                if let Some(json_as_string) = obj.as_str() {
                    let item: T = serde_json::from_str(json_as_string).context(
                        "Failed to deserialize the inner JSON string into the target struct",
                    )?;
                    return Ok(Some(item));
                } else {
                    let res =
                        serde_json::from_value(obj.clone()).context("Failed to deserialize row")?;
                    return Ok(Some(res));
                }
            }
        }

        Ok(None)
    }

    pub fn deserialize_multiple_orm<T>(&mut self) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut out: Vec<T> = Vec::new();
        while let Some(row) = self.rows.pop() {
            if let serde_json::Value::Object(object_row) = row {
                if let Some(obj) = object_row.get("jsonb_build_object") {
                    if let Some(json_as_string) = obj.as_str() {
                        let item: T = serde_json::from_str(json_as_string).context(
                            "Failed to deserialize the inner JSON string into the target struct",
                        )?;
                        out.push(item);
                    } else {
                        out.push(serde_json::from_value(obj.clone()).context(
                            "Failed to deserialize raw JSON object (the value was not a string)",
                        )?);
                    }
                }
            }
        }

        Ok(out)
    }
}
