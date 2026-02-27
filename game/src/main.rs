mod card;
mod card_cache;
mod game_api;
mod game_state;
mod generate;
mod solana;
mod solana_api;

use axum::routing::{get, post};
use axum::{Json, Router};
use card_cache::CardCache;
use generate::AppState;
use game_state::build_base_cards;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

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

    let generation_url =
        std::env::var("GENERATION_URL").expect("GENERATION_URL env var is required");
    log::info!("Using generation server at {generation_url}");

    // Load cards.json
    let cards_data = std::fs::read_to_string("cards.json").expect("Failed to read cards.json");
    let cards_json: serde_json::Value =
        serde_json::from_str(&cards_data).expect("Failed to parse cards.json");
    let base_cards = build_base_cards(&cards_json);
    log::info!("Loaded {} base cards", base_cards.len());

    // Load categories.json
    let cats_data =
        std::fs::read_to_string("categories.json").expect("Failed to read categories.json");
    let categories: Vec<String> =
        serde_json::from_str(&cats_data).expect("Failed to parse categories.json");
    log::info!("Loaded {} categories", categories.len());

    // Load card cache
    let card_cache = CardCache::load(std::path::Path::new("cards/card-cache.json"));

    // Load Solana config
    let solana_config = solana::SolanaConfig::from_env().map(std::sync::Arc::new);
    if solana_config.is_some() {
        log::info!("Solana integration enabled");
    } else {
        log::info!("Solana integration not configured (set SOLANA_KEYPAIR_PATH, SOLANA_RPC_URL, HELIUS_API_KEY, COLLECTION_ADDRESS to enable)");
    }

    let state = Arc::new(AppState {
        generation_url,
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .expect("failed to build HTTP client"),
        games: RwLock::new(HashMap::new()),
        card_cache: RwLock::new(card_cache),
        base_cards,
        categories,
        solana: solana_config,
    });

    let app = Router::new()
        .route("/status", get(status))
        .route("/generate-card", post(generate::generate_card))
        .route("/api/cards", get(game_api::list_cards))
        .route("/api/game/new", post(game_api::new_game))
        .route("/api/game/{id}", get(game_api::get_game))
        .route("/api/game/{id}/combine", post(game_api::combine))
        .route("/api/game/{id}/finalize-combine", post(game_api::finalize_combine))
        .route("/api/game/{id}/place", post(game_api::place))
        .route("/api/game/{id}/discard", post(game_api::discard))
        .route("/api/game/{id}/end-turn", post(game_api::end_turn))
        .route("/api/game/{id}/bot-combine", post(game_api::bot_combine))
        .route("/api/game/{id}/bot-place", post(game_api::bot_place))
        // Solana wallet endpoints
        .route("/api/wallet/cards", post(solana_api::wallet_cards))
        .route("/api/wallet/claim", post(solana_api::wallet_claim))
        .route("/api/wallet/combine", post(solana_api::wallet_combine))
        .route("/api/wallet/pack/buy", post(solana_api::wallet_pack_buy))
        .route("/api/wallet/pack/confirm", post(solana_api::wallet_pack_confirm))
        .route("/api/wallet/submit-tx", post(solana_api::wallet_submit_tx))
        .nest_service("/cards", ServeDir::new("cards"))
        .fallback_service(ServeDir::new("game/static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    log::info!("Game server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
