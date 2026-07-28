use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub(crate) enum ServerError {
    BadRequest(String),
    Conflict(String),
    Forbidden,
    Internal(anyhow::Error),
    NotFound,
    ServiceUnavailable(String),
    Unauthorized,
}

impl From<anyhow::Error> for ServerError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(detail) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": detail })),
            )
                .into_response(),
            Self::Conflict(detail) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": detail })),
            )
                .into_response(),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "administrator role required" })),
            )
                .into_response(),
            Self::Internal(error) => {
                eprintln!("server error: {error:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "internal server error",
                    })),
                )
                    .into_response()
            }
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "resource not found" })),
            )
                .into_response(),
            Self::ServiceUnavailable(detail) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": detail })),
            )
                .into_response(),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "authentication required",
                })),
            )
                .into_response(),
        }
    }
}
