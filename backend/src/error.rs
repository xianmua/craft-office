use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;

use crate::guard::GuardError;

#[derive(Debug)]
pub enum ApiError {
    Guard(GuardError),
    TooLarge,
    Upstream(String),
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl ApiError {
    fn code(&self) -> &'static str {
        match self {
            ApiError::Guard(_) => "invalid_target",
            ApiError::TooLarge => "payload_too_large",
            ApiError::Upstream(_) => "upstream_error",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

impl From<GuardError> for ApiError {
    fn from(e: GuardError) -> Self {
        ApiError::Guard(e)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Guard(e) => write!(f, "{e}"),
            ApiError::TooLarge => write!(f, "文件超过大小上限"),
            ApiError::Upstream(e) => write!(f, "{e}"),
            ApiError::Internal(e) => write!(f, "{e}"),
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Guard(_) => StatusCode::BAD_REQUEST,
            ApiError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            code: self.code(),
            message: self.to_string(),
        })
    }
}
