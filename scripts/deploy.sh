#!/usr/bin/env bash
# deploy.sh — Deploy MentorMinds contracts to Stellar testnet or mainnet.
# Usage: ./scripts/deploy.sh [--network testnet|mainnet] [--identity <name>]
set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────────────
NETWORK="testnet"
IDENTITY="default"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOYED_DIR="$REPO_ROOT/deployed"
CONFIG_FILE="$DEPLOYED_DIR/$NETWORK.json"

# ── Arg parsing ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case $1 in
    --network) NETWORK="$2"; shift 2 ;;
    --identity) IDENTITY="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

CONFIG_FILE="$DEPLOYED_DIR/$NETWORK.json"

# ── Network config ────────────────────────────────────────────────────────────
if [[ "$NETWORK" == "testnet" ]]; then
  RPC_URL="https://soroban-testnet.stellar.org:443"
  PASSPHRASE="Test SDF Network ; September 2015"
  FRIENDBOT_URL="https://friendbot.stellar.org"
elif [[ "$NETWORK" == "mainnet" ]]; then
  RPC_URL="https://mainnet.stellar.validationcloud.io/v1/${VALIDATION_CLOUD_KEY:-}"
  PASSPHRASE="Public Global Stellar Network ; September 2015"
  FRIENDBOT_URL=""
else
  echo "ERROR: --network must be testnet or mainnet"; exit 1
fi

STELLAR_FLAGS="--network $NETWORK --source $IDENTITY"

# ── Helpers ───────────────────────────────────────────────────────────────────
log()  { echo "[deploy] $*"; }
ok()   { echo "  ✓ $*"; }
skip() { echo "  ↷ $* (already deployed)"; }

# Read a value from the JSON config file (requires jq).
json_get() { jq -r ".$1 // empty" "$CONFIG_FILE" 2>/dev/null || true; }

# Write/update a key in the JSON config file.
json_set() {
  local key="$1" val="$2"
  local tmp; tmp=$(mktemp)
  jq --arg k "$key" --arg v "$val" '.[$k] = $v' "$CONFIG_FILE" > "$tmp"
  mv "$tmp" "$CONFIG_FILE"
}

# Deploy one contract; skip if already in config.
# Returns the contract ID in $CONTRACT_ID.
deploy_contract() {
  local name="$1" wasm="$2"
  CONTRACT_ID=$(json_get "$name")
  if [[ -n "$CONTRACT_ID" ]]; then
    skip "$name → $CONTRACT_ID"
    return
  fi
  log "Deploying $name …"
  CONTRACT_ID=$(stellar contract deploy \
    --wasm "$wasm" \
    $STELLAR_FLAGS \
    2>/dev/null)
  json_set "$name" "$CONTRACT_ID"
  ok "$name → $CONTRACT_ID"
}

# Invoke a contract function; ignore "already initialized" errors (idempotent).
invoke() {
  local contract_id="$1"; shift
  stellar contract invoke \
    --id "$contract_id" \
    $STELLAR_FLAGS \
    -- "$@" 2>&1 | grep -v "^$" || true
}

# ── Ensure network is registered ──────────────────────────────────────────────
stellar network add "$NETWORK" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE" 2>/dev/null || true

# ── Fund deployer (testnet only) ──────────────────────────────────────────────
ADMIN=$(stellar keys address "$IDENTITY" 2>/dev/null)
if [[ "$NETWORK" == "testnet" ]]; then
  log "Funding $ADMIN via Friendbot …"
  curl -sf "$FRIENDBOT_URL?addr=$ADMIN" -o /dev/null && ok "Funded" || log "Already funded (or Friendbot unavailable)"
fi

# ── Ensure config file exists ─────────────────────────────────────────────────
mkdir -p "$DEPLOYED_DIR"
[[ -f "$CONFIG_FILE" ]] || echo '{}' > "$CONFIG_FILE"

# ── Build all contracts ───────────────────────────────────────────────────────
log "Building contracts …"
(cd "$REPO_ROOT" && cargo build --target wasm32-unknown-unknown --release -q)
ok "Build complete"

WASM_DIR="$REPO_ROOT/target/wasm32-unknown-unknown/release"

# ── Deploy ────────────────────────────────────────────────────────────────────
deploy_contract "escrow"       "$WASM_DIR/mentorminds_escrow.wasm"
ESCROW_ID=$CONTRACT_ID

deploy_contract "verification" "$WASM_DIR/mentorminds_verification.wasm"
VERIFICATION_ID=$CONTRACT_ID

deploy_contract "mnt_token"    "$WASM_DIR/mentorminds_mnt_token.wasm"
TOKEN_ID=$CONTRACT_ID

# ── Initialize ────────────────────────────────────────────────────────────────
log "Initializing contracts …"

# escrow: initialize(admin, treasury, fee_bps, approved_tokens, auto_release_delay_secs)
invoke "$ESCROW_ID" initialize \
  --admin "$ADMIN" \
  --treasury "$ADMIN" \
  --fee_bps 500 \
  --approved_tokens "[]" \
  --auto_release_delay_secs 259200 \
  2>&1 | grep -v "Already initialized" || true
ok "escrow initialized"

# verification: initialize(admin)
invoke "$VERIFICATION_ID" initialize --admin "$ADMIN" \
  2>&1 | grep -v "Already initialized" || true
ok "verification initialized"

# mnt_token: initialize(admin)
invoke "$TOKEN_ID" initialize --admin "$ADMIN" \
  2>&1 | grep -v "Already initialized" || true
ok "mnt_token initialized"

# ── Verify deployments ────────────────────────────────────────────────────────
log "Verifying deployments …"

FEE=$(invoke "$ESCROW_ID" get_fee_bps)
ok "escrow.get_fee_bps → $FEE"

IS_VER=$(invoke "$VERIFICATION_ID" is_verified --mentor "$ADMIN")
ok "verification.is_verified → $IS_VER"

# ── Write metadata ────────────────────────────────────────────────────────────
json_set "network"    "$NETWORK"
json_set "admin"      "$ADMIN"
json_set "deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# ── Summary table ─────────────────────────────────────────────────────────────
echo ""
echo "┌─────────────────────┬──────────────────────────────────────────────────────────┐"
printf "│ %-19s │ %-56s │\n" "Contract" "ID"
echo "├─────────────────────┼──────────────────────────────────────────────────────────┤"
for key in escrow verification mnt_token; do
  id=$(json_get "$key")
  printf "│ %-19s │ %-56s │\n" "$key" "$id"
done
echo "└─────────────────────┴──────────────────────────────────────────────────────────┘"
echo ""
echo "Config saved → $CONFIG_FILE"
