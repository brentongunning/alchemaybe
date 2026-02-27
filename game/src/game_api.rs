use crate::card;
use crate::card::CardKind;
use crate::card_cache::{self, CachedCard};
use crate::game_state::{CraftedCard, GameMode, GamePhase, GameState, HandCard, PlacedCard};
use crate::generate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct NftCardSelection {
    pub mint_address: String,
    pub card_id: String,
}

#[derive(Deserialize)]
pub struct NewGameRequest {
    pub mode: GameMode,
    #[serde(default)]
    pub wallet_address: Option<String>,
    #[serde(default)]
    pub nft_cards: Vec<NftCardSelection>,
}

#[derive(Deserialize)]
pub struct CombineRequest {
    pub card_indices: Vec<usize>,
    #[serde(default)]
    pub async_image: bool,
}

#[derive(Deserialize)]
pub struct FinalizeCombineRequest {
    pub cache_key: String,
    pub name: String,
    pub description: String,
}

#[derive(Deserialize)]
pub struct PlaceRequest {
    pub hand_index: usize,
    pub row: usize,
    pub col: usize,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

pub async fn list_cards(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "cards": state.base_cards,
    }))
}

pub async fn new_game(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NewGameRequest>,
) -> Result<Json<GameState>, (StatusCode, Json<ApiError>)> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut game = GameState::new(id.clone(), req.mode, &state.categories, &state.base_cards);

    // If player has NFT cards selected, verify and add them to hand
    if !req.nft_cards.is_empty() {
        if req.nft_cards.len() > 4 {
            return Err(err(StatusCode::BAD_REQUEST, "Max 4 NFT cards"));
        }

        // Verify ownership if Solana is configured
        if let (Some(wallet), Some(solana)) = (&req.wallet_address, state.solana.as_deref()) {
            let owned = solana
                .query_owned_cards(wallet)
                .await
                .map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;

            for nft in &req.nft_cards {
                if !owned.iter().any(|o| o.mint_address == nft.mint_address && o.card_id == nft.card_id) {
                    return Err(err(
                        StatusCode::BAD_REQUEST,
                        format!("NFT {} not owned by wallet", nft.mint_address),
                    ));
                }
            }
        }

        // Build HandCards from NFT selections
        let cache = state.card_cache.read().await;
        let mut nft_hand_cards = Vec::new();
        for nft in &req.nft_cards {
            // Check base cards first
            if let Some(base) = state.base_cards.iter().find(|b| b.id == nft.card_id) {
                let mut hc = HandCard::from_base(base);
                hc.nft_mint = Some(nft.mint_address.clone());
                nft_hand_cards.push(hc);
            } else if let Some(cached) = cache.get(&nft.card_id) {
                nft_hand_cards.push(HandCard {
                    name: cached.name.clone(),
                    description: cached.description.clone(),
                    kind: "crafted".to_string(),
                    image_path: cached.image_path.clone(),
                    id: cached.id.clone(),
                    nft_mint: Some(nft.mint_address.clone()),
                });
            }
        }

        // Replace the first N cards in hand with NFT cards
        let replace_count = nft_hand_cards.len().min(game.players[0].hand.len());
        for (i, nft_card) in nft_hand_cards.into_iter().enumerate() {
            if i < replace_count {
                game.players[0].hand[i] = nft_card;
            }
        }
    }

    // Set wallet on player state
    if let Some(wallet) = req.wallet_address {
        game.players[0].wallet = Some(wallet);
    }

    state.games.write().await.insert(id, game.clone());
    Ok(Json(game))
}

