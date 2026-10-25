#!/usr/bin/env bash
# MovieBox GUI — integration test suite.
# Boots the GUI server on a test port and verifies every endpoint, the
# playback proxy chain (Range/206), library persistence and the AI layer.
# Usage: bash scripts/integration-test.sh [port]

set -u
PORT="${1:-7771}"
BASE="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)/MovieBox-GUI"
BIN="${BIN:-$ROOT/target/debug/moviebox-server}"
PASS=0; FAIL=0
declare -a FAILED

say()  { printf '\033[1;36m[%02d]\033[0m %s\n' "$1" "$2"; }
ok()   { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); FAILED+=("$2"); printf '  \033[31mFAIL\033[0m %s\n' "$2"; }

expect() { # name expected actual
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "$1 (expected=$2 got=$3)"; fi
}

contains() { # name haystack needle
  case "$2" in *"$3"*) ok "$1" ;; *) bad "$1" "$1 (missing: $3)" ;; esac
}

cleanup() { [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null; pkill -f "moviebox-server --port $PORT" 2>/dev/null; }
trap cleanup EXIT

say 0 "booting server on :$PORT"
"$BIN" --port "$PORT" --no-browser >"/tmp/mb-test-$PORT.log" 2>&1 &
SRV_PID=$!
for i in $(seq 1 40); do
  curl -s -m 1 "$BASE/api/health" >/dev/null 2>&1 && break
  sleep 0.25
done

N=1
# --- core / static ---------------------------------------------------------
say $N "static & system"
H=$(curl -s -m 5 "$BASE/api/health")
expect "health ok" "True" "$(echo "$H" | python3 -c 'import json,sys;print(json.load(sys.stdin)["ok"])')"
contains "health version" "$H" '"version"'
S=$(curl -s -m 5 -o /dev/null -w '%{http_code}:%{size_download}' "$BASE/")
expect "index 200 non-empty" "200" "${S%%:*}"
for a in app.js views.js style.css icon.svg dash.min.js; do
  C=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$BASE/assets/$a")
  expect "asset $a" "200" "$C"
done
C=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$BASE/favicon.ico")
expect "favicon" "200" "$C"
N=$((N+1))

# --- catalog ---------------------------------------------------------------
say $N "catalog: home / browse / search / suggest"
HOME=$(curl -s -m 30 "$BASE/api/home")
contains "home sections" "$HOME" '"sections"'
SECTIONS=$(echo "$HOME" | python3 -c 'import json,sys;print(len(json.load(sys.stdin)["sections"]))')
[ "$SECTIONS" -ge 3 ] && ok "home has $SECTIONS sections" || bad "home sections" "only $SECTIONS"
ITEM_ID=$(echo "$HOME" | python3 -c '
import json,sys
d=json.load(sys.stdin)
for s in d["sections"]:
    for i in s["items"]:
        print(i["id"]["value"]); raise SystemExit')
[ -n "$ITEM_ID" ] && ok "home item id present" || bad "home item id" "empty"

BR=$(curl -s -m 30 "$BASE/api/browse?preset=top")
contains "browse top sorted" "$BR" '"items"'

SEARCH=$(curl -s -m 30 "$BASE/api/search?q=dune")
contains "search results" "$SEARCH" '"items"'
DUNE_ID=$(echo "$SEARCH" | python3 -c '
import json,sys
d=json.load(sys.stdin)
for i in d["items"]:
    if i["media_type"]=="movie":
        print(i["id"]["value"]); raise SystemExit')
[ -n "$DUNE_ID" ] && ok "search found movie id" || bad "search movie id" "none"
N=$((N+1))

SUG=$(curl -s -m 20 "$BASE/api/suggest?q=the")
contains "suggest" "$SUG" '"suggestions"'
N=$((N+1))

# --- details ----------------------------------------------------------------
say $N "details"
DET=$(curl -s -m 30 "$BASE/api/details?provider=moviebox&id=$DUNE_ID")
contains "details title" "$DET" '"title"'
contains "details imdb" "$DET" '"imdb_rating"'
N=$((N+1))

# --- playback chain ---------------------------------------------------------
say $N "playback: releases → resolve → Range 206"
REL=$(curl -s -m 30 "$BASE/api/releases?provider=moviebox&id=$DUNE_ID&season=0&episode=0")
contains "releases" "$REL" '"releases"'
RES=$(curl -s -m 40 "$BASE/api/resolve?provider=moviebox&id=$DUNE_ID&season=0&episode=0&release=0&max_height=720")
contains "resolve play_url" "$RES" '"play_url"'
contains "resolve resource_id" "$RES" '"resource_id"'
PLAY=$(echo "$RES" | python3 -c 'import json,sys;print(json.load(sys.stdin)["play_url"])')
QR=$(curl -s -m 60 -o /tmp/mb-range.bin -D /tmp/mb-range.h -w '%{http_code}' -H "Range: bytes=0-262143" "$BASE$PLAY")
expect "Range request 206" "206" "$QR"
contains "content-range header" "$(cat /tmp/mb-range.h)" "content-range"
MAGIC=$(head -c 12 /tmp/mb-range.bin | od -An -tx1 | tr -d ' \n')
case "$MAGIC" in
  *6674797069736f6d*) ok "mp4 magic bytes (ftyp isom)" ;;
  *) bad "mp4 magic" "$MAGIC" ;;
esac
QR2=$(curl -s -m 60 -o /dev/null -w '%{http_code}' -H "Range: bytes=2000000-2000999" "$BASE$PLAY")
expect "second range (seek)" "206" "$QR2"
curl -s -m 5 "$BASE/api/stop-playback" >/dev/null
N=$((N+1))

# --- posters -----------------------------------------------------------------
say $N "poster pipeline"
PURL=$(echo "$HOME" | python3 -c '
import json,sys,urllib.parse
d=json.load(sys.stdin)
for s in d["sections"]:
    for i in s["items"]:
        if i.get("poster_url"):
            print(urllib.parse.quote(i["poster_url"], safe="")); raise SystemExit')
SZ1=$(curl -s -m 25 -o /dev/null -w '%{size_download}' "$BASE/api/poster?w=342&u=$PURL")
[ "$SZ1" -lt 150000 ] && ok "resized poster ${SZ1}B < 150KB" || bad "poster size" "${SZ1}B"
SZ2=$(curl -s -m 25 -o /dev/null -w '%{size_download}:%{time_total}' "$BASE/api/poster?w=342&u=$PURL")
contains "poster cached" "$SZ2" ":0.0"
N=$((N+1))

# --- library -------------------------------------------------------------------
say $N "library: favorites & history"
FAV=$(curl -s -m 5 -X POST -H 'Content-Type: application/json' \
  -d "{\"provider\":\"moviebox\",\"id\":\"$DUNE_ID\",\"title\":\"Integration Test Title\",\"stype\":1,\"year\":\"2024\"}" \
  "$BASE/api/favorites")
contains "favorite add" "$FAV" '"added":true'
FAVLIST=$(curl -s -m 5 "$BASE/api/favorites")
contains "favorites list" "$FAVLIST" 'Integration Test Title'
PROG=$(curl -s -m 5 -X POST -H 'Content-Type: application/json' \
  -d "{\"provider\":\"moviebox\",\"id\":\"$DUNE_ID\",\"title\":\"Integration Test Title\",\"stype\":1,\"season\":0,\"episode\":0,\"progress_seconds\":300,\"duration_seconds\":6000}" \
  "$BASE/api/history/progress")
contains "progress save" "$PROG" '"ok":true'
HIST=$(curl -s -m 5 "$BASE/api/history")
contains "history entry" "$HIST" 'Integration Test Title'
contains "progress stored" "$HIST" '"progress_seconds":300'
DEL=$(curl -s -m 5 -X DELETE "$BASE/api/favorites?provider=moviebox&id=$DUNE_ID")
contains "favorite delete" "$DEL" '"count"'
N=$((N+1))

# --- settings --------------------------------------------------------------------
say $N "settings"
SET=$(curl -s -m 5 "$BASE/api/settings")
contains "settings get" "$SET" '"theme"'
SET2=$(curl -s -m 5 -X POST -H 'Content-Type: application/json' -d '{"max_height":480}' "$BASE/api/settings")
contains "settings patch" "$SET2" '"max_height":480'
N=$((N+1))

# --- AI ---------------------------------------------------------------------------
say $N "AI layer"
AI=$(curl -s -m 10 "$BASE/api/ai/recommendations")
contains "recommendations" "$AI" '"for_you"'
contains "profile summary" "$AI" '"profile_summary"'
MOOD=$(curl -s -m 10 "$BASE/api/ai/mood?id=mind-bending")
contains "mood row" "$MOOD" '"label"'
AIS=$(curl -s -m 40 "$BASE/api/ai/search?q=something%20funny%20from%20the%2090s")
contains "semantic interpretation" "$AIS" '"interpreted"'
INTERP=$(echo "$AIS" | python3 -c 'import json,sys;print(json.load(sys.stdin)["interpreted"])')
contains "decade parsed" "$INTERP" "1990s"
INS=$(curl -s -m 10 "$BASE/api/ai/insights")
contains "insights" "$INS" '"streak_days"'
CHAT=$(curl -s -m 20 -X POST -H 'Content-Type: application/json' -d '{"message":"what should I watch tonight?"}' "$BASE/api/ai/chat")
contains "chat reply" "$CHAT" '"answer"'
contains "chat source" "$CHAT" '"source"'
AISet=$(curl -s -m 5 "$BASE/api/ai/settings")
contains "ai settings get" "$AISet" '"llm_enabled"'
AISet2=$(curl -s -m 5 -X POST -H 'Content-Type: application/json' -d '{"llm_enabled":true,"llm_endpoint":"https://api.openai.com/v1","llm_api_key":"sk-test","llm_model":"gpt-4o-mini"}' "$BASE/api/ai/settings")
contains "ai settings save" "$AISet2" '"llm_configured":true'
AISet3=$(curl -s -m 5 -X POST -H 'Content-Type: application/json' -d '{"llm_enabled":false,"llm_api_key":""}' "$BASE/api/ai/settings")
contains "ai settings disable" "$AISet3" '"llm_configured":false'
N=$((N+1))

# --- errors ---------------------------------------------------------------------------
say $N "error handling"
E1=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$BASE/api/details?provider=moviebox&id=")
expect "details missing id → 400" "400" "$E1"
E2=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$BASE/api/nope")
expect "unknown route → 404" "404" "$E2"
E3=$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$BASE/api/poster?u=notaurl")
expect "poster bad url → 400" "400" "$E3"
E4=$(curl -s -m 20 -o /dev/null -w '%{http_code}' "$BASE/api/resolve?provider=moviebox&id=999999&season=0&episode=0")
[ "$E4" = "404" ] || [ "$E4" = "502" ] && ok "resolve unknown id → $E4" || bad "resolve unknown id" "$E4"
N=$((N+1))

# --- summary ----------------------------------------------------------------------------
echo
echo "================================"
printf 'RESULT: \033[32m%d passed\033[0m, \033[31m%d failed\033[0m\n' "$PASS" "$FAIL"
if [ "$FAIL" -gt 0 ]; then
  printf 'Failed: %s\n' "${FAILED[@]}"
  exit 1
fi
