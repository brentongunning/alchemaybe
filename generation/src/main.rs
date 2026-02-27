mod bot_move;
mod combine;
mod generator;
mod image;
mod judge;
mod ollama;

use axum::routing::{get, post};
use axum::{Json, Router};
use ollama::{OllamaConfig, OllamaGenerator};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
struct Status {
    status: &'static str,
}

async fn status() -> Json<Status> {
    Json(Status { status: "ok" })
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = OllamaConfig::from_env();
    let generator = Arc::new(OllamaGenerator::new(config));

    let app = Router::new()
        .route("/status", get(status))
        .route("/combine", post(combine::combine::<OllamaGenerator>))
        .route(
            "/generate-image",
            post(image::generate_image::<OllamaGenerator>),
        )
        .route("/judge", post(judge::judge::<OllamaGenerator>))
        .route(
            "/bot-combine",
            post(bot_move::bot_combine::<OllamaGenerator>),
        )
        .route(
            "/bot-place",
            post(bot_move::bot_place::<OllamaGenerator>),
        )
        .with_state(generator);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    log::info!("Generation server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
