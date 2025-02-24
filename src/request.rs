use crate::Client;
use anyhow::Result;
use wstd::{
    http::{IntoBody, Method, Request},
    io::AsyncRead,
};

pub async fn post<B, R>(connection: &Client, url: &str, body: B) -> Result<R>
where
    B: serde::Serialize,
    R: serde::de::DeserializeOwned,
{
    let body = serde_json::to_string(&body)?;
    post_raw(connection, url, body.as_bytes()).await
}

pub async fn post_raw<R>(connection: &Client, url: &str, body: &[u8]) -> Result<R>
where
    R: serde::de::DeserializeOwned,
{
    let request = Request::builder()
        .uri(url)
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .header("User-Agent", "neon-http-serverless/0.1.0")
        .header(
            "Neon-Connection-String",
            connection.connection_string.clone(),
        )
        .header("Neon-Raw-Text-Output", "true")
        //TODO: Create a serde deserializer for `QueryResult` to allow Neon-Array-Mode which results in less data being sent over the wire.
        .header("Neon-Array-Mode", "false")
        .header("Content-Length", body.len().to_string())
        .body(body.into_body())?;

    let res = connection.client.send(request).await?;
    let (parts, mut body) = res.into_parts();

    let mut utf8_body = Vec::new();

    body.read_to_end(&mut utf8_body).await?;

    if parts.status != 200 {
        match std::str::from_utf8(&utf8_body) {
            Ok(utf8_body) => anyhow::bail!("Error: {utf8_body}"),
            Err(_) => anyhow::bail!("Error: Unable to convert body to utf8 string"),
        }
    }

    Ok(serde_json::from_slice(&utf8_body)?)
}
