use crate::card;
use crate::card::CardKind;
use crate::card_cache::{self, CachedCard};
use crate::game_state::HandCard;
use crate::generate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rand::Rng;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

fn require_solana(state: &AppState) -> Result<&crate::solana::SolanaConfig, (StatusCode, Json<ApiError>)> {
    state.solana.as_deref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Solana integration not configured",
        )
    })
}

// --- POST /api/wallet/cards ---

#[derive(Deserialize)]
pub struct WalletCardsRequest {
    pub wallet_address: String,
}

pub async fn wallet_cards(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WalletCardsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;

    log::info!("Querying cards for wallet: {}", req.wallet_address);
    let owned = solana
        .query_owned_cards(&req.wallet_address)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;
    log::info!("Found {} owned cards", owned.len());

    // Enrich with card cache and base card data
    let cache = state.card_cache.read().await;
    let mut cards = Vec::new();
    for card in &owned {
        let base = state.base_cards.iter().find(|b| b.id == card.card_id);
        let cached = cache.get(&card.card_id);
        let (name, description, image_path, kind) = if let Some(b) = base {
            (b.name.as_str(), b.description.as_str(), b.image_path.as_str(), b.kind.as_str())
        } else if let Some(c) = cached {
            (c.name.as_str(), c.description.as_str(), c.image_path.as_str(), "crafted")
        } else {
            (card.name.as_str(), "", "", "crafted")
        };
        cards.push(serde_json::json!({
            "mint_address": card.mint_address,
            "card_id": card.card_id,
            "name": name,
            "description": description,
            "image_path": image_path,
            "kind": kind,
        }));
    }

    Ok(Json(serde_json::json!({ "cards": cards })))
}

fn determine_card_kind(state: &AppState, card_id: &str) -> &'static str {
    if state.base_cards.iter().any(|b| b.id == card_id) {
        let base = state.base_cards.iter().find(|b| b.id == card_id).unwrap();
        if base.kind == "intent" {
            "intent"
        } else {
            "material"
        }
    } else {
        "crafted"
    }
}

// --- POST /api/wallet/claim ---

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub wallet_address: String,
    pub card_id: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub game_id: Option<String>,
}

pub async fn wallet_claim(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClaimRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;
    let recipient = Pubkey::from_str(&req.wallet_address)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid wallet: {e}")))?;

    // Verify the card exists in cache
    let cache = state.card_cache.read().await;
    let cached = cache
        .get(&req.card_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Card not found in cache"))?;

    if cached.impossible {
        return Err(err(StatusCode::BAD_REQUEST, "Cannot claim impossible card"));
    }

    // Ensure metadata JSON exists
    let metadata_uri = solana
        .ensure_metadata_json(&req.card_id, &cached.name, &cached.description, &cached.image_path)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Build mint transaction
    let (tx_base64, asset_pubkey) = solana
        .build_mint_tx(&req.card_id, &cached.name, &metadata_uri, &recipient)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "transaction": tx_base64,
        "asset_address": asset_pubkey,
        "card": {
            "card_id": req.card_id,
            "name": cached.name,
            "description": cached.description,
            "image_path": cached.image_path,
        }
    })))
}

// --- POST /api/wallet/combine ---

#[derive(Deserialize)]
pub struct WalletCombineRequest {
    pub wallet_address: String,
    pub mint_addresses: Vec<String>,
}

