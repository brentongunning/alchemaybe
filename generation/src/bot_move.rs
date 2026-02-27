use crate::generator::{
    BotCombineGenerator, BotCombineRequest, BotCombineResult, BotPlaceGenerator, BotPlaceRequest,
    BotPlaceResult,
};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct BotMoveError {
    pub reason: String,
}

pub async fn bot_combine<G: BotCombineGenerator>(
    State(generator): State<Arc<G>>,
    Json(req): Json<BotCombineRequest>,
) -> Result<Json<BotCombineResult>, (StatusCode, Json<BotMoveError>)> {
    log::info!("Computing bot combine...");

    match generator.bot_combine(&req).await {
        Ok(result) => {
            log::info!("Bot chose to combine indices {:?}", result.combine);
            Ok(Json(result))
        }
        Err(reason) => {
            log::error!("Bot combine failed: {reason}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BotMoveError { reason }),
            ))
        }
    }
}

pub async fn bot_place<G: BotPlaceGenerator>(
    State(generator): State<Arc<G>>,
    Json(req): Json<BotPlaceRequest>,
) -> Result<Json<BotPlaceResult>, (StatusCode, Json<BotMoveError>)> {
    log::info!("Computing bot placement...");

    match generator.bot_place(&req).await {
        Ok(result) => {
            if result.skip {
                log::info!("Bot chose to skip placement");
            } else {
                log::info!(
                    "Bot chose to place hand[{}] at ({}, {})",
                    result.hand_index,
                    result.target_row,
                    result.target_col
                );
            }
            Ok(Json(result))
        }
        Err(reason) => {
            log::error!("Bot place failed: {reason}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BotMoveError { reason }),
            ))
        }
    }
}
