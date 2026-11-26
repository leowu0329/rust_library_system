use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("資料庫操作失敗: {0}")]
    Database(#[from] mongodb::error::Error),

    #[error("無效的資料識別碼格式")]
    InvalidObjectId,

    #[error("權限不足: {0}")]
    Forbidden(String),

    #[error("業務邏輯錯誤: {0}")]
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::InvalidObjectId | AppError::BadRequest(_) => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
        };

        (status, message).into_response()
    }
}