pub async fn wallet_combine(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WalletCombineRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;
    let owner = Pubkey::from_str(&req.wallet_address)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid wallet: {e}")))?;

    if req.mint_addresses.len() < 2 || req.mint_addresses.len() > 4 {
        return Err(err(StatusCode::BAD_REQUEST, "Select 2-4 cards to combine"));
    }

    // Verify ownership and get card_ids via DAS
    let owned = solana
        .query_owned_cards(&req.wallet_address)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;

    let mut selected_cards: Vec<(String, String)> = Vec::new(); // (mint, card_id)
    for mint_addr in &req.mint_addresses {
        let owned_card = owned
            .iter()
            .find(|c| c.mint_address == *mint_addr)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("Card {mint_addr} not owned")))?;
        selected_cards.push((mint_addr.clone(), owned_card.card_id.clone()));
    }

    // Look up card details from cache and base cards
    let cache = state.card_cache.read().await;
    let mut hand_cards: Vec<HandCard> = Vec::new();
    for (_mint, card_id) in &selected_cards {
        // Check base cards first
        if let Some(base) = state.base_cards.iter().find(|b| b.id == *card_id) {
            hand_cards.push(HandCard::from_base(base));
        } else if let Some(cached) = cache.get(card_id) {
            hand_cards.push(HandCard {
                name: cached.name.clone(),
                description: cached.description.clone(),
                kind: "crafted".to_string(),
                image_path: cached.image_path.clone(),
                id: cached.id.clone(),
                nft_mint: None,
            });
        } else {
            return Err(err(StatusCode::NOT_FOUND, format!("Card {card_id} not found")));
        }
    }
    drop(cache);

    // Validate combination (same rules as game)
    let material_like_count = hand_cards
        .iter()
        .filter(|c| c.kind == "material" || c.kind == "crafted")
        .count();
    let intent_count = hand_cards.iter().filter(|c| c.kind == "intent").count();
    if material_like_count < 1 {
        return Err(err(StatusCode::BAD_REQUEST, "Need at least 1 material"));
    }
    if intent_count > 1 {
        return Err(err(StatusCode::BAD_REQUEST, "At most 1 intent"));
    }

    // Compute cache key
    let material_ids: Vec<&str> = hand_cards
        .iter()
        .filter(|c| c.kind != "intent")
        .map(|c| c.id.as_str())
        .collect();
    let intent_id = hand_cards
        .iter()
        .find(|c| c.kind == "intent")
        .map(|c| c.id.as_str());
    let key = card_cache::compute_crafted_card_id(&material_ids, intent_id);

    // Check cache
    {
        let cache = state.card_cache.read().await;
        if let Some(cached) = cache.get(&key) {
            if cached.impossible {
                return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "Combination not possible"));
            }

            // Build burn+mint tx
            let metadata_uri = solana
                .ensure_metadata_json(&key, &cached.name, &cached.description, &cached.image_path)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

            let burn_pubkeys: Vec<Pubkey> = req
                .mint_addresses
                .iter()
                .map(|a| Pubkey::from_str(a))
                .collect::<Result<_, _>>()
                .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid mint: {e}")))?;

            let (tx_base64, asset_pubkey) = solana
                .build_burn_and_mint_tx(&burn_pubkeys, &key, &cached.name, &metadata_uri, &owner)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

            return Ok(Json(serde_json::json!({
                "transaction": tx_base64,
                "asset_address": asset_pubkey,
                "card": {
                    "card_id": key,
                    "name": cached.name,
                    "description": cached.description,
                    "image_path": cached.image_path,
                },
                "is_new": !cached.discovered,
            })));
        }
    }

    // Cache miss — call generation server
    let combine_cards: Vec<serde_json::Value> = hand_cards
        .iter()
        .map(|c| {
            let kind = if c.kind == "intent" { "intent" } else { "material" };
            serde_json::json!({
                "name": c.name,
                "description": c.description,
                "kind": kind,
            })
        })
        .collect();

    let combine_resp = state
        .client
        .post(format!("{}/combine", state.generation_url))
        .json(&serde_json::json!({ "cards": combine_cards }))
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Generation error: {e}")))?;

    if !combine_resp.status().is_success() {
        let body = combine_resp.text().await.unwrap_or_default();
        return Err(err(StatusCode::BAD_GATEWAY, format!("Combination failed: {body}")));
    }

    let combined: serde_json::Value = combine_resp
        .json()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Parse error: {e}")))?;

    let card_name = combined["name"].as_str().unwrap_or("Unknown").to_string();
    let card_desc = combined["description"].as_str().unwrap_or("").to_string();

    if card_name.to_lowercase().contains("not possible") {
        let mut cache = state.card_cache.write().await;
        cache.insert(
            key.clone(),
            CachedCard {
                name: "Not possible".to_string(),
                description: String::new(),
                image_path: String::new(),
                id: key,
                discovered: false,
                impossible: true,
            },
        );
        cache.save(std::path::Path::new("cards/card-cache.json"));
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "Combination not possible"));
    }

    // Generate image
    let image_resp = state
        .client
        .post(format!("{}/generate-image", state.generation_url))
        .json(&serde_json::json!({
            "name": card_name,
            "description": card_desc,
        }))
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image error: {e}")))?;

    if !image_resp.status().is_success() {
        return Err(err(StatusCode::BAD_GATEWAY, "Image generation failed"));
    }

    let art_bytes = image_resp
        .bytes()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image read error: {e}")))?;

    let png = card::render_card(&card_name, &art_bytes, &CardKind::Material)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Render error: {e}")))?;

    let safe_name = card_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect::<String>()
        .replace(' ', "-");
    let filename = format!("{safe_name}-{key}.png");
    let disk_path = format!("cards/crafted/{filename}");
    let serve_path = format!("/cards/crafted/{filename}");

    let _ = std::fs::create_dir_all("cards/crafted");
    std::fs::write(&disk_path, &png)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Write error: {e}")))?;

    let cached = CachedCard {
        name: card_name.clone(),
        description: card_desc.clone(),
        image_path: serve_path.clone(),
        id: key.clone(),
        discovered: true,
        impossible: false,
    };

    {
        let mut cache = state.card_cache.write().await;
        cache.insert(key.clone(), cached);
        cache.save(std::path::Path::new("cards/card-cache.json"));
    }

    // Build burn+mint tx
    let metadata_uri = solana
        .ensure_metadata_json(&key, &card_name, &card_desc, &serve_path)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let burn_pubkeys: Vec<Pubkey> = req
        .mint_addresses
        .iter()
        .map(|a| Pubkey::from_str(a))
        .collect::<Result<_, _>>()
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid mint: {e}")))?;

    let (tx_base64, asset_pubkey) = solana
        .build_burn_and_mint_tx(&burn_pubkeys, &key, &card_name, &metadata_uri, &owner)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "transaction": tx_base64,
        "asset_address": asset_pubkey,
        "card": {
            "card_id": key,
            "name": card_name,
            "description": card_desc,
            "image_path": serve_path,
        },
        "is_new": true,
    })))
}

