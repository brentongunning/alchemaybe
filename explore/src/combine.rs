use crate::theories::{Card, CardKind};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Replicates the exact system prompt from generation/src/ollama.rs
const SYSTEM_PROMPT: &str = "\
You combine items alchemically. Output what the items PRODUCE together.

Some inputs may be concept cards (like \"Forge\" or \"Wild\") rather than physical materials.
These guide what you create from the materials — they set the intent, not the substance.
Only materials are consumed. The concept shapes the outcome.

Good examples:
Fire + Water = {\"name\": \"Steam\", \"description\": \"Hot wispy cloud that fogs up every mirror in sight\"}
Tree + Fire = {\"name\": \"Ash\", \"description\": \"Fine grey powder that used to be a tree\"}
Sand + Fire = {\"name\": \"Glass\", \"description\": \"Clear solid that shatters if you look at it wrong\"}
Earth + Water + Seed = {\"name\": \"Sprout\", \"description\": \"Tiny green shoot reaching for the sun\"}
Water (x3) = {\"name\": \"Pond\", \"description\": \"Still little pool where frogs hold their meetings\"}
Fire (x3) = {\"name\": \"Bonfire\", \"description\": \"Roaring blaze that demands marshmallows\"}
Metal + Fire [Forge] = {\"name\": \"Sword\", \"description\": \"Long sharp blade that solves all disagreements\"}
Wood + Fiber [Harmony] = {\"name\": \"Lute\", \"description\": \"Stringed little charmer that plays itself in your dreams\"}
Clay + Water [Wild] = {\"name\": \"Wasp Nest\", \"description\": \"Mud fortress buzzing with angry tenants\"}

Not possible examples (output \"Not possible\" for these):
Water + Wood [Forge] = Not possible (Forge needs metal — there is no metal here)
Fire + Ice [Harmony] = Not possible (these just cancel out, nothing harmonious to make)
Stone + Stone = Not possible (two rocks just sit there)
Bone + Light = Not possible (light does nothing to bone without a process)
Egg + Stone = Not possible (no heat or process to do anything meaningful)

Rules:
- Output what the interaction PRODUCES, not what survives.
- Must be a real, human-scale, physically existing thing. No magic or fiction.
- It must be a single cohesive thing, not a collection.
- STRICT material conservation: the result can ONLY be made from substances actually present in the inputs. You cannot conjure materials that aren't there. Metal + Fire can make a Sword, but Wood + Water cannot — there is no metal.
- A concept/intent card steers the direction but CANNOT introduce new materials. [Forge] without metal inputs = Not possible. [Harmony] without something that vibrates or resonates = Not possible.
- Two passive, inert materials with no energy source or process usually = Not possible. Stone + Bone, Clay + Seed (no water), Wood + Stone — these just sit next to each other.
- At least one input must provide energy, transformation, or a biological/chemical process (fire, water, wind, light, or a living thing like seed/egg).
- If you cannot explain a short, real-world physical process that turns EXACTLY these inputs into the result, output \"Not possible\".
- If the combination is ongoing (like burning), output what it produces.
- If items repeat, the result is a bigger or more intense version.
- If there is no obvious combination, name is \"Not possible\".
- The name alone must identify the thing. Use a specific recognizable noun. The name should imply the description — e.g. \"Molten Metal\" not just \"Metal\" if it is hot.
- Name: 1-3 words.
- Description: MUST start with an adjective or noun. NEVER start with A, An, The, This, It, or Its. One short funny sentence about what it is, not how it was made.";

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    system: String,
    stream: bool,
    format: serde_json::Value,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    seed: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombineResult {
    pub name: String,
    pub description: String,
}

pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }

    pub async fn combine(&self, cards: &[Card]) -> Result<CombineResult, String> {
        let prompt = build_user_prompt(cards);
        let url = format!("{}/api/generate", self.base_url);

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt,
            system: SYSTEM_PROMPT.to_string(),
            stream: false,
            format: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" }
                },
                "required": ["name", "description"]
            }),
            options: GenerateOptions {
                temperature: 0.0,
                seed: 42,
            },
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        let result: CombineResult = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse LLM output: {e}"))?;

        Ok(result)
    }

    /// Score a card against all board categories. Returns a map of category -> score (1-10).
    pub async fn score_categories(
        &self,
        card_name: &str,
        card_description: &str,
        categories: &[&str],
    ) -> Result<HashMap<String, u32>, String> {
        let cats_list = categories
            .iter()
            .map(|c| format!("  \"{c}\": <1-10>"))
            .collect::<Vec<_>>()
            .join(",\n");

        let system = format!(
            "Rate how well this item fits each game category. Score 1-10.\n\
             1-3 = poor fit, 4-6 = moderate, 7-10 = strong fit. Be strict.\n\
             Return JSON with exactly these keys:\n{{\n{cats_list}\n}}"
        );

        let prompt = format!("Item: {card_name}\nDescription: {card_description}");
        let url = format!("{}/api/generate", self.base_url);

        // Build properties for JSON schema
        let mut props = serde_json::Map::new();
        for cat in categories {
            props.insert(
                cat.to_string(),
                serde_json::json!({ "type": "integer" }),
            );
        }
        let required: Vec<&str> = categories.to_vec();

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt,
            system,
            stream: false,
            format: serde_json::json!({
                "type": "object",
                "properties": props,
                "required": required
            }),
            options: GenerateOptions {
                temperature: 0.0,
                seed: 42,
            },
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama scoring request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse scoring response: {e}"))?;

        let scores: HashMap<String, u32> = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse scoring output: {e}"))?;

        Ok(scores)
    }
}

fn build_user_prompt(cards: &[Card]) -> String {
    let mut material_counts: Vec<(String, String, usize)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut intent: Option<(String, String)> = None;

    for card in cards {
        if card.kind == CardKind::Intent {
            intent = Some((card.name.clone(), card.description.clone()));
            continue;
        }
        let key = card.name.to_lowercase();
        if let Some(&idx) = seen.get(&key) {
            material_counts[idx].2 += 1;
        } else {
            seen.insert(key, material_counts.len());
            material_counts.push((card.name.clone(), card.description.clone(), 1));
        }
    }

    let parts: Vec<String> = material_counts
        .iter()
        .map(|(name, _, count)| {
            if *count > 1 {
                format!("{name} (x{count})")
            } else {
                name.clone()
            }
        })
        .collect();

    let combo = if let Some((ref intent_name, _)) = intent {
        format!("{} [{}]", parts.join(" + "), intent_name)
    } else {
        parts.join(" + ")
    };

    let mut prompt = format!("{combo} = ?\n\nDescriptions:\n");
    for (name, desc, _) in &material_counts {
        prompt.push_str(&format!("- {name}: {desc}\n"));
    }
    if let Some((intent_name, intent_desc)) = &intent {
        prompt.push_str(&format!("- [{intent_name}]: {intent_desc}\n"));
    }
    prompt
}