pub async fn get_game(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<GameState>, (StatusCode, Json<ApiError>)> {
    let games = state.games.read().await;
    match games.get(&id) {
        Some(game) => Ok(Json(game.clone())),
        None => Err(err(StatusCode::NOT_FOUND, "Game not found")),
    }
}

pub async fn combine(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CombineRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let (game, player_idx) = {
        let games = state.games.read().await;
        let game = games
            .get(&id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;
        if game.phase == GamePhase::GameOver {
            return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
        }
        (game.clone(), game.current_player)
    };

    let hand = &game.players[player_idx].hand;

    // Validate indices
    if req.card_indices.len() < 2 || req.card_indices.len() > 4 {
        return Err(err(StatusCode::BAD_REQUEST, "Select 2-4 cards to combine"));
    }
    for &idx in &req.card_indices {
        if idx >= hand.len() {
            return Err(err(StatusCode::BAD_REQUEST, "Invalid card index"));
        }
    }

    // Collect selected cards
    let selected: Vec<_> = req.card_indices.iter().map(|&i| &hand[i]).collect();

    // Materials and crafted cards count as "material-like" for combination
    let material_like_count = selected
        .iter()
        .filter(|c| c.kind == "material" || c.kind == "crafted")
        .count();
    let intent_count = selected.iter().filter(|c| c.kind == "intent").count();

    if material_like_count < 1 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Need at least 1 material card",
        ));
    }
    if intent_count > 1 {
        return Err(err(StatusCode::BAD_REQUEST, "At most 1 intent allowed"));
    }

    // Build cache key from card IDs
    let material_ids: Vec<&str> = selected
        .iter()
        .filter(|c| c.kind != "intent")
        .map(|c| c.id.as_str())
        .collect();
    let intent_id = selected
        .iter()
        .find(|c| c.kind == "intent")
        .map(|c| c.id.as_str());
    let key = card_cache::compute_crafted_card_id(&material_ids, intent_id);

    // Check cache
    {
        let mut cache = state.card_cache.write().await;
        if let Some(cached) = cache.get(&key).cloned() {
            if cached.impossible {
                return Err(err(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Combination not possible",
                ));
            }
            let is_new = !cached.discovered;
            if is_new {
                // Mark as discovered
                let mut updated = cached.clone();
                updated.discovered = true;
                cache.insert(key.clone(), updated);
                cache.save(std::path::Path::new("cards/card-cache.json"));
            }
            return finish_combine(
                &state,
                &id,
                player_idx,
                &req.card_indices,
                &cached,
                is_new,
            )
            .await;
        }
    }

    // Cache miss — call generation server
    let combine_cards: Vec<serde_json::Value> = selected
        .iter()
        .map(|c| {
            let kind = if c.kind == "intent" {
                "intent"
            } else {
                "material"
            };
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
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Generation server error: {e}")))?;

    if !combine_resp.status().is_success() {
        let body = combine_resp.text().await.unwrap_or_default();
        return Err(err(
            StatusCode::BAD_GATEWAY,
            format!("Combination failed: {body}"),
        ));
    }

    let combined: serde_json::Value = combine_resp
        .json()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Parse error: {e}")))?;

    let card_name = combined["name"]
        .as_str()
        .unwrap_or("Unknown")
        .to_string();
    let card_desc = combined["description"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Check for "Not possible" — cache it so we don't retry
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
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Combination not possible",
        ));
    }

    // If async_image requested, return early with name/desc before image generation
    if req.async_image {
        let mut games = state.games.write().await;
        let game = games
            .get_mut(&id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;

        // Remove used cards from hand (highest index first)
        let mut sorted_indices: Vec<usize> = req.card_indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
        for idx in sorted_indices {
            if idx < game.players[player_idx].hand.len() {
                game.players[player_idx].hand.remove(idx);
            }
        }

        // Add crafted card with empty image_path (pending)
        game.players[player_idx].hand.push(HandCard {
            name: card_name.clone(),
            description: card_desc.clone(),
            kind: "crafted".to_string(),
            image_path: String::new(),
            id: key.clone(),
            nft_mint: None,
        });

        return Ok(Json(serde_json::json!({
            "game": game.clone(),
            "crafted_card": {
                "name": card_name,
                "description": card_desc,
            },
            "is_new": true,
            "image_pending": true,
            "cache_key": key,
        })));
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
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image generation error: {e}")))?;

    if !image_resp.status().is_success() {
        return Err(err(StatusCode::BAD_GATEWAY, "Image generation failed"));
    }

    let art_bytes = image_resp
        .bytes()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image read error: {e}")))?;

    // Render the card
    let png = card::render_card(&card_name, &art_bytes, &CardKind::Material)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Card render error: {e}")))?;

    // Save to disk — use card ID for unique filename
    let safe_name = card_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace(' ', "-");
    let filename = format!("{safe_name}-{key}.png");
    let disk_path = format!("cards/crafted/{filename}");
    let serve_path = format!("/cards/crafted/{filename}");

    let _ = std::fs::create_dir_all("cards/crafted");
    std::fs::write(&disk_path, &png)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("File write error: {e}")))?;

    let cached = CachedCard {
        name: card_name,
        description: card_desc,
        image_path: serve_path,
        id: key.clone(),
        discovered: true,
        impossible: false,
    };

    // Save to cache
    {
        let mut cache = state.card_cache.write().await;
        cache.insert(key, cached.clone());
        cache.save(std::path::Path::new("cards/card-cache.json"));
    }

    finish_combine(&state, &id, player_idx, &req.card_indices, &cached, true).await
}

