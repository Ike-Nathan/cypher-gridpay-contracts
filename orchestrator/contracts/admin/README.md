# Admin Contract

Part of the [Cypher GridPay smart contracts](../../../README.md) suite on Stellar/Soroban.

## Purpose

The admin contract gates privileged operations across the other contracts. It acts as a central authority that can coordinate emergency actions — such as pausing the payment, escrow, and refund contracts — in a single Soroban call.

## Role & Permission Model

### Admin Address

- The admin is a single `Address` stored in contract instance storage under the `Admin` data key.
- It is set **once** during `initialize(admin, pauser, payment_contract, escrow_contract, refund_contract)` and cannot be changed after initialization.
- Calling `initialize` a second time returns `Error::AlreadyInitialized`.
- The admin address must authorize the `initialize` call via Soroban authentication (`require_auth()`).

### Pauser Address

- A separate `Pauser` address is stored alongside the admin. Only the pauser can trigger `emergency_pause_all` / `emergency_unpause_all`.
- The pauser address is forwarded to each child contract's `pause_contract` / `unpause_contract`, so it must also be an admin on the payment, escrow, and refund contracts. If any child rejects it, the whole call reverts and nothing is paused.

### Permission Checks

Every privileged function performs two authorization checks:

1. **Authentication** — the caller must pass Soroban's `require_auth()` for the supplied admin/pauser address.
2. **Authorization** — the caller's address must match the stored admin (or pauser) address exactly. A mismatch returns `Error::Unauthorized`.

### Privileged Operations

| Function | Description | Admin Required |
|---|---|---|
| `initialize(admin, pauser, payment_contract, escrow_contract, refund_contract)` | Configures the contract with the admin, pauser, and child contract addresses. | Yes (sets the admin) |
| `emergency_pause_all(pauser, reason)` | Pauses the payment, escrow, and refund contracts in one call. | Pauser |
| `emergency_unpause_all(pauser)` | Unpauses the payment, escrow, and refund contracts in one call. Idempotent. | Pauser |
| `set_payment_contract(admin, payment_contract)` | Repoints the orchestrator at a new payment contract (e.g. after an upgrade). | Admin |
| `set_escrow_contract(admin, escrow_contract)` | Repoints the orchestrator at a new escrow contract. | Admin |
| `set_refund_contract(admin, refund_contract)` | Repoints the orchestrator at a new refund contract. | Admin |
| `ping()` | Extends the contract instance TTL. Permissionless. | No |

### Instance TTL

Every administrative call extends the contract's instance TTL to `INSTANCE_BUMP_AMOUNT` (~30 days) once it drops below `INSTANCE_LIFETIME_THRESHOLD`. Between admin actions, a keeper should call `ping()` periodically so the orchestrator is never archived — an archived orchestrator cannot trigger an emergency pause until it is restored.

### Error Codes

| Code | Constant | Description |
|---|---|---|
| 1 | `AlreadyInitialized` | `initialize` was called more than once. |
| 2 | `NotInitialized` | A privileged function was called before `initialize`, or a managed slot is unset. |
| 3 | `Unauthorized` | The caller's address does not match the stored admin/pauser. |
| 4 | `ChildContractCallFailed` | A child contract refused to pause or unpause. The whole transaction reverts. |
| 5 | `ChildAlreadyInTargetState` | Pre-flight found a child already in the target state; nothing was mutated. |
| 6 | `ChildStateVerificationFailed` | Post-conditions were not met after every child reported success. |

## Security Considerations

- The admin address should be a **multi-sig** or **governance contract** address, never a single private key, to avoid a single point of failure.
- The pauser is a separate credential from the admin so emergency response does not require the long-term admin key. It should follow the same multi-sig guidance.
- Because `emergency_pause_all` halts all child contracts, the pauser key should be treated as a high-value credential and stored securely (e.g., in a hardware wallet or threshold-signing scheme).
- There is no `transfer_admin` or `renounce_admin` function — the admin role is permanent. Review deployment scripts carefully before calling `initialize`.
- `get_coordination_status()` is permissionless on purpose: monitoring must be able to detect a split state even while the platform is mid-incident.


## Incident Response

See the [Operational Runbook](../../../docs/OPERATIONAL_RUNBOOK.md) for step-by-step Stellar CLI procedures to trigger an emergency pause, verify it, inspect state after an incident, upgrade contracts, and resume operations.

## Testing

```bash
cd orchestrator && cargo test --workspace
```

The suite lives in [`src/test.rs`](src/test.rs).

---

## See Also

- [Root README](../../../README.md) — architecture overview and workspace setup
- [Operational Runbook](../../../docs/OPERATIONAL_RUNBOOK.md)
- [Payment Contract](../../../core/contracts/payment/README.md)
- [Escrow Contract](../../../core/contracts/escrow/README.md)
- [Refund Contract](../../../core/contracts/refund/README.md)
