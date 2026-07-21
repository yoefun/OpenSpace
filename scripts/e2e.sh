#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-18080}"
DATA="${OPENSPACE_DATA:-/tmp/openspace-e2e-data}"
rm -rf "$DATA"
mkdir -p "$DATA"

if [[ ! -x ./target/release/openspace-api ]]; then
  cargo build -p openspace-api --release
fi
if [[ ! -f web/dist/index.html ]]; then
  (cd web && npm install && npm run build)
fi

OPENSPACE_DATA="$DATA" OPENSPACE_WEB=web/dist PORT="$PORT" \
  ./target/release/openspace-api > /tmp/openspace-e2e.log 2>&1 &
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT
sleep 1

curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null
RESP=$(curl -sf -F "name=E2E" -F "floorplan=@fixtures/floorplans/synthetic_ortho.png" \
  "http://127.0.0.1:${PORT}/api/projects")
ID=$(python3 -c "import json,sys; print(json.load(sys.stdin)['id'])" <<<"$RESP")
curl -sf -X POST "http://127.0.0.1:${PORT}/api/projects/${ID}/detect" >/dev/null
curl -sf -F "room_name=Room 1" -F "photo=@fixtures/photos/living_red.png" \
  "http://127.0.0.1:${PORT}/api/projects/${ID}/photos" >/dev/null
curl -sf -X POST "http://127.0.0.1:${PORT}/api/projects/${ID}/build" >/dev/null

for _ in $(seq 1 40); do
  ST=$(curl -sf "http://127.0.0.1:${PORT}/api/projects/${ID}" | python3 -c "import json,sys; print(json.load(sys.stdin)['status'])")
  [[ "$ST" == "ready" ]] && break
  [[ "$ST" == "failed" ]] && { echo "build failed"; exit 1; }
  sleep 0.25
done

curl -sf -o /tmp/openspace-e2e.glb "http://127.0.0.1:${PORT}/api/projects/${ID}/model.glb"
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:${PORT}/api/projects/${ID}/design")
[[ "$CODE" == "501" ]] || { echo "expected design 501 got $CODE"; exit 1; }
SIZE=$(wc -c </tmp/openspace-e2e.glb)
echo "e2e ok: project=$ID glb_bytes=$SIZE design=$CODE"