async fn finish_combine(
    state: &Arc<AppState>,
    game_id: &str,
    player_idx: usize,
    card_indices: &[usize],
    cached: &CachedCard,
    is_new: bool,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let mut games = state.games.write().await;
    let game = games
        .get_mut(game_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;

    // Remove used cards from hand (highest index first to avoid shifting)
    let mut sorted_indices: Vec<usize> = card_indices.to_vec();
    sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted_indices {
        if idx < game.players[player_idx].hand.len() {
            game.players[player_idx].hand.remove(idx);
        }
    }

    // Add crafted card to hand
    game.players[player_idx].hand.push(HandCard {
        name: cached.name.clone(),
        description: cached.description.clone(),
        kind: "crafted".to_string(),
        image_path: cached.image_path.clone(),
        id: cached.id.clone(),
        nft_mint: None,
    });

    Ok(Json(serde_json::json!({
        "game": game.clone(),
        "crafted_card": {
            "name": cached.name,
            "description": cached.description,
            "image_path": cached.image_path,
        },
        "is_new": is_new,
    })))
}

pub async fn finalize_combine(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<FinalizeCombineRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Generate image
    let image_resp = state
        .client
        .post(format!("{}/generate-image", state.generation_url))
        .json(&serde_json::json!({
            "name": req.name,
            "description": req.description,
        }))
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image generation error: {e}")))?;

    if !image_resp.status().is_success() {
        return Err(err(StatusCode::BAD_GATEWAY, "Image generation failed"));
    }

    let art_bytes = image_resp
        .bytes()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Image read error: {e}")))?;

    // Render the card
    let png = card::render_card(&req.name, &art_bytes, &CardKind::Material)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Card render error: {e}")))?;

    // Save to disk — use card ID for unique filename
    let safe_name = req
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace(' ', "-");
    let filename = format!("{safe_name}-{}.png", req.cache_key);
    let disk_path = format!("cards/crafted/{filename}");
    let serve_path = format!("/cards/crafted/{filename}");

    let _ = std::fs::create_dir_all("cards/crafted");
    std::fs::write(&disk_path, &png)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("File write error: {e}")))?;

    let cached = CachedCard {
        name: req.name.clone(),
        description: req.description.clone(),
        image_path: serve_path.clone(),
        id: req.cache_key.clone(),
        discovered: true,
        impossible: false,
    };

    // Save to cache
    {
        let mut cache = state.card_cache.write().await;
        cache.insert(req.cache_key.clone(), cached);
        cache.save(std::path::Path::new("cards/card-cache.json"));
    }

    // Update the pending card's image_path in the player's hand
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;

    let player_idx = game.current_player;
    for card in &mut game.players[player_idx].hand {
        if card.id == req.cache_key && card.image_path.is_empty() {
            card.image_path = serve_path.clone();
            break;
        }
    }

    Ok(Json(serde_json::json!({
        "game": game.clone(),
        "image_path": serve_path,
    })))
}

