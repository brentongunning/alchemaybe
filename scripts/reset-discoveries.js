#!/usr/bin/env node
/**
 * Reset all cards in card-cache.json to discovered: false.
 *
 * Usage:
 *   node scripts/reset-discoveries.js
 */

const fs = require("fs");

const CACHE_PATH = "cards/card-cache.json";

try {
  const cache = JSON.parse(fs.readFileSync(CACHE_PATH, "utf8"));
  let count = 0;
  for (const key of Object.keys(cache.entries || {})) {
    if (cache.entries[key].discovered) {
      cache.entries[key].discovered = false;
      count++;
    }
  }
  fs.writeFileSync(CACHE_PATH, JSON.stringify(cache, null, 2));
  console.log(`Reset ${count} cards to undiscovered (${Object.keys(cache.entries || {}).length} total)`);
} catch (e) {
  console.error(`Error: ${e.message}`);
  process.exit(1);
}
