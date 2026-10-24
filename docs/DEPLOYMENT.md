# Deploying Cypher GridPay Smart Contracts

This guide walks you through building, setting up identities, funding accounts, and deploying all four Cypher GridPay smart contracts across the `core` and `orchestrator` workspaces to Stellar Testnet (or local quickstart) using the **Stellar CLI**.

---

## Gas Consumption Limits

The `admin` contract dispatches calls to the `payment`, `escrow`, and `refund`
contracts in a single Soroban transaction. The table below records the measured
resource consumption of the multi-contract dispatch entry points under the
Soroban test environment (Issue #81).

| Entry point | CPU instructions | Memory bytes | Dominant cost |
| --- | --- | --- | --- |
| `emergency_pause_all` | ~549,000 | ~74,200 | 3 cross-contract `pause_contract` calls |
| `emergency_unpause_all` | ~801,000 | ~113,500 | 3 cross-contract `unpause_contract` calls |
| `get_system_status` | ~440,000 | ~62,700 | 6 read-only cross-contract queries |

### Budget limits

- **CPU instruction limit:** 1,000,000,000 instructions per transaction (current Soroban network limit).
- **Memory limit:** 40,000,000 bytes per transaction.
- The most expensive dispatch (`emergency_unpause_all`) uses roughly **0.08% of the CPU budget** and **0.3% of the memory budget**, leaving ample headroom for the surrounding transaction (signature verification, transaction size, etc.).

> **Note:** Measurements are taken in the Soroban test environment, which
> underestimates CPU and memory relative to the WASM equivalent. Treat the
> numbers above as lower bounds when provisioning mainnet capacity.

### Optimization notes

- The dispatch cost is dominated by the cross-contract calls, which are irreducible: each child contract must be paused/unpaused individually, and each read-only health query is a separate cross-contract call.
- The admin contract reads the three child contract addresses through a single shared helper (`get_contract_addresses`) to avoid duplicating storage reads and error handling across dispatch functions.
- The `reason` string passed to `emergency_pause_all` is cloned once per child contract call, which is unavoidable because each child contract requires its own copy.

---

## Prerequisites

```
Step 1: Payment Contract   (core)
Step 2: Escrow Contract    (core)
Step 3: Refund Contract    (core)
Step 4: Admin Orchestrator (orchestrator)
Step 5: Initialize Admin Orchestrator with Step 1-3 addresses
```

---

## Prerequisites

1. **Rust & `wasm32` Target**:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **Stellar CLI**:
   ```bash
   cargo install --locked stellar-cli --features opt
   ```

3. **jq** (for JSON processing):
   ```bash
   # macOS
   brew install jq
   # Ubuntu/Debian
   apt-get install jq
   ```

---

## Deployment Order

The contracts must be deployed and initialized in the following order due to cross-contract dependencies:

### Step 1: Deploy Payment Contract

The payment contract is the core entry point for creating and managing payments.

```bash
# Build the payment contract
cd core
make build
cd ..

# Deploy to testnet
PAYMENT_ID=$(stellar contract deploy \
    --wasm core/target/wasm32v1-none/release/payment.wasm \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    --alias payment)

echo "Payment contract deployed: $PAYMENT_ID"
```

### Step 2: Deploy Escrow Contract

The escrow contract holds funds during the payment lifecycle.

```bash
ESCROW_ID=$(stellar contract deploy \
    --wasm core/target/wasm32v1-none/release/escrow.wasm \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    --alias escrow)

echo "Escrow contract deployed: $ESCROW_ID"
```

### Step 3: Deploy Refund Contract

The refund contract handles refund processing and arbitration.

```bash
REFUND_ID=$(stellar contract deploy \
    --wasm core/target/wasm32v1-none/release/refund.wasm \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    --alias refund)

echo "Refund contract deployed: $REFUND_ID"
```

### Step 4: Deploy Admin Contract

The admin contract orchestrates the other three contracts and provides administrative control.

```bash
ADMIN_ID=$(stellar contract deploy \
    --wasm orchestrator/target/wasm32v1-none/release/admin.wasm \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    --alias admin)

echo "Admin contract deployed: $ADMIN_ID"
```

### Step 5: Initialize Admin Contract

Initialize the admin contract with the addresses of the other three deployed contracts.

```bash
stellar contract invoke \
    --id $ADMIN_ID \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    -- initialize \
    --admin <YOUR_ACCOUNT_ADDRESS> \
    --pauser <YOUR_ACCOUNT_ADDRESS> \
    --payment_contract $PAYMENT_ID \
    --escrow_contract $ESCROW_ID \
    --refund_contract $REFUND_ID
```

### Step 6: Initialize Payment Contract

```bash
stellar contract invoke \
    --id $PAYMENT_ID \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    -- initialize \
    --admin <YOUR_ACCOUNT_ADDRESS>
```

### Step 7: Initialize Escrow Contract

```bash
stellar contract invoke \
    --id $ESCROW_ID \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    -- initialize \
    --admin <YOUR_ACCOUNT_ADDRESS>
```

### Step 8: Initialize Refund Contract

```bash
stellar contract invoke \
    --id $REFUND_ID \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    -- initialize \
    --admin <YOUR_ACCOUNT_ADDRESS>
```

### Step 9: Configure Cross-Contract References

Register the escrow contract address in the payment contract:

```bash
stellar contract invoke \
    --id $PAYMENT_ID \
    --source <YOUR_SECRET_KEY> \
    --network testnet \
    -- set_escrow_contract \
    --escrow_contract $ESCROW_ID
```

---

## Automated Deployment

For automated deployment, use the provided script:

```bash
export ADMIN_ADDRESS=<YOUR_ACCOUNT_ADDRESS>
export ADMIN_SECRET=<YOUR_SECRET_KEY>
./scripts/deploy-testnet.sh testnet
```

This script handles all build, deploy, initialization, and cross-registration steps automatically.

The script writes a deployment manifest to `deployment-manifest.json` containing the network, timestamp, admin address, and all four contract IDs:

```json
{
  "network": "testnet",
  "deployed_at": "2026-01-01T00:00:00Z",
  "admin_address": "G...",
  "contracts": {
    "payment": "C...",
    "escrow": "C...",
    "refund": "C...",
    "admin": "C..."
  }
}
```

The manifest JSON is validated by the script before it exits successfully. Pass `futurenet` as the argument to deploy to Futurenet instead of Testnet.

---

## Admin Setup

After deployment, configure the admin contract:

1. **Add additional admins** (if using multi-sig):
   ```bash
   stellar contract invoke \
       --id $PAYMENT_ID \
       --source <YOUR_SECRET_KEY> \
       --network testnet \
       -- add_admin \
       --new_admin <NEW_ADMIN_ADDRESS>
   ```

2. **Configure fee structure**:
   ```bash
   stellar contract invoke \
       --id $PAYMENT_ID \
       --source <YOUR_SECRET_KEY> \
       --network testnet \
       -- set_fee_config \
       --fee_bps 100 \
       --min_fee 0 \
       --max_fee 0 \
       --treasury <TREASURY_ADDRESS> \
       --fee_token <TOKEN_ADDRESS> \
       --active true
   ```

3. **Configure rate limits** (optional):
   ```bash
   stellar contract invoke \
       --id $PAYMENT_ID \
       --source <YOUR_SECRET_KEY> \
       --network testnet \
       -- set_rate_limit_config \
       --max_payments_per_window 10 \
       --window_duration 60 \
       --max_payment_amount 1000000000 \
       --max_daily_volume 10000000000
   ```

---

## Verification

After deployment, verify all contracts are properly configured:

```bash
# Check admin contract
stellar contract invoke --id $ADMIN_ID --network testnet -- get_admin

# Check payment contract
stellar contract invoke --id $PAYMENT_ID --network testnet -- get_admin

# Check escrow contract
stellar contract invoke --id $ESCROW_ID --network testnet -- get_admin

# Check refund contract
stellar contract invoke --id $REFUND_ID --network testnet -- get_admin
```

---

## Troubleshooting

### Common Issues

1. **"already initialized" error**: The contract has already been initialized. Check the contract state.
2. **"unauthorized" error**: The admin key doesn't match the configured admin address.
3. **WASM too large**: Run `make check-size` to verify the WASM binary doesn't exceed 256KB.

### Resetting

To redeploy from scratch, generate a new WASM hash and deploy with a new alias or contract ID.