pub async fn place(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PlaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let (game, player_idx) = {
        let games = state.games.read().await;
        let game = games
            .get(&id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;
        if game.phase == GamePhase::GameOver {
            return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
        }
        if game.has_placed {
            return Err(err(StatusCode::BAD_REQUEST, "Already placed a card this turn"));
        }
        (game.clone(), game.current_player)
    };

    if req.row >= 3 || req.col >= 3 {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid board position"));
    }
    if req.hand_index >= game.players[player_idx].hand.len() {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid card index"));
    }

    let hand_card = &game.players[player_idx].hand[req.hand_index];
    if hand_card.kind != "crafted" {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Only crafted cards can be placed",
        ));
    }

    let crafted = CraftedCard {
        name: hand_card.name.clone(),
        description: hand_card.description.clone(),
        image_path: hand_card.image_path.clone(),
        id: hand_card.id.clone(),
    };
    let cell = &game.board[req.row][req.col];

    let mut judgment = None;

    // Check if cell is occupied by opponent
    if let Some(placed) = &cell.card {
        if placed.owner == player_idx {
            return Err(err(StatusCode::BAD_REQUEST, "You already own this cell"));
        }

        // Contest! Call judge
        let judge_resp = state
            .client
            .post(format!("{}/judge", state.generation_url))
            .json(&serde_json::json!({
                "category": cell.category,
                "card_a": {
                    "name": placed.card.name,
                    "description": placed.card.description,
                },
                "card_b": {
                    "name": crafted.name,
                    "description": crafted.description,
                },
            }))
            .send()
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Judge error: {e}")))?;

        if !judge_resp.status().is_success() {
            return Err(err(StatusCode::BAD_GATEWAY, "Judge call failed"));
        }

        let judge_result: serde_json::Value = judge_resp
            .json()
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Judge parse error: {e}")))?;

        let winner = judge_result["winner"].as_str().unwrap_or("a");
        let reason = judge_result["reason"]
            .as_str()
            .unwrap_or("")
            .to_string();

        judgment = Some(serde_json::json!({
            "winner": winner,
            "reason": reason,
            "defender": placed.card.name,
            "attacker": crafted.name,
            "category": cell.category,
        }));

        if winner == "a" {
            // Defender wins — attacker keeps their card
            let games = state.games.read().await;
            let game = games.get(&id).unwrap();

            return Ok(Json(serde_json::json!({
                "result": "defended",
                "judgment": judgment,
                "game": game.clone(),
            })));
        }
        // Attacker wins — falls through to place
    }

    // Place the card
    let mut games = state.games.write().await;
    let game = games.get_mut(&id).unwrap();

    // If replacing an opponent's card, decrease their score
    if let Some(placed) = &game.board[req.row][req.col].card {
        let prev_owner = placed.owner;
        if prev_owner != player_idx {
            game.players[prev_owner].score = game.players[prev_owner].score.saturating_sub(1);
        }
    }

    game.board[req.row][req.col].card = Some(PlacedCard {
        card: crafted,
        owner: player_idx,
    });
    game.players[player_idx].hand.remove(req.hand_index);
    game.players[player_idx].score += 1;
    game.has_placed = true;
    game.check_winner();

    Ok(Json(serde_json::json!({
        "result": if judgment.is_some() { "conquered" } else { "placed" },
        "judgment": judgment,
        "game": game.clone(),
    })))
}

#[derive(Deserialize)]
pub struct DiscardRequest {
    pub card_indices: Vec<usize>,
}

pub async fn discard(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DiscardRequest>,
) -> Result<Json<GameState>, (StatusCode, Json<ApiError>)> {
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;

    if game.phase == GamePhase::GameOver {
        return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
    }

    if req.card_indices.is_empty() || req.card_indices.len() > 3 {
        return Err(err(StatusCode::BAD_REQUEST, "Discard 1-3 cards"));
    }

    let player_idx = game.current_player;
    let hand_len = game.players[player_idx].hand.len();
    for &idx in &req.card_indices {
        if idx >= hand_len {
            return Err(err(StatusCode::BAD_REQUEST, "Invalid card index"));
        }
    }

    // Remove from highest index first
    let mut sorted: Vec<usize> = req.card_indices.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    sorted.dedup();
    for idx in sorted {
        game.players[player_idx].hand.remove(idx);
    }

    Ok(Json(game.clone()))
}

pub async fn end_turn(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<GameState>, (StatusCode, Json<ApiError>)> {
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;

    if game.phase == GamePhase::GameOver {
        return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
    }

    game.advance_turn(&state.base_cards);

    Ok(Json(game.clone()))
}

fn build_board_data(game: &GameState) -> Vec<Vec<serde_json::Value>> {
    game.board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    serde_json::json!({
                        "category": cell.category,
                        "card": cell.card.as_ref().map(|p| serde_json::json!({
                            "name": p.card.name,
                            "description": p.card.description,
                            "owner": if p.owner == 0 { "player" } else { "bot" },
                        })),
                    })
                })
                .collect()
        })
        .collect()
}

fn build_hand_data(game: &GameState, player: usize) -> Vec<serde_json::Value> {
    game.players[player]
        .hand
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "description": c.description,
                "kind": c.kind,
            })
        })
        .collect()
}

