# Storage Schema Versioning

Cypher GridPay Soroban smart contracts (`core/contracts/payment`, `core/contracts/refund`, `core/contracts/escrow`) and the orchestrator contracts (`orchestrator/contracts/admin`) implement an explicit storage schema versioning convention. This allows deployed contracts to track their data storage layout version on-chain and perform state migrations as stored data structures evolve over time.

---

## 📐 How Storage Schema Versions Are Tracked

Every contract tracks its schema version in instance storage under a dedicated storage key (`ConfigKey::SchemaVersion`, `SystemKey::SchemaVersion` or `DataKey::SchemaVersion`).

### Key Functions

1. **`get_schema_version(env: Env) -> u32`**
   - Returns the current schema version number stored in contract instance storage.
   - Defaults to `1` (`INITIAL_SCHEMA_VERSION`) if no custom version has been written yet.

2. **`migrate_schema(env: Env, admin: Address, target_version: u32) -> Result<(), Error>`**
   - Authorized admin-only function that updates the contract schema version to `target_version`.
   - Returns an error (`SchemaAlreadyAtTarget`) if the current stored version is already greater than or equal to `target_version`.
   - **Validates storage invariants before persisting the version (Issue #88).** Every data transformation registered for the version steps in `(current_version, target_version]` is executed *first*. `target_version` is written only after all transformations succeed, so a version can never be bumped on top of partially migrated state.

---

## 🛡️ Migration Invariant Checks (Issue #88)

`migrate_schema` never trusts the caller blindly. Before writing `target_version`, each contract runs the data migrations registered for the intermediate version steps:

| Contract | Version step | Transformation |
|---|---|---|
| `core/contracts/payment` | v1 → v2 | Re-indexes every payment in `1..=PaymentKey::Counter` into the customer index, the merchant index and the paged merchant index. |
| `core/contracts/refund` | v1 → v2 | Re-indexes every refund in `1..=RefundCounter` into the per-status index and the customer history index, and backfills `SystemKey::RefundRejectedAt` for rejected refunds. |
| `orchestrator/contracts/admin` | v1 → v2 | Re-validates the stored orchestrator configuration: the pauser and every child contract address must be present and the three child contracts must be distinct, otherwise an emergency pause could leave a role unmanaged. |

If a single entry cannot be read, or its stored id disagrees with the key it is stored under, the migration aborts with `SchemaMigrationFailed` and **the whole transaction is reverted**:

- the stored schema version stays at its previous value, so the migration can be retried after the corrupted entry is fixed;
- every write performed for the entries that *did* succeed is rolled back, so no half-migrated state survives.

```rust
// Pseudocode of the contract-side flow
if current >= target_version { return Err(SchemaAlreadyAtTarget); }
run_data_migrations(&env, current, target_version)?; // reverts on any failure
env.storage().instance().set(&schema_version_key, &target_version);
```

Reference tests: [`core/contracts/payment/src/test_schema_migration.rs`](../core/contracts/payment/src/test_schema_migration.rs) and [`core/contracts/refund/src/test_schema_migration.rs`](../core/contracts/refund/src/test_schema_migration.rs).

---

## 🛠️ Contributor Workflow: Changing Stored Data Shapes

When modifying an existing stored data structure (such as adding fields to a struct, modifying enum variants, or restructuring storage keys), contributors must adhere to the following workflow:

1. **Assess Breaking Changes**:
   - Determine if the change breaks backwards compatibility with existing on-chain data.
   - Adding non-optional fields or re-interpreting existing byte encodings requires a schema migration.

2. **Define Migration Logic**:
   - Update `migrate_schema()` in the relevant contract (e.g., [`core/contracts/payment/src/lib.rs`](../core/contracts/payment/src/lib.rs) or [`core/contracts/refund/src/lib.rs`](../core/contracts/refund/src/lib.rs)) to handle reading historical data shapes and writing upgraded data structures.
   - Register the per-version transformation in `run_data_migrations()` and return `SchemaMigrationFailed` when an entry cannot be transformed, so the failure reverts the transaction instead of silently bumping the version.

3. **Increment Target Schema Version**:
   - Ensure contract calls specify the new target version integer (`target_version > current_version`).

4. **Add & Update Unit Tests**:
   - Create or update contract tests to verify that:
     - `get_schema_version()` starts at `1` after contract `initialize()`.
     - `migrate_schema()` successfully increments the version when called by an authorized admin.
     - Calling `migrate_schema()` with a target version `<= current_version` fails with `SchemaAlreadyAtTarget`.
     - A failing entry migration leaves the stored version untouched and rolls back every partial write (Issue #88).

---

## 🔑 Data Key Namespacing Conventions

Issue #86 audited every `enum DataKey` / `enum PaymentKey` / `enum ConfigKey`
family in the repository for key collisions. This section documents the rules
those contracts follow, because they are the only thing standing between a
renamed variant and silent state corruption.

### Why Names Matter More Than Positions

Soroban does **not** encode a `#[contracttype]` enum by its position. It encodes
it as a `ScVal::Vec` whose first element is a `ScVal::Symbol` holding the
**variant name**, followed by one element per tuple field:

```
ConfigKey::Admin              ->  Vec [ Symbol("Admin") ]
PaymentKey::Data(7)           ->  Vec [ Symbol("Data"), U32(7) ]
DataKey::Config(ConfigKey::Admin)
                              ->  Vec [ Symbol("Config"), Vec [ Symbol("Admin") ] ]
```

Two practical consequences:

- **Inserting or reordering variants is safe.** Position carries no meaning, so
  appending a variant in the middle of an enum does not move existing data.
- **Spelling a variant name twice is a silent alias.** `DataKey::X(Foo(1))` and
  `SomeOtherKey::Foo(1)` serialize to the *same* bytes and therefore read and
  write the *same* storage slot. The compiler will not complain.

### The Two Namespacing Styles Used Here

| Contract | Style | Isolation mechanism |
| --- | --- | --- |
| `core/contracts/payment` | Single outer `DataKey` wrapping seven inner enums (`Config`, `Payment`, `Subscription`, `Feature`, `Customer`, `Merchant`, `State`) | The **outer** variant name is the namespace, so `DataKey::Customer(CustomerDataKey::Analytics(a))` and `DataKey::Merchant(MerchantDataKey::Analytics(a))` are distinct slots despite the shared inner name. |
| `core/contracts/escrow` | Single outer `DataKey` wrapping four inner enums (`Config`, `Escrow`, `Participant`, `Dispute`) plus two un-namespaced keys (`VoteWeight`, `ReleaseThresholdBps`) | Same mechanism. The un-namespaced keys are deliberate direct slots and are audited alongside the namespaced ones. |
| `core/contracts/refund` | **Flat.** Nine independent key enums (`DataKey`, `ArbitrationKey`, `PolicyKey`, `SystemKey`, `EvidenceKey`, `VoucherKey`, `TokenKey`, `RefundExtKey`, `EligibilityKey`) are written straight to `env.storage().instance()`. | **No namespace at all** — every variant name in every one of those nine enums must be globally unique. |

Because the refund contract is flat, it is the most exposed to this class of bug.
It previously carried `RefundPolicyVersion(Address, u32)` and
`RefundPolicyVersionCount(Address)` in *both* `DataKey` and `PolicyKey`; the two
spellings aliased one slot. `PolicyKey` is now the single documented owner and
`DataKey` must never spell those names again. Because the encoding is
name-based, the fix changed **no on-chain bytes** and needs no migration.

### Rules for Contributors

1. **One spelling, one owner.** A given variant name may be declared in exactly
   one key enum per contract. If two enums need the same logical key, one must
   wrap the other (`DataKey::Config(ConfigKey::Admin)`), never duplicate the
   name.
2. **Namespace before you nest.** When adding a new family of keys, add an outer
   `DataKey` variant that wraps a new inner enum rather than adding flat
   variants to an existing enum.
3. **Never rename a variant in place.** Renaming changes the serialized symbol
   and orphans the existing slot. Add the new name, migrate the data, then
   remove the old name in a later schema version.
4. **Audit the flat contract hardest.** Any new key in `core/contracts/refund`
   must be checked against all nine of its key enums before merging.

---

## 🗂️ Complete Data Key Schema & Persistence Layout

This section enumerates every persistent and instance storage key used by the
Payment, Escrow and Refund contracts, the value type stored under each key, and
the serialization format Soroban applies. It complements the namespacing rules
above with the concrete layout a migration author needs.

### Serialization Format

All keys and values are `#[contracttype]` types serialized by the Soroban host
into `ScVal` before being written to the ledger:

- **Keys** are encoded as `ScVal::Vec` with a leading `ScVal::Symbol` holding the
  variant name (see the namespacing section above).
- **Structs** are encoded as `ScVal::Map` with `ScVal::Symbol` field names as
  keys, so field order is irrelevant and adding a field is a breaking change
  only for readers that require it.
- **Enums** are encoded as `ScVal::Vec` with a leading `ScVal::Symbol` variant
  name, followed by one element per payload field.
- **Integers** map to the smallest `ScVal` integer type that fits (`U32`, `I32`,
  `U64`, `I64`, `U128`, `I128`); `Address` maps to `ScVal::Address`; `bool` to
  `ScVal::Bool`; `Bytes`/`BytesN` to `ScVal::Bytes`; `String`/`Symbol` to
  `ScVal::String`/`ScVal::Symbol`.

### Payment Contract (`core/contracts/payment`)

**Instance storage** (lives with the contract instance, bumped together):

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Config(ConfigKey::Admin)` | `Address` | Contract admin authorized for config and migrations. |
| `DataKey::Config(ConfigKey::SchemaVersion)` | `u32` | Current storage schema version. |
| `DataKey::Config(ConfigKey::Paused)` | `bool` | Global pause flag. |
| `DataKey::Config(ConfigKey::FeeBps)` | `u32` | Protocol fee in basis points. |
| `DataKey::Config(ConfigKey::FeeRecipient)` | `Address` | Recipient of collected fees. |
| `DataKey::Config(ConfigKey::Token)` | `Address` | Accepted payment token contract. |
| `DataKey::State(StateKey::Initialized)` | `bool` | Guards against re-initialization. |
| `DataKey::Payment(PaymentKey::Counter)` | `u64` | Monotonic payment id counter. |
| `DataKey::Subscription(SubscriptionKey::Counter)` | `u64` | Monotonic subscription id counter. |

**Persistent storage** (per-entity entries, independently extendable):

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Payment(PaymentKey::Data(id))` | `PaymentRecord` | Payment record for `id`. |
| `DataKey::Payment(PaymentKey::Status(id))` | `PaymentStatus` | Lifecycle status of payment `id`. |
| `DataKey::Customer(CustomerDataKey::Index(customer))` | `Vec<u64>` | Payment ids belonging to a customer. |
| `DataKey::Customer(CustomerDataKey::Analytics(customer))` | `CustomerAnalytics` | Aggregated per-customer counters. |
| `DataKey::Merchant(MerchantDataKey::Index(merchant))` | `Vec<u64>` | Payment ids belonging to a merchant. |
| `DataKey::Merchant(MerchantDataKey::PagedIndex(merchant, page))` | `Vec<u64>` | Paged merchant payment ids. |
| `DataKey::Merchant(MerchantDataKey::Analytics(merchant))` | `MerchantAnalytics` | Aggregated per-merchant counters. |
| `DataKey::Subscription(SubscriptionKey::Data(id))` | `Subscription` | Subscription record for `id`. |
| `DataKey::Feature(FeatureKey::Flag(name))` | `bool` | Feature flag toggle. |

**Struct fields** (serialized as `ScVal::Map`):

| Struct | Fields |
| --- | --- |
| `PaymentRecord` | `id: u64`, `customer: Address`, `merchant: Address`, `amount: i128`, `token: Address`, `status: PaymentStatus`, `created_at: u64`, `metadata: Bytes` |
| `PaymentStatus` | enum: `Pending`, `Completed`, `Refunded`, `Failed` |
| `CustomerAnalytics` | `total_payments: u64`, `total_volume: i128`, `last_payment_at: u64` |
| `MerchantAnalytics` | `total_payments: u64`, `total_volume: i128`, `last_payment_at: u64` |
| `Subscription` | `id: u64`, `customer: Address`, `merchant: Address`, `amount: i128`, `interval: u64`, `next_charge_at: u64`, `active: bool` |

### Escrow Contract (`core/contracts/escrow`)

**Instance storage**:

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Config(ConfigKey::Admin)` | `Address` | Contract admin. |
| `DataKey::Config(ConfigKey::SchemaVersion)` | `u32` | Current storage schema version. |
| `DataKey::Config(ConfigKey::Token)` | `Address` | Escrowed token contract. |
| `DataKey::Config(ConfigKey::Paused)` | `bool` | Global pause flag. |
| `DataKey::VoteWeight` | `u32` | Default dispute vote weight. |
| `DataKey::ReleaseThresholdBps` | `u32` | Release approval threshold in basis points. |
| `DataKey::Escrow(EscrowKey::Counter)` | `u64` | Monotonic escrow id counter. |

**Persistent storage**:

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Escrow(EscrowKey::Data(id))` | `EscrowRecord` | Escrow record for `id`. |
| `DataKey::Escrow(EscrowKey::Status(id))` | `EscrowStatus` | Lifecycle status of escrow `id`. |
| `DataKey::Participant(ParticipantKey::Role(id, addr))` | `ParticipantRole` | Role held by `addr` in escrow `id`. |
| `DataKey::Participant(ParticipantKey::Approval(id, addr))` | `bool` | Whether `addr` approved release of escrow `id`. |
| `DataKey::Dispute(DisputeKey::Data(id))` | `Dispute` | Dispute record for escrow `id`. |
| `DataKey::Dispute(DisputeKey::Vote(id, addr))` | `Vote` | Vote cast by `addr` on dispute `id`. |

**Struct fields**:

| Struct | Fields |
| --- | --- |
| `EscrowRecord` | `id: u64`, `depositor: Address`, `beneficiary: Address`, `amount: i128`, `token: Address`, `status: EscrowStatus`, `created_at: u64`, `deadline: u64` |
| `EscrowStatus` | enum: `Created`, `Funded`, `Released`, `Refunded`, `Disputed` |
| `ParticipantRole` | enum: `Depositor`, `Beneficiary`, `Arbiter` |
| `Dispute` | `escrow_id: u64`, `opened_by: Address`, `reason: Bytes`, `opened_at: u64`, `resolved: bool` |
| `Vote` | `voter: Address`, `weight: u32`, `in_favor: bool`, `cast_at: u64` |

### Refund Contract (`core/contracts/refund`)

The refund contract is **flat**: all nine key enums below are written directly
to `env.storage().instance()` / `env.storage().persistent()` with no outer
namespace, so every variant name must be globally unique (see the namespacing
section above).

**Instance storage**:

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Admin` | `Address` | Contract admin. |
| `DataKey::Token` | `Address` | Refund token contract. |
| `DataKey::Paused` | `bool` | Global pause flag. |
| `SystemKey::SchemaVersion` | `u32` | Current storage schema version. |
| `SystemKey::RefundCounter` | `u64` | Monotonic refund id counter. |
| `SystemKey::RefundRejectedAt(id)` | `u64` | Timestamp a refund was rejected (backfilled by the v1→v2 migration). |
| `PolicyKey::RefundPolicyVersion(addr, v)` | `RefundPolicy` | Policy version `v` for `addr`. |
| `PolicyKey::RefundPolicyVersionCount(addr)` | `u32` | Number of policy versions for `addr`. |
| `TokenKey::Supported(token)` | `bool` | Whether `token` is an accepted refund token. |
| `EligibilityKey::Rule(id)` | `EligibilityRule` | Eligibility rule `id`. |

**Persistent storage**:

| Key | Value type | Purpose |
| --- | --- | --- |
| `DataKey::Refund(id)` | `RefundRecord` | Refund record for `id`. |
| `DataKey::StatusIndex(status, id)` | `bool` | Membership of refund `id` in the per-status index. |
| `DataKey::CustomerHistory(customer, id)` | `bool` | Membership of refund `id` in a customer's history index. |
| `ArbitrationKey::Case(id)` | `ArbitrationCase` | Arbitration case for refund `id`. |
| `ArbitrationKey::Ruling(id)` | `Ruling` | Ruling issued for arbitration case `id`. |
| `EvidenceKey::Item(id, seq)` | `Evidence` | Evidence item `seq` attached to refund `id`. |
| `VoucherKey::Data(code)` | `Voucher` | Voucher identified by `code`. |
| `VoucherKey::Redeemed(code)` | `bool` | Whether voucher `code` has been redeemed. |
| `RefundExtKey::Metadata(id)` | `Bytes` | Free-form metadata for refund `id`. |

**Struct fields**:

| Struct | Fields |
| --- | --- |
| `RefundRecord` | `id: u64`, `payment_id: u64`, `customer: Address`, `merchant: Address`, `amount: i128`, `token: Address`, `status: RefundStatus`, `created_at: u64`, `reason: Bytes` |
| `RefundStatus` | enum: `Pending`, `Approved`, `Rejected`, `Completed` |
| `RefundPolicy` | `version: u32`, `max_refund_bps: u32`, `window_secs: u64`, `requires_arbitration: bool` |
| `EligibilityRule` | `id: u32`, `min_amount: i128`, `max_amount: i128`, `min_age_secs: u64` |
| `ArbitrationCase` | `refund_id: u64`, `opened_by: Address`, `opened_at: u64`, `resolved: bool` |
| `Ruling` | `case_id: u64`, `in_favor_of_customer: bool`, `amount: i128`, `issued_at: u64` |
| `Evidence` | `refund_id: u64`, `seq: u32`, `submitter: Address`, `data: Bytes`, `submitted_at: u64` |
| `Voucher` | `code: Symbol`, `amount: i128`, `expires_at: u64`, `redeemed: bool` |

### Storage Durability Summary

| Contract | Instance keys | Persistent keys | Schema version key |
| --- | --- | --- | --- |
| `core/contracts/payment` | Config, State, counters | Payment, Customer, Merchant, Subscription, Feature | `DataKey::Config(ConfigKey::SchemaVersion)` |
| `core/contracts/escrow` | Config, VoteWeight, ReleaseThresholdBps, counter | Escrow, Participant, Dispute | `DataKey::Config(ConfigKey::SchemaVersion)` |
| `core/contracts/refund` | DataKey, SystemKey, PolicyKey, TokenKey, EligibilityKey | DataKey, ArbitrationKey, EvidenceKey, VoucherKey, RefundExtKey | `SystemKey::SchemaVersion` |
