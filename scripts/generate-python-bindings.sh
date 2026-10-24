#!/usr/bin/env bash
# generate-python-bindings.sh — Generate Python client SDK for Cypher GridPay contracts
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUTPUT_DIR="${1:-${REPO_ROOT}/sdk/python/cypher_gridpay_sdk/bindings}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

command -v stellar >/dev/null 2>&1 || { log_error "Stellar CLI not found."; exit 1; }
command -v python3 >/dev/null 2>&1 || { log_error "python3 not found."; exit 1; }

CONTRACTS=("payment" "escrow" "refund" "admin")
CONTRACT_WASM_PATHS=(
    "core/target/wasm32v1-none/release/payment.wasm"
    "core/target/wasm32v1-none/release/escrow.wasm"
    "core/target/wasm32v1-none/release/refund.wasm"
    "orchestrator/target/wasm32v1-none/release/admin.wasm"
)

log_info "Generating Python bindings for Cypher GridPay contracts..."
log_info "Output directory: ${OUTPUT_DIR}"

mkdir -p "${OUTPUT_DIR}"

for i in "${!CONTRACTS[@]}"; do
    contract_name="${CONTRACTS[$i]}"
    wasm_path="${REPO_ROOT}/${CONTRACT_WASM_PATHS[$i]}"

    if [[ ! -f "${wasm_path}" ]]; then
        log_warn "WASM not found for ${contract_name}: ${wasm_path}"
        continue
    fi

    log_info "Generating Python bindings for ${contract_name}..."
    stellar contract bindings python \
        --wasm "${wasm_path}" \
        --output "${OUTPUT_DIR}/${contract_name}" \
        2>&1 || log_warn "Bindings generation failed for ${contract_name}"
done

log_info "Generated bindings are in ${OUTPUT_DIR}/"
log_info "Install the maintained SDK from sdk/python with: pip install -e sdk/python"
