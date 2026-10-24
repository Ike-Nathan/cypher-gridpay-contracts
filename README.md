# Cypher GridPay Smart Contracts

Stellar-based smart contracts for Cypher GridPay. Secure, auditable, and transparent payment infrastructure.

## 🏗️ Architecture

Cypher GridPay uses Soroban smart contracts on Stellar for:
- **Payment Processing**: Accept and lock crypto payments
- **Settlement**: Convert and transfer to merchants in USDC
- **Escrow**: Hold funds during dispute periods
- **Refunds**: Process customer refunds

## 📋 Prerequisites

- Rust 1.74.0 or later
- Stellar CLI (`stellar` command)
- Soroban SDK

### Installation

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Stellar CLI
cargo install --locked stellar-cli --features opt

# Add wasm target
rustup target add wasm32-unknown-unknown
```

## 🚀 Quick Start

### Build All Contracts

```bash
# From root directory
make
```

### Run Tests

```bash
# Run all workspace tests (core + orchestrator)
make test

# Core workspace (escrow, payments, refund)
cargo test --workspace      # from inside core/

# Orchestrator workspace (admin)
cargo test --workspace      # from inside orchestrator/

# Test a single test file or function within a contract
cargo test -p refund test_arbitration_fees   # from inside core/
cargo test -p escrow test_dispute_resolution # from inside core/

# Run tests matching a pattern
cargo test -p payments test_fee # from inside core/
```

## 📂 Contract Overview

### Payment Contract (`core/contracts/payment`)

Handles payment creation and processing:
- `create_payment()` - Customer initiates payment
- `complete_payment()` - Admin releases to merchant
- `refund_payment()` - Admin refunds to customer
- `get_payment()` - Query payment details

### Escrow Contract (`core/contracts/escrow`)

Manages fund holding and disputes:
- `create_escrow()` - Lock funds
- `release_escrow()` - Release to merchant
- `dispute_escrow()` - Handle disputes

### Refund Contract (`core/contracts/refund`)

Processes refund requests:
- `request_refund()` - Merchant initiates
- `approve_refund()` - Admin approves
- `process_refund()` - Execute refund
- `get_refunds_by_reason_code()` - Filter refunds by structured reason code with pagination
- `get_reason_code_analytics()` - Count refunds by reason code (sorted by frequency)

#### Refund Reason Code Migration (Breaking)

`request_refund()` now requires a `reason_code: RefundReasonCode` argument in addition to free-text `reason`.

- Old call shape: `request_refund(..., reason, payment_created_at)`
- New call shape: `request_refund(..., reason, reason_code, payment_created_at)`

Recommended migration path:
1. Update all callers to pass a concrete enum value (`ProductDefect`, `NonDelivery`, `DuplicateCharge`, `Unauthorized`, `CustomerRequest`, `Other`).
2. For unknown/legacy flows, pass `Other` first and backfill specific codes in your upstream app logic.
3. If upgrading a deployed instance with existing data, plan a storage/data migration for historical refunds before reading them as the new `Refund` shape.

## 🧩 Client SDK (TypeScript)

Type-safe client bindings for all four contracts are generated from the
compiled WASM and published as `@cypher-gridpay/contracts-sdk`:

```bash
# Generate bindings (builds the contracts first)
./scripts/generate-ts-bindings.sh

# Build and pack the package
cd sdk/typescript
npm install
npm run build
npm pack          # -> cypher-gridpay-contracts-sdk-<version>.tgz
```

```ts
import { PaymentsClient } from "@cypher-gridpay/contracts-sdk";

const client = new PaymentsClient({
  contractId: paymentContractId,
  publicKey: viewerPublicKey,
  server,
  networkPassphrase: Networks.TESTNET,
});

const payment = await client.getPayment(paymentId);
```

Pass no `source` for a read-only client. Re-run the generator after any contract
interface change — the generated `.d.ts` turns a breaking change into a compile
error in your app. See [sdk/typescript/README.md](sdk/typescript/README.md).

## 🔄 Development Workflow

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Write tests for your changes
4. Ensure all tests pass (`cargo test --workspace`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to your branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## 🔗 Links

- [TypeScript SDK](sdk/typescript/README.md)
- [Python SDK](docs/PYTHON_SDK.md)
- [Storage Versioning Guide](docs/STORAGE_VERSIONING.md)
- [Storage Restoration Guide](docs/STORAGE_RESTORATION.md)
- Telegram: https://t.me/+afM9uh7GGtVkYmZk
- [API Repository](https://github.com/cypher-gridpay/cypher-gridpay-api)
- [SDK Repository](https://github.com/cypher-gridpay/cypher-gridpay-sdk)


## License

This project is licensed under the [MIT License](LICENSE).

## Handsoff notes

<!-- handsoff-issue-10 -->
- #10: payment: Implement dynamic risk fee surcharge updates based on merchant chargeback history

<!-- handsoff-issue-11 -->
- #11: payment: Add nonce and replay protection to multisig action proposal execution

<!-- handsoff-issue-13 -->
- #13: payment: Add expiration and overflow checks to customer loyalty points system
