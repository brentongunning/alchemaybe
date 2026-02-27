#!/usr/bin/env node
/**
 * Continuously generate crafted cards by combining materials and intents.
 * Runs until stopped (Ctrl+C).
 *
 * Requires both servers running (make run). Calls the generation server for
 * combine + image, then the game server for card rendering. Saves results
 * to cards/card-cache.json with discovered=false.
 *
 * Usage:
 *   node scripts/generate-cards.js
 */

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const GENERATION_URL = process.env.GENERATION_URL || "http://localhost:3000";
const GAME_URL = process.env.GAME_URL || "http://localhost:3001";
const CARDS_JSON = "cards.json";
const CACHE_PATH = "cards/card-cache.json";
const CRAFTED_DIR = "cards/crafted";

function loadCards() {
  const data = JSON.parse(fs.readFileSync(CARDS_JSON, "utf8"));
  return { materials: data.materials, intents: data.intents };
}

function loadCache() {
  try {
    return JSON.parse(fs.readFileSync(CACHE_PATH, "utf8"));
  } catch {
    return { entries: {} };
  }
}

function saveCache(cache) {
  fs.mkdirSync(path.dirname(CACHE_PATH), { recursive: true });
  fs.writeFileSync(CACHE_PATH, JSON.stringify(cache, null, 2));
}

function computeBaseCardId(name) {
  return crypto.createHash("sha256").update(name.toLowerCase()).digest("hex").slice(0, 12);
}

function computeCraftedCardId(materialIds, intentId) {
  const sorted = [...materialIds].sort();
  let key = sorted.join("+");
  if (intentId) key += `+[${intentId}]`;
  return crypto.createHash("sha256").update(key).digest("hex").slice(0, 12);
}

function pickRandom(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}

function pickN(arr, n) {
  const shuffled = [...arr].sort(() => Math.random() - 0.5);
  return shuffled.slice(0, n);
}

/**
 * Generate a random combination to try.
 * Weighted distribution:
 *   30% — 1 material + 1 intent
 *   30% — 2 materials + 1 intent
 *   15% — 3 materials + 1 intent
 *   10% — 2 materials (no intent)
 *   10% — 3 materials (no intent)
 *    5% — 4 materials (no intent)
 */
function randomCombo(materials, intents) {
  const roll = Math.random();

  if (roll < 0.3) {
    // 1 material + 1 intent
    return { mats: pickN(materials, 1), intent: pickRandom(intents) };
  } else if (roll < 0.6) {
    // 2 materials + 1 intent
    return { mats: pickN(materials, 2), intent: pickRandom(intents) };
  } else if (roll < 0.75) {
    // 3 materials + 1 intent
    return { mats: pickN(materials, 3), intent: pickRandom(intents) };
  } else if (roll < 0.85) {
    // 2 materials, no intent
    return { mats: pickN(materials, 2), intent: null };
  } else if (roll < 0.95) {
    // 3 materials, no intent
    return { mats: pickN(materials, 3), intent: null };
  } else {
    // 4 materials, no intent
    return { mats: pickN(materials, 4), intent: null };
  }
}

async function tryCombine(mats, intent) {
  const cards = mats.map((m) => ({
    name: m.name,
    description: m.description,
    kind: "material",
  }));
  if (intent) {
    cards.push({
      name: intent.name,
      description: intent.description,
      kind: "intent",
    });
  }

  try {
    const resp = await fetch(`${GENERATION_URL}/combine`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cards }),
    });
    if (!resp.ok) return null;
    const data = await resp.json();
    if (data.name.toLowerCase().includes("not possible")) return null;
    return { name: data.name, description: data.description };
  } catch (e) {
    console.log(`  Combine error: ${e.message}`);
    return null;
  }
}

async function generateImage(name, description) {
  try {
    const resp = await fetch(`${GENERATION_URL}/generate-image`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, description }),
    });
    if (!resp.ok) return null;
    return Buffer.from(await resp.arrayBuffer());
  } catch (e) {
    console.log(`  Image error: ${e.message}`);
    return null;
  }
}

async function saveCardImage(name, cardId, artBytes) {
  fs.mkdirSync(CRAFTED_DIR, { recursive: true });
  const safeName = name
    .replace(/[^a-zA-Z0-9 -]/g, "_")
    .replace(/ /g, "-");
  const filename = `${safeName}-${cardId}.png`;
  const diskPath = path.join(CRAFTED_DIR, filename);

  // Call game server to render card frame around the art
  try {
    const resp = await fetch(`${GAME_URL}/generate-card`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, description: "" }),
    });
    if (resp.ok) {
      fs.writeFileSync(diskPath, Buffer.from(await resp.arrayBuffer()));
      return `/cards/crafted/${filename}`;
    }
  } catch {}

  // Fallback: save raw art
  fs.writeFileSync(diskPath, artBytes);
  return `/cards/crafted/${filename}`;
}

async function main() {
  const { materials, intents } = loadCards();
  // Compute base card IDs
  for (const m of materials) m.id = computeBaseCardId(m.name);
  for (const i of intents) i.id = computeBaseCardId(i.name);
  const cache = loadCache();

  console.log(`Loaded ${materials.length} materials, ${intents.length} intents`);
  console.log(`Cache has ${Object.keys(cache.entries || {}).length} existing entries`);
  console.log(`Running continuously. Press Ctrl+C to stop.\n`);

  let success = 0;
  let notPossible = 0;
  let failed = 0;
  let skipped = 0;
  let attempt = 0;

  while (true) {
    attempt++;
    const { mats, intent } = randomCombo(materials, intents);
    const matIds = mats.map((m) => m.id);
    const intentId = intent ? intent.id : null;
    const key = computeCraftedCardId(matIds, intentId);

    const matNames = mats.map((m) => m.name);
    let label = matNames.join(" + ");
    if (intent) label += ` [${intent.name}]`;

    // Skip if already cached
    if (cache.entries && cache.entries[key]) {
      skipped++;
      continue;
    }

    console.log(`#${attempt} ${label}`);

    // Step 1: Combine
    const result = await tryCombine(mats, intent);
    if (!result) {
      console.log(`  Not possible`);
      if (!cache.entries) cache.entries = {};
      cache.entries[key] = {
        name: "Not possible",
        description: "",
        image_path: "",
        id: key,
        discovered: false,
        impossible: true,
      };
      saveCache(cache);
      notPossible++;
      console.log(`  [${success} ok, ${notPossible} impossible, ${failed} failed, ${skipped} skipped]`);
      continue;
    }

    console.log(`  -> ${result.name}: ${result.description}`);

    // Step 2: Generate image
    console.log(`  Generating image...`);
    const artBytes = await generateImage(result.name, result.description);
    if (!artBytes) {
      console.log(`  Image generation failed`);
      failed++;
      console.log(`  [${success} ok, ${notPossible} impossible, ${failed} failed, ${skipped} skipped]`);
      continue;
    }

    // Step 3: Render and save card
    console.log(`  Rendering card...`);
    const servePath = await saveCardImage(result.name, key, artBytes);

    // Step 4: Save to cache as undiscovered
    if (!cache.entries) cache.entries = {};
    cache.entries[key] = {
      name: result.name,
      description: result.description,
      image_path: servePath,
      id: key,
      discovered: false,
    };
    saveCache(cache);

    success++;
    console.log(`  Saved! (${servePath})`);
    console.log(`  [${success} ok, ${notPossible} impossible, ${failed} failed, ${skipped} skipped]`);
  }
}

main().catch(console.error);
