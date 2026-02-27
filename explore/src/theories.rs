use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    #[default]
    Material,
    Intent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: CardKind,
}

impl Card {
    pub fn material(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            kind: CardKind::Material,
        }
    }

    pub fn intent(name: &str, meaning: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Concept card — guides the combination toward {meaning}"),
            kind: CardKind::Intent,
        }
    }
}

// ---------- Element sets ----------

pub struct ElementTheory {
    pub name: &'static str,
    pub label: &'static str,
    pub hypothesis: &'static str,
    pub elements: Vec<Card>,
}

pub fn all_theories() -> Vec<ElementTheory> {
    vec![
        ElementTheory {
            name: "A",
            label: "Classical",
            hypothesis: "Baseline — mineral-heavy, may lack diversity",
            elements: vec![
                Card::material("Earth", "Rich brown soil"),
                Card::material("Water", "Clear flowing liquid"),
                Card::material("Fire", "Hot roaring flames"),
                Card::material("Wind", "Invisible rushing air"),
                Card::material("Wood", "Sturdy fibrous timber"),
                Card::material("Stone", "Hard grey rock"),
                Card::material("Metal", "Shiny solid ore"),
                Card::material("Sand", "Fine granules of rock"),
                Card::material("Ice", "Frozen solid water"),
                Card::material("Crystal", "Translucent gemstone facets"),
                Card::material("Clay", "Soft wet moldable earth"),
                Card::material("Seed", "Tiny plant embryo"),
            ],
        },
        ElementTheory {
            name: "E",
            label: "Four Kingdoms",
            hypothesis: "Balanced mineral/plant/animal/energy",
            elements: vec![
                Card::material("Fire", "Hot roaring flames"),
                Card::material("Water", "Clear flowing liquid"),
                Card::material("Stone", "Hard grey rock"),
                Card::material("Metal", "Shiny solid ore"),
                Card::material("Sand", "Fine granules of rock"),
                Card::material("Crystal", "Translucent gemstone facets"),
                Card::material("Wood", "Sturdy fibrous timber"),
                Card::material("Vine", "Twisting green tendril"),
                Card::material("Seed", "Tiny plant embryo"),
                Card::material("Bone", "Hard white animal remains"),
                Card::material("Feather", "Light fluffy plume"),
                Card::material("Silk", "Smooth lustrous thread"),
            ],
        },
        ElementTheory {
            name: "F",
            label: "Workshop Bench",
            hypothesis: "Medieval crafter, no abstract forces",
            elements: vec![
                Card::material("Iron", "Dark heavy metal ingot"),
                Card::material("Leather", "Tanned animal hide"),
                Card::material("Timber", "Rough-cut wooden plank"),
                Card::material("Rope", "Twisted fibrous cord"),
                Card::material("Wax", "Soft pale waxy lump"),
                Card::material("Clay", "Soft wet moldable earth"),
                Card::material("Flint", "Sharp chippable stone"),
                Card::material("Charcoal", "Blackened burnt wood"),
                Card::material("Glass", "Clear brittle solid"),
                Card::material("Thread", "Thin spun fiber strand"),
                Card::material("Oil", "Slick dark liquid fuel"),
                Card::material("Bone", "Hard white animal remains"),
            ],
        },
        ElementTheory {
            name: "G",
            label: "Primal + Organic",
            hypothesis: "Forces + minerals + organics triad",
            elements: vec![
                Card::material("Fire", "Hot roaring flames"),
                Card::material("Water", "Clear flowing liquid"),
                Card::material("Wind", "Invisible rushing air"),
                Card::material("Light", "Bright radiant energy"),
                Card::material("Stone", "Hard grey rock"),
                Card::material("Metal", "Shiny solid ore"),
                Card::material("Clay", "Soft wet moldable earth"),
                Card::material("Wood", "Sturdy fibrous timber"),
                Card::material("Bone", "Hard white animal remains"),
                Card::material("Fiber", "Raw stringy plant material"),
                Card::material("Egg", "Oval shell full of yolk"),
                Card::material("Seed", "Tiny plant embryo"),
            ],
        },
        ElementTheory {
            name: "H",
            label: "Unusual Starters",
            hypothesis: "Unusual/specific = more surprising?",
            elements: vec![
                Card::material("Honey", "Thick golden sweet syrup"),
                Card::material("Rust", "Crumbly orange corroded metal"),
                Card::material("Smoke", "Wispy grey rising haze"),
                Card::material("Coral", "Hard pink sea skeleton"),
                Card::material("Amber", "Warm golden fossilized resin"),
                Card::material("Moss", "Soft green creeping fuzz"),
                Card::material("Obsidian", "Glossy black volcanic glass"),
                Card::material("Tar", "Thick black sticky goo"),
                Card::material("Quartz", "Clear hard mineral point"),
                Card::material("Pollen", "Fine yellow flower dust"),
                Card::material("Shell", "Hard curved sea casing"),
                Card::material("Charcoal", "Blackened burnt wood"),
            ],
        },
    ]
}