// --- POST /api/wallet/pack/buy ---

#[derive(Deserialize)]
pub struct PackBuyRequest {
    pub wallet_address: String,
    pub pack_type: String, // "starter" or "premium"
}

pub async fn wallet_pack_buy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackBuyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;
    let buyer = Pubkey::from_str(&req.wallet_address)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid wallet: {e}")))?;

    // Pack config
    let (base_count, crafted_count, price_lamports) = match req.pack_type.as_str() {
        "starter" => (2, 1, 10_000_000u64),   // 0.01 SOL
        "premium" => (3, 2, 15_000_000u64),    // 0.015 SOL
        _ => return Err(err(StatusCode::BAD_REQUEST, "Invalid pack type")),
    };

    let mut pack_cards: Vec<(String, String, String)> = Vec::new(); // (card_id, name, metadata_uri)
    let mut pack_display: Vec<serde_json::Value> = Vec::new();

    // Pre-select random indices before any await points (ThreadRng is !Send)
    let base_selections: Vec<usize> = {
        let mut rng = rand::rng();
        (0..base_count)
            .map(|_| rng.random_range(0..state.base_cards.len()))
            .collect()
    };

    // Select random base cards
    for idx in &base_selections {
        let base = &state.base_cards[*idx];
        let metadata_uri = solana
            .ensure_metadata_json(&base.id, &base.name, &base.description, &base.image_path)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        pack_cards.push((base.id.clone(), base.name.clone(), metadata_uri));
        pack_display.push(serde_json::json!({
            "card_id": base.id,
            "name": base.name,
            "description": base.description,
            "image_path": base.image_path,
            "kind": base.kind,
        }));
    }

    // Select random discovered crafted card from cache
    {
        let cache = state.card_cache.read().await;
        let discovered: Vec<CachedCard> = cache
            .all_entries()
            .filter(|(_, c)| c.discovered && !c.impossible && !c.image_path.is_empty())
            .map(|(_, c)| c.clone())
            .collect();

        let crafted_selections: Vec<Option<usize>> = {
            let mut rng = rand::rng();
            (0..crafted_count)
                .map(|_| {
                    if discovered.is_empty() {
                        None
                    } else {
                        Some(rng.random_range(0..discovered.len()))
                    }
                })
                .collect()
        };

        for selection in &crafted_selections {
            if let Some(idx) = selection {
                let crafted = &discovered[*idx];
                let metadata_uri = solana
                    .ensure_metadata_json(
                        &crafted.id,
                        &crafted.name,
                        &crafted.description,
                        &crafted.image_path,
                    )
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
                pack_cards.push((crafted.id.clone(), crafted.name.clone(), metadata_uri));
                pack_display.push(serde_json::json!({
                    "card_id": crafted.id,
                    "name": crafted.name,
                    "description": crafted.description,
                    "image_path": crafted.image_path,
                    "kind": "crafted",
                }));
            } else {
                // No crafted cards available; add another base card
                let fallback_idx = {
                    let mut rng = rand::rng();
                    rng.random_range(0..state.base_cards.len())
                };
                let base = &state.base_cards[fallback_idx];
                let metadata_uri = solana
                    .ensure_metadata_json(&base.id, &base.name, &base.description, &base.image_path)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
                pack_cards.push((base.id.clone(), base.name.clone(), metadata_uri));
                pack_display.push(serde_json::json!({
                    "card_id": base.id,
                    "name": base.name,
                    "description": base.description,
                    "image_path": base.image_path,
                    "kind": base.kind,
                }));
            }
        }
    }

    // Build payment transaction (user signs this one)
    let payment_tx = solana
        .build_payment_tx(price_lamports, &buyer)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "payment_transaction": payment_tx,
        "cards": pack_display,
        "pack_cards": pack_cards.iter().map(|(id, name, uri)| {
            serde_json::json!({"card_id": id, "name": name, "metadata_uri": uri})
        }).collect::<Vec<_>>(),
        "wallet_address": req.wallet_address,
        "price_sol": price_lamports as f64 / 1_000_000_000.0,
    })))
}

