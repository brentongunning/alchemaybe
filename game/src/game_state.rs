use crate::card_cache;
use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseCard {
    pub name: String,
    pub description: String,
    pub kind: String, // "material" or "intent"
    pub image_path: String,
    pub id: String,
}

/// A card in a player's hand — can be a base card or a crafted card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandCard {
    pub name: String,
    pub description: String,
    pub kind: String, // "material", "intent", or "crafted"
    pub image_path: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft_mint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftedCard {
    pub name: String,
    pub description: String,
    pub image_path: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardCell {
    pub category: String,
    pub card: Option<PlacedCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedCard {
    pub card: CraftedCard,
    pub owner: usize, // 0 or 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub hand: Vec<HandCard>,
    pub score: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase {
    Playing,
    GameOver,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
    Pvp,
    Bot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub id: String,
    pub mode: GameMode,
    pub phase: GamePhase,
    pub current_player: usize,
    pub board: Vec<Vec<BoardCell>>,
    pub players: [PlayerState; 2],
    pub winner: Option<usize>,
    pub has_placed: bool,
}

const HAND_SIZE: usize = 7;
const WIN_SCORE: u32 = 5;

impl HandCard {
    pub fn from_base(base: &BaseCard) -> Self {
        HandCard {
            name: base.name.clone(),
            description: base.description.clone(),
            kind: base.kind.clone(),
            image_path: base.image_path.clone(),
            id: base.id.clone(),
            nft_mint: None,
        }
    }
}

impl GameState {
    pub fn new(
        id: String,
        mode: GameMode,
        categories: &[String],
        base_cards: &[BaseCard],
    ) -> Self {
        let mut rng = rand::rng();

        // Pick 9 random categories
        let mut cats = categories.to_vec();
        cats.shuffle(&mut rng);
        let chosen: Vec<String> = cats.into_iter().take(9).collect();

        // Build 3x3 board
        let mut board = Vec::new();
        for row in 0..3 {
            let mut cells = Vec::new();
            for col in 0..3 {
                cells.push(BoardCell {
                    category: chosen[row * 3 + col].clone(),
                    card: None,
                });
            }
            board.push(cells);
        }

        let hand0: Vec<HandCard> = (0..HAND_SIZE)
            .map(|_| HandCard::from_base(draw_random_card(base_cards, &mut rng)))
            .collect();
        let hand1: Vec<HandCard> = (0..HAND_SIZE)
            .map(|_| HandCard::from_base(draw_random_card(base_cards, &mut rng)))
            .collect();

        GameState {
            id,
            mode,
            phase: GamePhase::Playing,
            current_player: 0,
            board,
            players: [
                PlayerState {
                    hand: hand0,
                    score: 0,
                    wallet: None,
                },
                PlayerState {
                    hand: hand1,
                    score: 0,
                    wallet: None,
                },
            ],
            winner: None,
            has_placed: false,
        }
    }

    /// Draw random base cards until hand has HAND_SIZE cards.
    /// Materials are drawn twice as frequently as intents.
    pub fn replenish_hand(&mut self, player: usize, base_cards: &[BaseCard]) {
        let mut rng = rand::rng();
        while self.players[player].hand.len() < HAND_SIZE {
            self.players[player]
                .hand
                .push(HandCard::from_base(draw_random_card(base_cards, &mut rng)));
        }
    }

    pub fn check_winner(&mut self) {
        for i in 0..2 {
            if self.players[i].score >= WIN_SCORE {
                self.winner = Some(i);
                self.phase = GamePhase::GameOver;
                return;
            }
        }
    }

    pub fn advance_turn(&mut self, base_cards: &[BaseCard]) {
        // Replenish current player's hand before switching
        let player = self.current_player;
        self.replenish_hand(player, base_cards);
        self.current_player = 1 - self.current_player;
        self.has_placed = false;
    }
}

/// Draw a random base card. Materials are drawn with 2:1 probability vs intents,
/// regardless of how many of each type exist.
fn draw_random_card<'a>(base_cards: &'a [BaseCard], rng: &mut rand::rngs::ThreadRng) -> &'a BaseCard {
    let materials: Vec<&BaseCard> = base_cards.iter().filter(|c| c.kind == "material").collect();
    let intents: Vec<&BaseCard> = base_cards.iter().filter(|c| c.kind == "intent").collect();

    // 2/3 chance material, 1/3 chance intent
    if !intents.is_empty() && !materials.is_empty() && rng.random_ratio(1, 3) {
        intents.choose(rng).unwrap()
    } else {
        materials.choose(rng).unwrap()
    }
}

pub fn build_base_cards(cards_json: &serde_json::Value) -> Vec<BaseCard> {
    let mut base_cards = Vec::new();

    if let Some(materials) = cards_json["materials"].as_array() {
        for m in materials {
            let name = m["name"].as_str().unwrap_or_default().to_string();
            base_cards.push(BaseCard {
                id: card_cache::compute_base_card_id(&name),
                description: m["description"].as_str().unwrap_or_default().to_string(),
                kind: "material".to_string(),
                image_path: format!("/cards/materials/{}.png", &name),
                name,
            });
        }
    }

    if let Some(intents) = cards_json["intents"].as_array() {
        for i in intents {
            let name = i["name"].as_str().unwrap_or_default().to_string();
            base_cards.push(BaseCard {
                id: card_cache::compute_base_card_id(&name),
                description: i["description"].as_str().unwrap_or_default().to_string(),
                kind: "intent".to_string(),
                image_path: format!("/cards/intents/{}.png", &name),
                name,
            });
        }
    }

    base_cards
}
