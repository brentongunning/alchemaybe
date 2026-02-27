use crate::combine::{Card, CardKind};
use crate::generator::{
    BotCombineGenerator, BotCombineRequest, BotCombineResult, BotPlaceGenerator, BotPlaceRequest,
    BotPlaceResult, CardGenerator, ImageGenerator, JudgeGenerator, JudgeRequest, JudgeResult,
};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct OllamaConfig {
    base_url: String,
    model: String,
    image_model: Option<String>,
}

impl OllamaConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("OLLAMA_URL").expect("OLLAMA_URL must be set"),
            model: std::env::var("OLLAMA_MODEL").expect("OLLAMA_MODEL must be set"),
            image_model: std::env::var("OLLAMA_IMAGE_MODEL").ok(),
        }
    }
}

pub struct OllamaGenerator {
    client: Client,
    config: OllamaConfig,
}

impl OllamaGenerator {
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");
        Self { client, config }
    }
}

const SYSTEM_PROMPT: &str = "\
You combine items alchemically. Output what the items PRODUCE together.

Some inputs may be intent cards (like \"Sharp\" or \"Hollow\") rather than physical materials.
These guide what you create from the materials — they set the creative direction, not the substance.
Only materials are consumed. The intent shapes the outcome.

IMPORTANT: When an intent card is present, make a BIG creative leap. Don't just combine the raw \
materials — imagine what a craftsperson would BUILD from those materials with that intent in mind. \
The intent transforms raw stuff into finished objects, tools, instruments, weapons, clothing, etc. \
Without an intent, combinations produce simpler, more elemental results.

Good examples WITHOUT intent (simple, elemental results):
Fire + Water = {\"name\": \"Steam\", \"description\": \"Hot wispy cloud that fogs up every mirror in sight\"}
Tree + Fire = {\"name\": \"Ash\", \"description\": \"Fine grey powder that used to be a tree\"}
Sand + Fire = {\"name\": \"Glass\", \"description\": \"Clear solid that shatters if you look at it wrong\"}
Earth + Water + Seed = {\"name\": \"Sprout\", \"description\": \"Tiny green shoot reaching for the sun\"}

Good examples WITH intent (big creative leap — finished objects):
Metal + Fire [Sharp] = {\"name\": \"Sword\", \"description\": \"Long sharp blade that solves all disagreements\"}
Wood + Fiber [Loud] = {\"name\": \"Drum\", \"description\": \"Hollow wooden cylinder that wakes the whole village\"}
Clay + Water [Hollow] = {\"name\": \"Vase\", \"description\": \"Elegant curved pot begging for flowers\"}
Metal + Coal + Fire [Heavy] = {\"name\": \"Anvil\", \"description\": \"Iron slab so heavy it might fall through the floor\"}
Bone + Fiber [Sharp] = {\"name\": \"Fishing Hook\", \"description\": \"Wickedly curved barb that fish never see coming\"}
Sand + Fire [Bright] = {\"name\": \"Lantern\", \"description\": \"Glass globe that holds a tiny captive sunrise\"}
Wood + Fiber [Soft] = {\"name\": \"Pillow\", \"description\": \"Plump cushion stuffed with dreams and plant fluff\"}
Stone + Metal [Ancient] = {\"name\": \"Runestone\", \"description\": \"Heavy slab carved with symbols nobody alive can read\"}
Clay + Fire [Sticky] = {\"name\": \"Tar Pot\", \"description\": \"Bubbling cauldron of goo that never lets go\"}
Fiber + Wood [Tiny] = {\"name\": \"Sewing Needle\", \"description\": \"Impossibly thin sliver that stitches worlds together\"}
Egg + Fire [Sweet] = {\"name\": \"Custard\", \"description\": \"Wobbly golden dessert that jiggles when you look at it\"}
Stone [Many] = {\"name\": \"Stone Wall\", \"description\": \"Towering stack of rocks that keeps everything interesting on the other side\"}
Fiber [Many] = {\"name\": \"Tapestry\", \"description\": \"Enormous woven hanging that tells a story nobody asked for\"}
Wood + Fire [Many] = {\"name\": \"Bonfire\", \"description\": \"Roaring blaze big enough to warm an entire camp\"}
Seed [Time] = {\"name\": \"Oak Tree\", \"description\": \"Massive gnarled trunk with branches that scrape the clouds\"}
Egg [Time] = {\"name\": \"Eagle\", \"description\": \"Fierce raptor with golden eyes and a wingspan wider than your house\"}
Metal + Water [Time] = {\"name\": \"Rust\", \"description\": \"Crumbly orange flakes that ate a perfectly good sword\"}

Not possible examples (output \"Not possible\" for these):
Water + Wood [Sharp] = Not possible (no hard material to form an edge)
Stone + Stone = Not possible (two rocks just sit there)
Bone + Light = Not possible (light does nothing to bone without a process)
Egg + Stone = Not possible (no heat or process to do anything meaningful)

Rules:
- Output what the interaction PRODUCES, not what survives.
- The result MUST be a real thing that actually exists (or existed) in the real world. \
Something you could find, buy, or make. Not an invented fantasy object, not a made-up compound, \
not a poetic abstraction. \"Sword\" is real. \"Flame Crystal\" is not. \"Bread\" is real. \"Fire Dough\" is not.
- It must be a single cohesive thing, not a collection.
- STRICT material conservation: the result can ONLY be made from substances actually present in the inputs. You cannot conjure materials that aren't there.
- An intent card steers the direction but CANNOT introduce new materials. Think of the intent as what a craftsperson WANTS to make — but they can only use the materials given.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
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

#[derive(Deserialize)]
struct LlmCard {
    name: String,
    description: String,
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

const VALIDATE_SYSTEM_PROMPT: &str = "\
You are a reality checker. Given the name of an object, decide if it is a REAL thing that exists \
(or has existed) in the real world. Something a person could find, buy, make, or encounter.

Answer \"yes\" if it is a real, recognized thing. Examples of real things: Sword, Bread, Candle, \
Drum, Glass, Rope, Brick, Compass, Fishing Hook, Custard, Lantern, Pillow, Anvil, Flute.

Answer \"no\" if it is made up, a fantasy invention, a poetic abstraction, or a compound word \
that doesn't refer to a real object. Examples of NOT real: Flame Crystal, Thunder Paste, \
Wind Silk, Ember Stone, Soul Vessel, Fire Dough, Light Weave, Bone Whisper.

Output JSON: {\"real\": true} or {\"real\": false}";

impl CardGenerator for OllamaGenerator {
    async fn generate(&self, cards: &[Card]) -> Result<Card, String> {
        let url = format!("{}/api/generate", self.config.base_url);
        let prompt = build_user_prompt(cards);
        log::debug!("Combine prompt:\n{prompt}");

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt,
            system: SYSTEM_PROMPT.to_string(),
            stream: false,
            format: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" }
                },
                "required": ["name", "description"]
            })),
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

        log::debug!("Combine response: {}", gen_resp.response);

        let llm_card: LlmCard = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse LLM output: {e}"))?;

        // Check for "Not possible" before validation
        if llm_card.name.to_lowercase().contains("not possible") {
            return Ok(Card {
                name: llm_card.name,
                description: llm_card.description,
                kind: Default::default(),
            });
        }

        // Validate that the result is a real thing
        log::info!("Validating '{}' is a real thing...", llm_card.name);
        let validate_request = GenerateRequest {
            model: self.config.model.clone(),
            prompt: format!("Is \"{}\" a real thing?", llm_card.name),
            system: VALIDATE_SYSTEM_PROMPT.to_string(),
            stream: false,
            format: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "real": { "type": "boolean" }
                },
                "required": ["real"]
            })),
            options: GenerateOptions {
                temperature: 0.0,
                seed: 42,
            },
        };

        let validate_resp = self
            .client
            .post(&url)
            .json(&validate_request)
            .send()
            .await
            .map_err(|e| format!("Validation request failed: {e}"))?;

        if validate_resp.status().is_success() {
            if let Ok(gen_resp) = validate_resp.json::<GenerateResponse>().await {
                if let Ok(result) = serde_json::from_str::<serde_json::Value>(&gen_resp.response) {
                    if result["real"].as_bool() == Some(false) {
                        log::info!("'{}' rejected — not a real thing", llm_card.name);
                        return Ok(Card {
                            name: "Not possible".to_string(),
                            description: format!("{} is not a real thing", llm_card.name),
                            kind: Default::default(),
                        });
                    }
                }
            }
        }
        log::info!("'{}' validated as real", llm_card.name);

        Ok(Card {
            name: llm_card.name,
            description: llm_card.description,
            kind: Default::default(),
        })
    }
}

#[derive(Serialize)]
struct ImageGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    keep_alive: u32,
    width: u32,
    height: u32,
    steps: u32,
    seed: u32,
}

#[derive(Deserialize)]
struct ImageGenerateResponse {
    image: String,
}

const IMAGE_DESCRIPTION_SYSTEM_PROMPT: &str = "\
You describe card artwork for a fantasy card game. Given a card name and description, \
output a vivid visual description of what the card illustration should look like. \
The goal is to produce dramatic, colorful, fantastical card art — like World of Warcraft \
or Hearthstone trading card illustrations.

Rules:
- The subject must be the clear, dominant focus of the image. It should be immediately \
obvious what the thing is. Start the description with the subject.
- Use the name as the subject, not the description. The description is just to \
differentiate the subject from other subjects of the same name.
- Epic fantasy painting style — bold, saturated colors, dramatic lighting, rich detail.
- The subject should feel larger-than-life, heroic, magical. Even mundane objects should \
look enchanted or legendary.
- Dark background (black, deep shadow, smoky void). The subject should be brightly lit \
with dramatic rim lighting and glowing effects, popping against the darkness.
- No border, no frame, no text, clean edges.
- Use vibrant, saturated colors — golds, deep blues, fiery oranges, emerald greens, \
royal purples. Make it visually striking.
- Add fantasy atmosphere: magical particles, embers, sparkles, wisps of energy, \
subtle glowing runes, or enchanted auras where appropriate.
- Do NOT add people, monsters, or creatures to the image unless the card IS a person, \
monster, or creature. A \"Tar\" card should show tar, not a tar monster. A \"Sword\" should \
show a sword, not a warrior holding a sword. Render the OBJECT itself.
- Keep the description short (2-3 sentences). Focus on the most striking visual elements.
- Output ONLY the visual description. No questions, no offers to refine, no preamble.";

const INTENT_IMAGE_DESCRIPTION_SYSTEM_PROMPT: &str = "\
You describe card artwork for a fantasy card game. This is an INTENT card — an abstract concept \
that guides how materials combine, not a physical object. Given the concept name and description, \
output a vivid visual description of what the card illustration should look like. \
The goal is to produce dramatic, mystical card art — like a World of Warcraft spell or enchantment card.

Rules:
- This is an abstract concept, not a physical thing. Show it as a powerful symbolic visual.
- Use a single iconic symbol or dramatic visual metaphor that clearly represents the concept. \
The symbol must be the dominant focus — immediately recognizable. Start the description with it.
- Epic fantasy painting style — bold, saturated colors, dramatic lighting, magical energy.
- Dark background (deep purple, dark indigo, black void). The symbol should blaze with \
light and magical energy against the darkness.
- No border, no frame, no text, clean edges.
- Use rich, glowing colors — arcane purples, molten golds, ethereal blues, spectral greens.
- Add magical atmosphere: swirling energy, floating runes, crackling power, mystical auras.
- Keep the description short (2-3 sentences). Focus on the most striking visual elements.
- Output ONLY the visual description. No questions, no offers to refine, no preamble.";

const MAX_DESCRIPTION_RETRIES: u32 = 5;

impl OllamaGenerator {
    async fn describe_card_image(&self, card: &Card) -> Result<String, String> {
        let mut last_err = String::new();
        for attempt in 1..=MAX_DESCRIPTION_RETRIES {
            match self.try_describe_card_image(card, attempt).await {
                Ok(description) => return Ok(description),
                Err(e) => {
                    log::warn!("Image description attempt {attempt}/{MAX_DESCRIPTION_RETRIES} failed: {e}");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    async fn try_describe_card_image(&self, card: &Card, attempt: u32) -> Result<String, String> {
        let start = Instant::now();
        log::info!("Generating image description for '{}' (attempt {attempt})...", card.name);
        let url = format!("{}/api/generate", self.config.base_url);

        let prompt = format!(
            "Card name: {}\nCard description: {}\n\nDescribe the card illustration.",
            card.name, card.description
        );
        log::debug!("Image description prompt:\n{prompt}");

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt,
            system: if card.kind == CardKind::Intent {
                INTENT_IMAGE_DESCRIPTION_SYSTEM_PROMPT
            } else {
                IMAGE_DESCRIPTION_SYSTEM_PROMPT
            }
            .to_string(),
            stream: false,
            format: None,
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
            .map_err(|e| {
                log::error!("Ollama image description request failed after {:.1}s: {e}", start.elapsed().as_secs_f64());
                format!("Ollama image description request failed: {e}")
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            log::error!("Ollama returned {status} for image description after {:.1}s: {body}", start.elapsed().as_secs_f64());
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        let description = gen_resp.response.trim().to_string();
        if description.is_empty() {
            return Err("LLM returned empty description".to_string());
        }
        log::info!("Image description generated in {:.1}s", start.elapsed().as_secs_f64());
        log::debug!("Image description result:\n{description}");
        Ok(description)
    }
}

impl ImageGenerator for OllamaGenerator {
    async fn generate_image(&self, card: &Card) -> Result<Vec<u8>, String> {
        let image_model = self
            .config
            .image_model
            .as_ref()
            .ok_or("OLLAMA_IMAGE_MODEL is not configured")?;

        let visual_description = self.describe_card_image(card).await?;
        let start = Instant::now();
        log::info!("Generating image for '{}'...", card.name);
        log::debug!("Image generation prompt:\n{visual_description}");

        let url = format!("{}/api/generate", self.config.base_url);

        let request = ImageGenerateRequest {
            model: image_model.clone(),
            prompt: visual_description,
            stream: false,
            keep_alive: 0,
            width: 750,
            height: 1050,
            steps: 4,
            seed: 42,
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                log::error!("Ollama image request failed after {:.1}s: {e}", start.elapsed().as_secs_f64());
                format!("Ollama image request failed: {e}")
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            log::error!("Ollama returned {status} for image after {:.1}s: {body}", start.elapsed().as_secs_f64());
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: ImageGenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama image response: {e}"))?;

        log::info!("Image rendered in {:.1}s", start.elapsed().as_secs_f64());

        base64::engine::general_purpose::STANDARD
            .decode(&gen_resp.image)
            .map_err(|e| format!("Failed to decode base64 image: {e}"))
    }
}

const JUDGE_SYSTEM_PROMPT: &str = "\
You are a judge in an alchemy card game. Two crafted items are competing for a category slot on the board.

Given a category and two cards (A and B), decide which card fits the category BETTER.

Rules:
- Consider how well each card serves the purpose described by the category.
- A card doesn't need to be a perfect fit — just better than the other.
- Consider practical utility, not just name similarity.
- Be decisive. Always pick a winner.

Output JSON with:
- \"winner\": \"a\" or \"b\"
- \"reason\": One short sentence explaining why the winner fits the category better.";

impl JudgeGenerator for OllamaGenerator {
    async fn judge(&self, req: &JudgeRequest) -> Result<JudgeResult, String> {
        let url = format!("{}/api/generate", self.config.base_url);

        let prompt = format!(
            "Category: {}\n\nCard A: {} — {}\nCard B: {} — {}\n\nWhich card fits the category better?",
            req.category, req.card_a.name, req.card_a.description, req.card_b.name, req.card_b.description
        );

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt,
            system: JUDGE_SYSTEM_PROMPT.to_string(),
            stream: false,
            format: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "winner": { "type": "string", "enum": ["a", "b"] },
                    "reason": { "type": "string" }
                },
                "required": ["winner", "reason"]
            })),
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
            .map_err(|e| format!("Judge request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse judge response: {e}"))?;

        let result: JudgeResult = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse judge output: {e}"))?;

        Ok(result)
    }
}

const BOT_COMBINE_SYSTEM_PROMPT: &str = "\
You are an AI player in an alchemy card game. You need to choose cards from your hand to combine.

The board is a 3x3 grid. Each cell has a category. Some cells have cards placed by \"player\" or \"bot\".
First to 5 cells wins.

Your task: look at the board categories (especially empty cells and cells owned by \"player\") \
and pick 2-3 cards from your hand that could combine into something fitting one of those categories.

Strategy:
- Look at empty cells first — what categories need filling?
- If the player has 4 cells, you MUST try to craft something to conquer one of their cells.
- Pick materials that alchemically combine into something related to a target category.
- You may include at most 1 intent card to guide the combination.
- Material cards combine alchemically: Fire+Metal=[Sharp] could make a Sword (Weapon category).
- Think about what the combination will PRODUCE, not the inputs themselves.

Output JSON with:
- \"combine\": array of hand indices (0-based) to combine (2-4 cards, at least 2 must be materials/crafted)";

impl BotCombineGenerator for OllamaGenerator {
    async fn bot_combine(&self, req: &BotCombineRequest) -> Result<BotCombineResult, String> {
        let url = format!("{}/api/generate", self.config.base_url);

        let prompt = format!(
            "Your hand (by index):\n{}\n\nBoard:\n{}\n\nBot score: {}, Player score: {}\n\n\
             Pick cards from your hand to combine into something useful for the board.",
            req.hand
                .iter()
                .enumerate()
                .map(|(i, c)| format!("  [{}] {}", i, c))
                .collect::<Vec<_>>()
                .join("\n"),
            serde_json::to_string_pretty(&req.board).unwrap_or_default(),
            req.bot_score,
            req.player_score,
        );

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt,
            system: BOT_COMBINE_SYSTEM_PROMPT.to_string(),
            stream: false,
            format: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "combine": { "type": "array", "items": { "type": "integer" } }
                },
                "required": ["combine"]
            })),
            options: GenerateOptions {
                temperature: 0.3,
                seed: 42,
            },
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Bot combine request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse bot combine response: {e}"))?;

        let result: BotCombineResult = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse bot combine output: {e}"))?;

        Ok(result)
    }
}

const BOT_PLACE_SYSTEM_PROMPT: &str = "\
You are an AI player in an alchemy card game. You need to decide where to place a card on the board.

The board is a 3x3 grid. Each cell has a category. Some cells have cards placed by \"player\" or \"bot\".
First to 5 cells wins. Only crafted cards (kind=\"crafted\") can be placed.

Your task: look at your crafted cards and the board, and decide the best placement.

Strategy:
- Only crafted cards can be placed on the board.
- Place on empty cells where your card fits the category well.
- If the player has 4 cells, you MUST try to conquer one of their cells with a better-fitting card.
- If you contest an opponent's cell, a judge decides which card fits the category better. Only attack if confident.
- If none of your crafted cards fit any available category well, set skip=true to save them for later.
- Consider: is it better to place suboptimally now, or hold the card for a future turn?

Output JSON with:
- \"hand_index\": index of the crafted card in your hand to place
- \"target_row\": row index (0-2)
- \"target_col\": column index (0-2)
- \"skip\": true if you want to skip placing this turn (save crafted cards for later)";

impl BotPlaceGenerator for OllamaGenerator {
    async fn bot_place(&self, req: &BotPlaceRequest) -> Result<BotPlaceResult, String> {
        let url = format!("{}/api/generate", self.config.base_url);

        // Check if bot has any crafted cards
        let has_crafted = req.hand.iter().any(|c| {
            c.get("kind")
                .and_then(|k| k.as_str())
                .map(|k| k == "crafted")
                .unwrap_or(false)
        });
        if !has_crafted {
            return Ok(BotPlaceResult {
                hand_index: 0,
                target_row: 0,
                target_col: 0,
                skip: true,
            });
        }

        let prompt = format!(
            "Your hand (by index):\n{}\n\nBoard:\n{}\n\nBot score: {}, Player score: {}\n\n\
             Choose which crafted card to place and where, or skip if nothing fits well.",
            req.hand
                .iter()
                .enumerate()
                .map(|(i, c)| format!("  [{}] {}", i, c))
                .collect::<Vec<_>>()
                .join("\n"),
            serde_json::to_string_pretty(&req.board).unwrap_or_default(),
            req.bot_score,
            req.player_score,
        );

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt,
            system: BOT_PLACE_SYSTEM_PROMPT.to_string(),
            stream: false,
            format: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "hand_index": { "type": "integer" },
                    "target_row": { "type": "integer" },
                    "target_col": { "type": "integer" },
                    "skip": { "type": "boolean" }
                },
                "required": ["hand_index", "target_row", "target_col", "skip"]
            })),
            options: GenerateOptions {
                temperature: 0.3,
                seed: 42,
            },
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Bot place request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let gen_resp: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse bot place response: {e}"))?;

        let result: BotPlaceResult = serde_json::from_str(&gen_resp.response)
            .map_err(|e| format!("Failed to parse bot place output: {e}"))?;

        Ok(result)
    }
}
