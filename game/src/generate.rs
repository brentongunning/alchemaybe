use crate::card::{self, CardKind};
use crate::card_cache::CardCache;
use crate::game_state::{BaseCard, GameState};
use crate::solana::SolanaConfig;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub generation_url: String,
    pub client: reqwest::Client,
    pub games: RwLock<HashMap<String, GameState>>,
    pub card_cache: RwLock<CardCache>,
    pub base_cards: Vec<BaseCard>,
    pub categories: Vec<String>,
    pub solana: Option<Arc<SolanaConfig>>,
}

#[derive(Deserialize)]
pub struct CardRequest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: CardKind,
}

#[derive(Serialize)]
pub struct CardError {
    reason: String,
}

pub async fn generate_card(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CardRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<CardError>)> {
    log::info!("Generating card '{}'", req.name);

    // Call generation server for art
    let gen_url = format!("{}/generate-image", state.generation_url);
    let art_bytes = state
        .client
        .post(&gen_url)
        .json(&serde_json::json!({
            "name": req.name,
            "description": req.description,
            "kind": if req.kind == CardKind::Intent { "intent" } else { "material" },
        }))
        .send()
        .await
        .map_err(|e| {
            log::error!("Generation server request failed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(CardError {
                    reason: format!("generation server error: {e}"),
                }),
            )
        })?
        .error_for_status()
        .map_err(|e| {
            log::error!("Generation server returned error: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(CardError {
                    reason: format!("generation server error: {e}"),
                }),
            )
        })?
        .bytes()
        .await
        .map_err(|e| {
            log::error!("Failed to read generation response: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(CardError {
                    reason: format!("generation server error: {e}"),
                }),
            )
        })?;

    // Render the card
    let png = card::render_card(&req.name, &art_bytes, &req.kind).map_err(|e| {
        log::error!("Card rendering failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CardError { reason: e }),
        )
    })?;

    log::info!("Card '{}' rendered ({} bytes)", req.name, png.len());
    Ok(([(header::CONTENT_TYPE, "image/png")], png))
}