// ---------- Modifier families ----------

pub struct ModifierFamily {
    pub name: &'static str,
    pub hypothesis: &'static str,
    pub modifiers: Vec<Card>,
}

pub fn all_modifier_families() -> Vec<ModifierFamily> {
    vec![
        ModifierFamily {
            name: "Evocative",
            hypothesis: "Thematic words that feel like game-world concepts",
            modifiers: vec![
                Card::intent("Forge", "crafted metal objects"),
                Card::intent("Harmony", "music and balance"),
                Card::intent("Wild", "untamed nature"),
                Card::intent("Spark", "ignition and invention"),
                Card::intent("Ancient", "old and weathered things"),
                Card::intent("Delicate", "fragile refined things"),
            ],
        },
        ModifierFamily {
            name: "Labels",
            hypothesis: "Clear intent, less flavorful",
            modifiers: vec![
                Card::intent("Functional", "practical useful objects"),
                Card::intent("Musical", "instruments and sound"),
                Card::intent("Natural", "organic living things"),
                Card::intent("Technical", "engineered mechanisms"),
                Card::intent("Historical", "ancient artifacts"),
                Card::intent("Fragile", "breakable delicate things"),
            ],
        },
        ModifierFamily {
            name: "Emotions",
            hypothesis: "Emotional coloring might produce surprising/fun results",
            modifiers: vec![
                Card::intent("Happy", "joy and celebration"),
                Card::intent("Scared", "fear and defense"),
                Card::intent("Angry", "aggression and destruction"),
                Card::intent("Peaceful", "calm and tranquility"),
                Card::intent("Curious", "exploration and discovery"),
                Card::intent("Proud", "honor and display"),
            ],
        },
        ModifierFamily {
            name: "Actions",
            hypothesis: "Verb-driven, implies what to do with materials",
            modifiers: vec![
                Card::intent("Build", "construction and assembly"),
                Card::intent("Play", "games and entertainment"),
                Card::intent("Fight", "weapons and conflict"),
                Card::intent("Grow", "growth and cultivation"),
                Card::intent("Shine", "brightness and polish"),
                Card::intent("Break", "destruction and fragments"),
            ],
        },
        ModifierFamily {
            name: "Sensory",
            hypothesis: "Physical properties that steer the output",
            modifiers: vec![
                Card::intent("Loud", "noise and vibration"),
                Card::intent("Bright", "light and visibility"),
                Card::intent("Soft", "gentle textures"),
                Card::intent("Sharp", "cutting edges and points"),
                Card::intent("Sweet", "pleasant flavors and aromas"),
                Card::intent("Cold", "chill and preservation"),
            ],
        },
    ]
}

// ---------- Sensory modifier variations ----------

