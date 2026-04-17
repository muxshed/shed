// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use muxshed_common::error::{ApiErrorBody, ApiErrorResponse};
use muxshed_common::MuxshedError;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = ApiErrorResponse {
            error: ApiErrorBody {
                code: self.0.error_code().to_string(),
                message: self.0.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

pub struct ApiError(pub MuxshedError);

impl From<MuxshedError> for ApiError {
    fn from(err: MuxshedError) -> Self {
        Self(err)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self(MuxshedError::Database(err.to_string()))
    }
}
