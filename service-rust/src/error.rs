use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

pub type ApiResult = Result<Json<Value>, AppError>;

#[derive(Debug)]
pub struct AppError {
    pub code: i32,
    pub message: String,
}

impl AppError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(1400, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // The frontend handles business errors separately from transport failures.
        Json(json!({"code":self.code,"msg":self.message})).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error = %error, "database operation failed");
        Self::new(1200, "Database operation failed")
    }
}
impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        tracing::error!(error = %error, "file operation failed");
        Self::new(1300, "File operation failed")
    }
}
impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        tracing::debug!(error = %error, "remote request failed");
        Self::bad_request("Remote request failed")
    }
}

pub fn ok(data: impl Serialize) -> Json<Value> {
    Json(json!({"code":0,"msg":"OK","data":data}))
}
pub fn success() -> Json<Value> {
    Json(json!({"code":0,"msg":"OK"}))
}
pub fn list(data: impl Serialize, count: i64) -> Json<Value> {
    ok(json!({"list":data,"count":count}))
}
