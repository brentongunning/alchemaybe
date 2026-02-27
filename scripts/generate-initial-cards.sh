#!/usr/bin/env bash
# Generate card images for all starter cards in cards.json
# Requires: game server running on :3001, generation server running on :3000, jq

set -uo pipefail

GAME_SERVER="http://localhost:3001"
OUTPUT_DIR="cards"
MAX_RETRIES=3
RETRY_DELAY=2

mkdir -p "$OUTPUT_DIR/materials" "$OUTPUT_DIR/intents"

echo "Generating card images..."
echo "Game server: $GAME_SERVER"
echo "Output: $OUTPUT_DIR/"
echo

generate_card() {
    local name="$1" desc="$2" kind="$3" outfile="$4"

    if [ -f "$outfile" ]; then
        echo "  [cached] $name"
        return 0
    fi

    for attempt in $(seq 1 $MAX_RETRIES); do
        echo -n "  Generating $name (attempt $attempt)..."
        http_code=$(curl -s -o "$outfile" -w "%{http_code}" --max-time 120 \
            -X POST "$GAME_SERVER/generate-card" \
            -H "Content-Type: application/json" \
            -d "$(jq -n --arg n "$name" --arg d "$desc" --arg k "$kind" '{name: $n, description: $d, kind: $k}')")

        if [ "$http_code" -eq 200 ] && [ -s "$outfile" ]; then
            size=$(wc -c < "$outfile" | tr -d ' ')
            echo " done (${size} bytes)"
            return 0
        fi

        echo " FAILED (HTTP $http_code)"
        rm -f "$outfile"

        if [ "$attempt" -lt "$MAX_RETRIES" ]; then
            echo "    Retrying in ${RETRY_DELAY}s..."
            sleep "$RETRY_DELAY"
        fi
    done

    echo "  ERROR: Failed to generate $name after $MAX_RETRIES attempts"
    return 0
}

# Generate material cards
failed=0
count=$(jq '.materials | length' cards.json)
for i in $(seq 0 $((count - 1))); do
    name=$(jq -r ".materials[$i].name" cards.json)
    desc=$(jq -r ".materials[$i].description" cards.json)
    generate_card "$name" "$desc" "material" "$OUTPUT_DIR/materials/${name}.png"
done

echo

# Generate intent cards
count=$(jq '.intents | length' cards.json)
for i in $(seq 0 $((count - 1))); do
    name=$(jq -r ".intents[$i].name" cards.json)
    desc=$(jq -r ".intents[$i].description" cards.json)
    generate_card "$name" "$desc" "intent" "$OUTPUT_DIR/intents/${name}.png"
done

echo
echo "Done! Cards saved to $OUTPUT_DIR/"
ls -la "$OUTPUT_DIR/materials/" "$OUTPUT_DIR/intents/"
