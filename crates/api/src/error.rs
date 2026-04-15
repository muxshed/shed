// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
