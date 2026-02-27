use crate::combine::CombineResult;
use crate::theories::Card;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
pub struct Cache {
    results: HashMap<String, CachedEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CachedEntry {
    pub name: String,
    pub description: String,
}

impl Cache {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        let data = serde_json::to_string_pretty(self).expect("failed to serialize cache");
        std::fs::write(path, data).expect("failed to write cache file");
    }

    pub fn get(&self, cards: &[Card]) -> Option<CombineResult> {
        let key = cache_key(cards);
        self.results.get(&key).map(|e| CombineResult {
            name: e.name.clone(),
            description: e.description.clone(),
        })
    }

    pub fn insert(&mut self, cards: &[Card], result: &CombineResult) {
        let key = cache_key(cards);
        self.results.insert(
            key,
            CachedEntry {
                name: result.name.clone(),
                description: result.description.clone(),
            },
        );
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }
}

fn cache_key(cards: &[Card]) -> String {
    use crate::theories::CardKind;

    let mut materials: Vec<String> = cards
        .iter()
        .filter(|c| c.kind == CardKind::Material)
        .map(|c| c.name.to_lowercase())
        .collect();
    materials.sort();

    let intent: Option<String> = cards
        .iter()
        .find(|c| c.kind == CardKind::Intent)
        .map(|c| c.name.to_lowercase());

    match intent {
        Some(i) => format!("{}+[{}]", materials.join("+"), i),
        None => materials.join("+"),
    }
}
