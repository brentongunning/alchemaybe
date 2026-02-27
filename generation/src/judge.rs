use crate::generator::{JudgeGenerator, JudgeRequest, JudgeResult};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct JudgeError {
    pub reason: String,
}

pub async fn judge<G: JudgeGenerator>(
    State(generator): State<Arc<G>>,
    Json(req): Json<JudgeRequest>,
) -> Result<Json<JudgeResult>, (StatusCode, Json<JudgeError>)> {
    log::info!(
        "Judging '{}' vs '{}' for category '{}'",
        req.card_a.name,
        req.card_b.name,
        req.category
    );

    match generator.judge(&req).await {
        Ok(result) => {
            log::info!(
                "Judge result: {} wins — {}",
                result.winner,
                result.reason
            );
            Ok(Json(result))
        }
        Err(reason) => {
            log::error!("Judge failed: {reason}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JudgeError { reason }),
            ))
        }
    }
}
