use std::{error::Error, fmt::Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionResponse {
    Ok(TransactionResult),
    Err(NeonError),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub results: Vec<QueryResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryResponse {
    Ok(QueryResult),
    Err(NeonError),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub command: String,
    #[serde(alias = "rowCount")]
    pub row_count: i64,
    pub rows: Vec<serde_json::Value>,
    pub fields: Vec<Field>,
    #[serde(alias = "rowAsArray")]
    pub row_as_array: bool,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    name: String,
    #[serde(alias = "dataTypeID")]
    data_type_id: i64,
    #[serde(alias = "tableID")]
    table_id: i64,
    #[serde(alias = "columnID")]
    column_id: i64,
    #[serde(alias = "dataTypeSize")]
    data_type_size: i64,
    #[serde(alias = "dataTypeModifier")]
    data_type_modifier: i64,
    format: String,
}

#[allow(unused)]
#[derive(Deserialize, Serialize, Debug)]
pub struct NeonError {
    pub message: String,
    pub code: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub position: String,
    #[serde(alias = "internalPosition")]
    pub internal_position: Option<String>,
    #[serde(alias = "internalQuery")]
    pub internal_query: Option<String>,
    pub severity: String,
    #[serde(alias = "where")]
    pub location: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
    pub schema: Option<String>,
    pub data_type: Option<String>,
    pub constraint: Option<String>,
    pub file: String,
    pub line: String,
    pub routine: String,
}

impl Display for NeonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:#?}", self))
    }
}

impl Error for NeonError {}
