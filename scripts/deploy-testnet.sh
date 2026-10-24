#!/usr/bin/env bash
# deploy-testnet.sh — Automated deployment helper for Cypher GridPay contracts
# Deploys Payment, Escrow, Refund, and Admin contracts to Stellar Testnet.
# Usage: ./scripts/deploy-testnet.sh [network]
#   network: testnet (default) | futurenet

set -euo pipefail

NETWORK="${1:-testnet}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
MANIFEST_FILE="${REPO_ROOT}/deployment-manifest.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ── Prerequisites ──────────────────────────────────────────────────────────
command -v stellar >/dev/null 2>&1 || { log_error "Stellar CLI not found. Install: cargo install --locked stellar-cli --features opt"; exit 1; }
command -v jq >/dev/null 2>&1 || { log_error "jq not found. Install: brew install jq / apt-get install jq"; exit 1; }

# ── Configuration ──────────────────────────────────────────────────────────
ADMIN_ADDRESS="${ADMIN_ADDRESS:-}"
ADMIN_SECRET="${ADMIN_SECRET:-}"

if [[ -z "${ADMIN_ADDRESS}" ]]; then
    log_error "ADMIN_ADDRESS environment variable is required."
    exit 1
fi

if [[ -z "${ADMIN_SECRET}" ]]; then
    log_warn "ADMIN_SECRET not set. Using Stellar CLI identity 'admin'."
    ADMIN_SECRET=$(stellar keys show admin --network "${NETWORK}" 2>/dev/null || echo "")
fi

log_info "Deploying to network: ${NETWORK}"
log_info "Admin address: ${ADMIN_ADDRESS}"

# ── Build Contracts ────────────────────────────────────────────────────────
log_info "Building contracts..."
cd "${REPO_ROOT}/core"
make build

cd "${REPO_ROOT}/orchestrator"
make build

# ── Deploy Payment Contract ────────────────────────────────────────────────
log_info "Deploying Payment contract..."
PAYMENT_WASM="${REPO_ROOT}/core/target/wasm32v1-none/release/payment.wasm"
if [[ ! -f "${PAYMENT_WASM}" ]]; then
    log_error "Payment WASM not found at ${PAYMENT_WASM}"
    exit 1
fi

PAYMENT_ID=$(stellar contract deploy \
    --wasm "${PAYMENT_WASM}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --alias payment 2>&1 | tail -1)
log_info "Payment contract deployed: ${PAYMENT_ID}"

# ── Deploy Escrow Contract ─────────────────────────────────────────────────
log_info "Deploying Escrow contract..."
ESCROW_WASM="${REPO_ROOT}/core/target/wasm32v1-none/release/escrow.wasm"
ESCROW_ID=$(stellar contract deploy \
    --wasm "${ESCROW_WASM}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --alias escrow 2>&1 | tail -1)
log_info "Escrow contract deployed: ${ESCROW_ID}"

# ── Deploy Refund Contract ─────────────────────────────────────────────────
log_info "Deploying Refund contract..."
REFUND_WASM="${REPO_ROOT}/core/target/wasm32v1-none/release/refund.wasm"
REFUND_ID=$(stellar contract deploy \
    --wasm "${REFUND_WASM}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --alias refund 2>&1 | tail -1)
log_info "Refund contract deployed: ${REFUND_ID}"

# ── Deploy Admin Contract ──────────────────────────────────────────────────
log_info "Deploying Admin contract..."
ADMIN_WASM="${REPO_ROOT}/orchestrator/target/wasm32v1-none/release/admin.wasm"
ADMIN_CONTRACT_ID=$(stellar contract deploy \
    --wasm "${ADMIN_WASM}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --alias admin 2>&1 | tail -1)
log_info "Admin contract deployed: ${ADMIN_CONTRACT_ID}"

# ── Initialize Admin Contract ──────────────────────────────────────────────
log_info "Initializing Admin contract with deployed addresses..."
stellar contract invoke \
    --id "${ADMIN_CONTRACT_ID}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    -- initialize \
    --admin "${ADMIN_ADDRESS}" \
    --pauser "${ADMIN_ADDRESS}" \
    --payment_contract "${PAYMENT_ID}" \
    --escrow_contract "${ESCROW_ID}" \
    --refund_contract "${REFUND_ID}" 2>&1

# ── Initialize Payment Contract ────────────────────────────────────────────
log_info "Initializing Payment contract..."
stellar contract invoke \
    --id "${PAYMENT_ID}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    -- initialize \
    --admin "${ADMIN_ADDRESS}" 2>&1

# ── Initialize Escrow Contract ─────────────────────────────────────────────
log_info "Initializing Escrow contract..."
stellar contract invoke \
    --id "${ESCROW_ID}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    -- initialize \
    --admin "${ADMIN_ADDRESS}" 2>&1

# ── Initialize Refund Contract ─────────────────────────────────────────────
log_info "Initializing Refund contract..."
stellar contract invoke \
    --id "${REFUND_ID}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --initialize \
    --admin "${ADMIN_ADDRESS}" 2>&1

# ── Cross-register contracts ───────────────────────────────────────────────
log_info "Cross-registering contracts..."
# Register escrow contract address in payment contract
stellar contract invoke \
    --id "${PAYMENT_ID}" \
    --source "${ADMIN_SECRET}" \
    --network "${NETWORK}" \
    --set_escrow_contract \
    --escrow_contract "${ESCROW_ID}" 2>&1 || log_warn "set_escrow_contract not available, skipping"

# ── Generate Deployment Manifest ───────────────────────────────────────────
log_info "Generating deployment manifest..."
cat > "${MANIFEST_FILE}" <<EOF
{
  "network": "${NETWORK}",
  "deployed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "admin_address": "${ADMIN_ADDRESS}",
  "contracts": {
    "payment": "${PAYMENT_ID}",
    "escrow": "${ESCROW_ID}",
    "refund": "${REFUND_ID}",
    "admin": "${ADMIN_CONTRACT_ID}"
  }
}
EOF

# Validate manifest JSON before reporting success
jq . "${MANIFEST_FILE}" >/dev/null || { log_error "Failed to write valid deployment manifest JSON"; exit 1; }

log_info "Deployment manifest written to: ${MANIFEST_FILE}"
log_info "Deployment complete!"
echo ""
echo "Contract Addresses:"
echo "  Payment: ${PAYMENT_ID}"
echo "  Escrow:  ${ESCROW_ID}"
echo "  Refund:  ${REFUND_ID}"
echo "  Admin:   ${ADMIN_CONTRACT_ID}"