/// Phase 1: Bot decides which cards to combine
pub async fn bot_combine(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let game = {
        let games = state.games.read().await;
        let game = games
            .get(&id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;
        if game.mode != GameMode::Bot {
            return Err(err(StatusCode::BAD_REQUEST, "Not a bot game"));
        }
        if game.current_player != 1 {
            return Err(err(StatusCode::BAD_REQUEST, "Not bot's turn"));
        }
        if game.phase == GamePhase::GameOver {
            return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
        }
        game.clone()
    };

    let board_data = build_board_data(&game);
    let hand_data = build_hand_data(&game, 1);

    let resp = state
        .client
        .post(format!("{}/bot-combine", state.generation_url))
        .json(&serde_json::json!({
            "hand": hand_data,
            "board": board_data,
            "bot_score": game.players[1].score,
            "player_score": game.players[0].score,
        }))
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Bot combine error: {e}")))?;

    if !resp.status().is_success() {
        // LLM failed — skip turn
        let mut games = state.games.write().await;
        let game = games.get_mut(&id).unwrap();
        game.advance_turn(&state.base_cards);
        return Ok(Json(serde_json::json!({
            "result": "bot_failed",
            "game": game.clone(),
        })));
    }

    let bot_result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Parse error: {e}")))?;

    let combine_indices: Vec<usize> = bot_result["combine"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as usize))
        .collect();

    // Execute the combination (synchronous for bot — no async_image)
    let combine_result = combine(
        State(state.clone()),
        Path(id.clone()),
        Json(CombineRequest {
            card_indices: combine_indices,
            async_image: false,
        }),
    )
    .await;

    match combine_result {
        Ok(result) => Ok(result),
        Err(_) => {
            // Combination failed — skip turn
            let mut games = state.games.write().await;
            let game = games.get_mut(&id).unwrap();
            game.advance_turn(&state.base_cards);
            Ok(Json(serde_json::json!({
                "result": "bot_failed",
                "game": game.clone(),
            })))
        }
    }
}

/// Phase 2: Bot decides where to place a crafted card (or skip)
pub async fn bot_place(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let game = {
        let games = state.games.read().await;
        let game = games
            .get(&id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Game not found"))?;
        if game.mode != GameMode::Bot {
            return Err(err(StatusCode::BAD_REQUEST, "Not a bot game"));
        }
        if game.current_player != 1 {
            return Err(err(StatusCode::BAD_REQUEST, "Not bot's turn"));
        }
        if game.phase == GamePhase::GameOver {
            return Err(err(StatusCode::BAD_REQUEST, "Game is over"));
        }
        game.clone()
    };

    // Check if bot has any crafted cards
    let has_crafted = game.players[1].hand.iter().any(|c| c.kind == "crafted");
    if !has_crafted {
        // Nothing to place — end turn
        let mut games = state.games.write().await;
        let game = games.get_mut(&id).unwrap();
        game.advance_turn(&state.base_cards);
        return Ok(Json(serde_json::json!({
            "result": "bot_skipped_place",
            "game": game.clone(),
        })));
    }

    let board_data = build_board_data(&game);
    let hand_data = build_hand_data(&game, 1);

    let resp = state
        .client
        .post(format!("{}/bot-place", state.generation_url))
        .json(&serde_json::json!({
            "hand": hand_data,
            "board": board_data,
            "bot_score": game.players[1].score,
            "player_score": game.players[0].score,
        }))
        .send()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Bot place error: {e}")))?;

    if !resp.status().is_success() {
        // LLM failed — end turn
        let mut games = state.games.write().await;
        let game = games.get_mut(&id).unwrap();
        game.advance_turn(&state.base_cards);
        return Ok(Json(serde_json::json!({
            "result": "bot_failed",
            "game": game.clone(),
        })));
    }

    let bot_result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("Parse error: {e}")))?;

    let skip = bot_result["skip"].as_bool().unwrap_or(false);

    if skip {
        // Bot chose to save its crafted cards — end turn
        let mut games = state.games.write().await;
        let game = games.get_mut(&id).unwrap();
        game.advance_turn(&state.base_cards);
        return Ok(Json(serde_json::json!({
            "result": "bot_skipped_place",
            "game": game.clone(),
        })));
    }

    let hand_index = bot_result["hand_index"].as_u64().unwrap_or(0) as usize;
    let target_row = bot_result["target_row"].as_u64().unwrap_or(0) as usize;
    let target_col = bot_result["target_col"].as_u64().unwrap_or(0) as usize;

    // Execute the placement
    let place_result = place(
        State(state.clone()),
        Path(id.clone()),
        Json(PlaceRequest {
            hand_index,
            row: target_row.min(2),
            col: target_col.min(2),
        }),
    )
    .await;

    match place_result {
        Ok(mut result) => {
            // End bot's turn after placing
            let mut games = state.games.write().await;
            let game = games.get_mut(&id).unwrap();
            if game.phase != GamePhase::GameOver {
                game.advance_turn(&state.base_cards);
            }
            if let Some(obj) = result.0.as_object_mut() {
                obj.insert(
                    "game".to_string(),
                    serde_json::to_value(game.clone()).unwrap(),
                );
            }
            Ok(result)
        }
        Err(_) => {
            // Place failed — end turn (bot keeps the card)
            let mut games = state.games.write().await;
            let game = games.get_mut(&id).unwrap();
            game.advance_turn(&state.base_cards);
            Ok(Json(serde_json::json!({
                "result": "bot_skipped_place",
                "game": game.clone(),
            })))
        }
    }
}
