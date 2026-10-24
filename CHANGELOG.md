# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Structured Refund Lifecycle Events (Breaking, Issue #72)** — The refund contract now publishes dashboard-ready Soroban contract events for every refund request, approval, denial and appeal. Each event carries the `customer`, `merchant`, `refund_id` and the canonical `reason_code`, so customer and merchant dashboards can render the refund lifecycle in real time from the event stream alone.
  - **New events:** `RefundRequestedEvent`, `RefundApprovedEvent`, `RefundDeniedEvent`, `AppealFiledEvent` (topics `refund_requested_event`, `refund_approved_event`, `refund_denied_event`, `appeal_filed_event`).
  - **Renamed:** `RefundRequested` → `RefundRequestedEvent`, `RefundApproved` → `RefundApprovedEvent`, `AppealFiled` → `AppealFiledEvent`, `RefundRejected` → `RefundDeniedEvent`. Update Horizon/indexer subscriptions that matched the old topic names.
  - **New fields:** `reason_code` on all four events, `requested_at` / `approved_at` / `rejected_at` / `filed_at` timestamps, and `customer` + `merchant` on the approval, denial and appeal events.
  - **Coverage:** approvals and denials now also emit the enriched events when they originate from arbitration decisions, arbitration timeouts and TTL expiry sweeps, not only from `approve_refund()` / `reject_refund()`.
  - **Tests:** new suite `core/contracts/refund/src/test_refund_events.rs` asserts the emitted topic and every payload field of each flow.

- **Orchestrator Schema Versioning (Issue #74)** — The orchestrator `admin` contract now implements the `get_schema_version()` / `migrate_schema()` convention documented in [`docs/STORAGE_VERSIONING.md`](docs/STORAGE_VERSIONING.md), completing the convention across every contract in the repository. `initialize()` writes `INITIAL_SCHEMA_VERSION` (1), `migrate_schema()` is admin-only and runs the registered v1 → v2 data transformation before persisting `target_version`, and it returns `SchemaAlreadyAtTarget` when the stored version is already at or past the target. New suite: `orchestrator/contracts/admin/src/schema_version_test.rs`.

- **Schema Migration Invariant Checks (Issue #88)** — `migrate_schema()` in the payment and refund contracts now executes every registered data transformation *before* writing `target_version` to storage. A single unmigratable entry (unreadable record or id/key mismatch) aborts the call with `SchemaMigrationFailed` (`BasicError::SchemaMigrationFailed` / `ExtError::SchemaMigrationFailed`), reverting every write made by the migration so a version can never be bumped on top of partially migrated state. New suites: `core/contracts/payment/src/test_schema_migration.rs` and `core/contracts/refund/src/test_schema_migration.rs`.

- **Payment Contract Events Documentation** — Comprehensive event reference for all 50+ Soroban events emitted by the payment contract, including core payments, subscriptions, channels, fees, governance, and control events. Off-chain integrators can now use this table to subscribe to events via Horizon.

- **Refund Contract Events Documentation** — Comprehensive event reference for all 20+ Soroban events emitted by the refund contract, including refund lifecycle, appeals, arbitration, and stake management events. Enables off-chain monitoring of refund status changes and arbitration outcomes.

- **CHANGELOG.md** — This file. Tracks all breaking changes, new features, and bug fixes across contract releases to help API/SDK consumers plan upgrades.

- **SECURITY.md** — Vulnerability disclosure policy and security contact information for responsible security research.

### Changed

- **Paginated Customer History & Appeal Queries (Breaking, Issue #87)** — Per-customer and per-merchant lists are no longer returned as a single unbounded vector. Every query now takes `(limit, offset)` and every page size is capped at `MAX_QUERY_PAGE_SIZE` (100) so a result set can never exceed Soroban's 64KB ledger entry limit. Records remain stored individually under keyed indices (`SystemKey::AppealByCustomer(customer, index)`, `VoucherKey::CustomerVoucher(customer, index)`, `Merchant(PendingSettlementIndex(merchant, index))`, `State(PartialPaymentRecord(payment_id, installment))`).
  - **Refund contract:** `get_appeals_by_customer(customer)`, `get_customer_vouchers(customer)` and `get_merchant_pending_refunds(merchant)` now require `limit` and `offset`. New counters `get_appeal_count_by_customer()` and `get_customer_voucher_count()` let callers page through the full history.
  - **Payment contract:** `get_pending_settlements(merchant)` and `get_installment_history(payment_id)` now require `limit` and `offset`. New counters `get_pending_settlement_count(merchant)` and `get_installment_count(payment_id)`.
  - **Clamping (all contracts):** `get_payments_by_customer`, `get_payments_by_merchant`, `get_customer_refund_history`, `get_merchant_refunds`, `get_merchant_refunds_by_status`, `get_refunds_by_status` and `get_merchant_pending_refunds` silently clamp an oversized `limit` to 100 instead of returning an unbounded result.
  - **Migration path:** replace `f(actor)` with a paging loop, e.g. `let mut offset = 0; loop { let page = f(actor, &100, &offset); if page.is_empty() { break; } offset += page.len() as u64; }`, using the matching `*_count` getter as the total.

- **Refund Reason Code Migration (Breaking)** — The `request_refund()` function signature has changed to require a canonical `RefundReasonCode` enum variant in addition to free-text reason.
  - **Old signature:** `request_refund(..., reason: String, payment_created_at: u64)`
  - **New signature:** `request_refund(..., reason: String, reason_code: RefundReasonCode, payment_created_at: u64)`
  - **Reason:** Enables structured querying and analytics via `get_reason_code_analytics()` without free-form string inconsistency.
  - **Migration path:**
    1. Update all callers to pass a concrete enum value: `ProductDefect`, `NonDelivery`, `DuplicateCharge`, `Unauthorized`, `CustomerRequest`, or `Other`.
    2. For unknown/legacy flows, pass `Other` as a fallback and backfill specific codes in your upstream app logic.
    3. If upgrading a deployed instance with existing data, plan a storage/data migration for historical refunds before reading them as the new `Refund` shape.

### Fixed

- Improved documentation coverage to reduce friction for off-chain integrators consuming Soroban events.

- **Split Payment Dust Recipient Guard** — `create_split_payment()` rejects any recipient whose computed share falls below an admin-configured `min_split_amount` floor, preventing dust splits from bloating ledger storage. Enforced via `set_min_split_amount()` / `get_min_split_amount()` (see commit `061aeeb`).

---

## [Previous Versions]

For detailed information on previous releases, see the [Root README](README.md) and individual contract READMEs in the `contracts/` directory.

---

## Guidelines for Contributors

When proposing changes that affect consumers of these contracts:

1. **Breaking changes** — Document the change here under `[Unreleased]` → `### Changed`, include migration guidance, and consider the impact on API/SDK versions.
2. **New features** — Add under `### Added` with a brief description of the feature and where it's used.
3. **Bug fixes** — Add under `### Fixed` with a reference to the issue (if applicable).
4. **Dependencies** — Note any upgrades to Soroban SDK or Rust toolchain under `### Changed`.

Semantic versioning:
- **MAJOR** — breaking changes to contract interfaces (e.g., function signature changes, new required parameters)
- **MINOR** — additive features that don't break existing code
- **PATCH** — bug fixes and internal improvements