pub fn sensory_variations() -> Vec<ModifierFamily> {
    vec![
        ModifierFamily {
            name: "Sensory-A (Original)",
            hypothesis: "Physical properties that steer the output",
            modifiers: vec![
                Card::intent("Loud", "noise and vibration"),
                Card::intent("Bright", "light and visibility"),
                Card::intent("Soft", "gentle textures"),
                Card::intent("Sharp", "cutting edges and points"),
                Card::intent("Sweet", "pleasant flavors and aromas"),
                Card::intent("Cold", "chill and preservation"),
            ],
        },
        ModifierFamily {
            name: "Sensory-B (Intensity)",
            hypothesis: "Force and weight properties",
            modifiers: vec![
                Card::intent("Fierce", "aggressive forceful energy"),
                Card::intent("Gentle", "calm careful handling"),
                Card::intent("Heavy", "weight and solidity"),
                Card::intent("Hollow", "empty interior spaces"),
                Card::intent("Dense", "compressed tightly packed"),
                Card::intent("Flowing", "movement and fluidity"),
            ],
        },
        ModifierFamily {
            name: "Sensory-C (Temperature)",
            hypothesis: "Temperature and state transitions",
            modifiers: vec![
                Card::intent("Hot", "high temperature and heat"),
                Card::intent("Cool", "low temperature and chill"),
                Card::intent("Wet", "moisture and dampness"),
                Card::intent("Dry", "absence of moisture"),
                Card::intent("Solid", "rigidity and firmness"),
                Card::intent("Brittle", "fragile and breakable"),
            ],
        },
        ModifierFamily {
            name: "Sensory-D (Texture)",
            hypothesis: "Surface and structural properties",
            modifiers: vec![
                Card::intent("Hard", "resistance and durability"),
                Card::intent("Flexible", "bending without breaking"),
                Card::intent("Thin", "slender and narrow"),
                Card::intent("Thick", "wide and substantial"),
                Card::intent("Smooth", "even polished surfaces"),
                Card::intent("Rough", "coarse uneven surfaces"),
            ],
        },
        ModifierFamily {
            name: "Sensory-E (Nature)",
            hypothesis: "Natural sensory qualities",
            modifiers: vec![
                Card::intent("Warm", "comfortable gentle heat"),
                Card::intent("Silent", "quiet and still"),
                Card::intent("Fragrant", "pleasant natural scent"),
                Card::intent("Bitter", "harsh pungent taste"),
                Card::intent("Crackling", "snapping popping sounds"),
                Card::intent("Glowing", "soft light emission"),
            ],
        },
    ]
}

// ---------- Theory G elements accessor ----------

pub fn theory_g_elements() -> Vec<Card> {
    all_theories()
        .into_iter()
        .find(|t| t.name == "G")
        .expect("Theory G not found")
        .elements
}

// ---------- Baseline element set for step 1 ----------

pub fn baseline_elements() -> Vec<Card> {
    all_theories().remove(0).elements
}

// ---------- Sample pairs for modifier testing ----------

/// Returns 15 diverse pairs from the baseline set for modifier comparison.
pub fn sample_pairs(elements: &[Card]) -> Vec<(Card, Card)> {
    let n = elements.len();
    let mut all_pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            all_pairs.push((elements[i].clone(), elements[j].clone()));
        }
    }

    let total = all_pairs.len();
    let step = total as f64 / 15.0;
    (0..15)
        .map(|k| {
            let idx = (k as f64 * step) as usize;
            all_pairs[idx].clone()
        })
        .collect()
}

// ---------- Board categories ----------

pub const BOARD_CATEGORIES: &[&str] = &[
    "Weapon",
    "Armor",
    "Shield",
    "Tool",
    "Container",
    "Light Source",
    "Musical Instrument",
    "Clothing",
    "Food/Drink",
    "Building Material",
    "Shelter",
    "Transport",
    "Art/Decoration",
    "Medicine/Healing",
    "Trap/Hazard",
    "Signal/Communication",
    "Fuel/Energy",
    "Rope/Binding",
    "Lens/Optics",
    "Writing/Record",
    "Currency/Trade",
    "Hunting/Fishing",
    "Agriculture",
    "Navigation",
    "Ritual/Ceremonial",
];

// ---------- Target items checklist ----------

pub const TARGET_ITEMS: &[(&str, &[&str])] = &[
    ("Weapons", &["Sword", "Blade", "Spear", "Arrow", "Bow"]),
    ("Transport", &["Boat", "Raft", "Cart", "Sled"]),
    ("Shelter", &["Tent", "Hut", "Wall", "Brick"]),
    ("Music", &["Drum", "Flute", "Bell", "Lute"]),
    (
        "Useful",
        &[
            "Lens", "Candle", "Lantern", "Rope", "Pottery", "Leather", "Armor",
        ],
    ),
];
