use crate::combine::Card;

pub trait CardGenerator: Send + Sync {
    fn generate(
        &self,
        cards: &[Card],
    ) -> impl std::future::Future<Output = Result<Card, String>> + Send;
}

pub trait ImageGenerator: Send + Sync {
    fn generate_image(
        &self,
        card: &Card,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeRequest {
    pub category: String,
    pub card_a: JudgeCard,
    pub card_b: JudgeCard,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeCard {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeResult {
    pub winner: String, // "a" or "b"
    pub reason: String,
}

pub trait JudgeGenerator: Send + Sync {
    fn judge(
        &self,
        req: &JudgeRequest,
    ) -> impl std::future::Future<Output = Result<JudgeResult, String>> + Send;
}

// --- Bot Combine ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BotCombineRequest {
    pub hand: Vec<serde_json::Value>,
    pub board: Vec<Vec<serde_json::Value>>,
    pub bot_score: u32,
    pub player_score: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BotCombineResult {
    pub combine: Vec<usize>,
}

pub trait BotCombineGenerator: Send + Sync {
    fn bot_combine(
        &self,
        req: &BotCombineRequest,
    ) -> impl std::future::Future<Output = Result<BotCombineResult, String>> + Send;
}

// --- Bot Place ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BotPlaceRequest {
    pub hand: Vec<serde_json::Value>,
    pub board: Vec<Vec<serde_json::Value>>,
    pub bot_score: u32,
    pub player_score: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BotPlaceResult {
    pub hand_index: usize,
    pub target_row: usize,
    pub target_col: usize,
    pub skip: bool,
}

pub trait BotPlaceGenerator: Send + Sync {
    fn bot_place(
        &self,
        req: &BotPlaceRequest,
    ) -> impl std::future::Future<Output = Result<BotPlaceResult, String>> + Send;
}