// --- POST /api/wallet/pack/confirm ---

#[derive(Deserialize)]
pub struct PackConfirmRequest {
    pub payment_signature: String,
    pub wallet_address: String,
    pub pack_cards: Vec<PackCardInfo>,
}

#[derive(Deserialize)]
pub struct PackCardInfo {
    pub card_id: String,
    pub name: String,
    pub metadata_uri: String,
}

pub async fn wallet_pack_confirm(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackConfirmRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;
    let recipient = Pubkey::from_str(&req.wallet_address)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Invalid wallet: {e}")))?;

    // TODO: optionally verify payment_signature landed on-chain

    // Mint each card server-side
    let mut minted = Vec::new();
    for card in &req.pack_cards {
        let (sig, asset_pubkey) = solana
            .server_mint(&card.card_id, &card.name, &card.metadata_uri, &recipient)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        log::info!("Minted {} -> {} (sig: {})", card.name, asset_pubkey, sig);
        minted.push(serde_json::json!({
            "signature": sig,
            "asset_address": asset_pubkey,
        }));
    }

    Ok(Json(serde_json::json!({
        "minted": minted,
    })))
}

// --- POST /api/wallet/submit-tx ---

#[derive(Deserialize)]
pub struct SubmitTxRequest {
    pub signed_transaction: String,
}

pub async fn wallet_submit_tx(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitTxRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let solana = require_solana(&state)?;

    let signature = solana
        .submit_transaction(&req.signed_transaction)
        .map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;

    Ok(Json(serde_json::json!({
        "signature": signature,
    })))
}
