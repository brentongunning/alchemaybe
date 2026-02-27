use crate::combine::{Card, CardKind};
use crate::generator::ImageGenerator;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ImageRequest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: CardKind,
}

#[derive(Serialize)]
pub struct ImageError {
    pub reason: String,
}

pub async fn generate_image<G: ImageGenerator>(
    State(generator): State<Arc<G>>,
    Json(req): Json<ImageRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ImageError>)> {
    let card = Card {
        name: req.name,
        description: req.description,
        kind: req.kind,
    };

    match generator.generate_image(&card).await {
        Ok(bytes) => {
            log::info!("Image generated for '{}'", card.name);
            Ok(([(header::CONTENT_TYPE, "image/png")], bytes))
        }
        Err(reason) => {
            log::error!("Image generation failed for '{}': {reason}", card.name);
            Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ImageError { reason }),
            ))
        }
    }
}
