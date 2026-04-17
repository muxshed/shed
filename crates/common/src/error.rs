// Licensed under the Business Source License 1.1 — see LICENSE.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum MuxshedError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("pipeline error: {0}")]
    Pipeline(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl MuxshedError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Pipeline(_) => "PIPELINE_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn status_code(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            Self::BadRequest(_) => 400,
            Self::Unauthorized(_) => 401,
            Self::Pipeline(_) => 500,
            Self::Database(_) => 500,
            Self::Internal(_) => 500,
        }
    }
}
