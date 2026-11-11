//! JSON-RPC 2.0 响应构造。所有 MCP 交互都从这里出口，保证错误码一致。

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::Response,
};
use serde_json::{json, Value};

pub fn ok(id: Value, result: Value, session: Option<&str>) -> Response {
    build(
        StatusCode::OK,
        json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        session,
    )
}

pub fn err(id: Value, code: i64, message: &str, session: Option<&str>) -> Response {
    build(
        StatusCode::OK,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
        session,
    )
}

pub fn http_err(status: StatusCode, code: i64, message: &str) -> Response {
    build(
        status,
        json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "error": { "code": code, "message": message }
        }),
        None,
    )
}

pub fn accepted() -> Response {
    Response::builder()
        .status(StatusCode::ACCEPTED)
        .body(Body::empty())
        .unwrap_or_else(|_| plain(StatusCode::ACCEPTED))
}

fn plain(status: StatusCode) -> Response {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .expect("empty response")
}

fn build(status: StatusCode, payload: Value, session: Option<&str>) -> Response {
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(sid) = session {
        if let Ok(v) = HeaderValue::from_str(sid) {
            builder = builder.header("mcp-session-id", v);
        }
    }
    builder.body(Body::from(body)).unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR))
}
