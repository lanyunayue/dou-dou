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
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(resp: Response) -> Value {
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    #[tokio::test]
    async fn ok_passes_id_result_and_session_header() {
        let resp = ok(json!(42), json!({"hello": "world"}), Some("sess-1"));
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("mcp-session-id").unwrap(), "sess-1");
        let v = body_json(resp).await;
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 42);
        assert_eq!(v["result"]["hello"], "world");
    }

    #[tokio::test]
    async fn err_wraps_code_message_keeps_id() {
        let resp = err(json!(7), -32000, "boom", None);
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("mcp-session-id").is_none());
        let v = body_json(resp).await;
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["error"]["code"], -32000);
        assert_eq!(v["error"]["message"], "boom");
    }

    #[tokio::test]
    async fn http_err_uses_given_status_and_null_id() {
        let resp = http_err(StatusCode::BAD_REQUEST, -32602, "invalid params");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert_eq!(v["id"], Value::Null);
        assert_eq!(v["error"]["code"], -32602);
        assert_eq!(v["error"]["message"], "invalid params");
    }

    #[tokio::test]
    async fn accepted_is_empty_202() {
        let resp = accepted();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn invalid_session_id_header_is_omitted() {
        // 含换行等非法字符时不写入头，避免构造 panic。
        let resp = ok(json!(1), json!({}), Some("bad\r\nsid"));
        assert!(resp.headers().get("mcp-session-id").is_none());
    }
}
