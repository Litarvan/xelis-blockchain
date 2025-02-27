use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use thiserror::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

const JSON_RPC_VERSION: &str = "2.0";
const PARSE_ERROR_CODE: i16 = -32700;
const INVALID_REQUEST_CODE: i16 = -32600;
const METHOD_NOT_FOUND_CODE: i16 = -32601;
const INVALID_PARAMS_CODE: i16 = -32602;
const INTERNAL_ERROR_CODE: i16 = -32603;

pub type JsonRPCResult<T> = Result<T, JsonRPCError>;

#[derive(Deserialize)]
struct JsonRPCErrorResponse {
    code: i16,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Error)]
pub enum JsonRPCError {
    #[error("Server failed to parse request JSON data")]
    ParseError,
    #[error("Server received invalid JSON-RPC request")]
    InvalidRequest,
    #[error("Unknown method requested to the server")]
    MethodNotFound,
    #[error("Invalid parameters were provided")]
    InvalidParams,
    #[error("Server internal JSON-RPC error: {}", message)]
    InternalError {
        message: String,
        data: Option<String>,
    },
    #[error("Server returned error: [{}] {}", code, message)]
    ServerError {
        code: i16,
        message: String,
        data: Option<String>,
    },
    #[error("Server returned a response without result")]
    MissingResult,
    #[error("Error while (de)serializing JSON data: {}", _0)]
    SerializationError(#[from] serde_json::Error),
    #[error("HTTP error during JSON-RPC communication: {}", _0)]
    HttpError(#[from] reqwest::Error),
}

pub struct JsonRPCClient {
    http: HttpClient,
    target: String,
    count: AtomicUsize,
}

impl JsonRPCClient {
    pub fn new(target: String) -> Self {
        JsonRPCClient {
            http: HttpClient::new(),
            target,
            count: AtomicUsize::new(0),
        }
    }

    pub async fn call<R: DeserializeOwned>(&self, method: &str) -> JsonRPCResult<R> {
        let id = self.count.fetch_add(1, Ordering::SeqCst);
        self.send(json!({
            "jsonrpc": JSON_RPC_VERSION,
            "method": method,
            "id": id
        })).await
    }

    pub async fn call_with<P, R>(&self, method: &str, params: &P) -> JsonRPCResult<R>
        where P: Serialize + Sized, R: DeserializeOwned
    {
        let id = self.count.fetch_add(1, Ordering::SeqCst);
        self.send(json!({
            "jsonrpc": JSON_RPC_VERSION,
            "method": method,
            "id": id,
            "params": params
        })).await
    }

    pub async fn notify(&self, method: &str) -> JsonRPCResult<()> {
        self.http.post(&self.target)
            .json(&json!({
                "jsonrpc": JSON_RPC_VERSION,
                "method": method
            }))
            .send().await?;
        Ok(())
    }

    pub async fn notify_with<P>(&self, method: &str, params: P) -> JsonRPCResult<()>
        where P: Serialize + Sized
    {
        self.http
            .post(&self.target)
            .json(&json!({
                "jsonrpc": JSON_RPC_VERSION,
                "method": method,
                "params": &params
            }))
            .send().await?;
        Ok(())
    }

    pub async fn send<R: DeserializeOwned>(&self, value: Value) -> JsonRPCResult<R> {
        let mut response: Value = self.http.post(&self.target)
            .json(&value)
            .send().await?
            .json().await?;

        if let Some(error) = response.get_mut("error") {
            let error: JsonRPCErrorResponse = serde_json::from_value(error.take())?;
            let data = match error.data {
                Some(content) => Some(serde_json::to_string_pretty(&content)?),
                None => None,
            };

            return Err(match error.code {
                PARSE_ERROR_CODE => JsonRPCError::ParseError,
                INVALID_REQUEST_CODE => JsonRPCError::InvalidRequest,
                METHOD_NOT_FOUND_CODE => JsonRPCError::MethodNotFound,
                INVALID_PARAMS_CODE => JsonRPCError::InvalidParams,
                INTERNAL_ERROR_CODE => JsonRPCError::InternalError {
                    message: error.message.clone(),
                    data,
                },
                code => JsonRPCError::ServerError {
                    code,
                    message: error.message.clone(),
                    data,
                },
            });
        }

        Ok(serde_json::from_value(
            response
                .get_mut("result")
                .ok_or(JsonRPCError::MissingResult)?
                .take(),
        )?)
    }
}