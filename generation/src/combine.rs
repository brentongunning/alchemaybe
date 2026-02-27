use crate::generator::CardGenerator;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    #[default]
    Material,
    Intent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: CardKind,
}

#[derive(Deserialize)]
pub struct CombineRequest {
    pub cards: Vec<Card>,
}

#[derive(Serialize)]
pub struct CombineError {
    pub reason: String,
}

pub async fn combine<G: CardGenerator>(
    State(generator): State<Arc<G>>,
    Json(req): Json<CombineRequest>,
) -> Result<Json<Card>, (StatusCode, Json<CombineError>)> {
    let material_count = req.cards.iter().filter(|c| c.kind == CardKind::Material).count();
    let intent_count = req.cards.iter().filter(|c| c.kind == CardKind::Intent).count();
    if material_count < 1 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(CombineError {
                reason: "At least 1 material card is required".to_string(),
            }),
        ));
    }
    if intent_count > 1 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(CombineError {
                reason: "At most 1 intent card is allowed".to_string(),
            }),
        ));
    }
    match generator.generate(&req.cards).await {
        Ok(card) => Ok(Json(card)),
        Err(reason) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(CombineError { reason }),
        )),
    }
}
