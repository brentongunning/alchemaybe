use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCard {
    pub name: String,
    pub description: String,
    pub image_path: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub discovered: bool,
    #[serde(default)]
    pub impossible: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct CardCache {
    entries: HashMap<String, CachedCard>,
}

impl CardCache {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }

    pub fn get(&self, key: &str) -> Option<&CachedCard> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: String, card: CachedCard) {
        self.entries.insert(key, card);
    }

    pub fn all_entries(&self) -> impl Iterator<Item = (&String, &CachedCard)> {
        self.entries.iter()
    }
}

/// Compute the ID for a base card: SHA-256 of lowercase name, first 12 hex chars.
pub fn compute_base_card_id(name: &str) -> String {
    let hash = Sha256::digest(name.to_lowercase().as_bytes());
    format!("{:x}", hash)[..12].to_string()
}

/// Compute the ID for a crafted card from its input IDs.
/// Sorts material IDs, joins with "+", appends "+[intent_id]" if present.
/// Returns SHA-256 first 12 hex chars.
pub fn compute_crafted_card_id(material_ids: &[&str], intent_id: Option<&str>) -> String {
    let mut ids: Vec<String> = material_ids.iter().map(|id| id.to_string()).collect();
    ids.sort();
    let mut key = ids.join("+");
    if let Some(intent) = intent_id {
        key.push_str(&format!("+[{}]", intent));
    }
    let hash = Sha256::digest(key.as_bytes());
    format!("{:x}", hash)[..12].to_string()
}
