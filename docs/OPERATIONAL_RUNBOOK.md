# Operational Runbook: Incident Response

This runbook is for the protocol team when a security vulnerability or live exploit affects the Cypher GridPay contracts. It walks through the order of operations: **contain → verify → investigate → recover → upgrade → resume**.

Keep it next to your signing setup. In an incident, the aim is to run the commands below without stopping to look anything up.

---

## 0. Before an incident: preparation checklist

Do these ahead of time. During an incident there is no time to set up keys.

- [ ] The **pauser** identity for the admin (orchestrator) contract is available to at least two on-call engineers (hardware wallet or multisig signer).
- [ ] The pauser address is a member of the multisig admin list on the **payment** and **escrow** contracts, and is the admin of the **refund** contract. `emergency_pause_all` forwards the pauser address to each child contract. If any child rejects it, **the whole call reverts and nothing is paused** (see [§2.3](#23-fallback-pause-each-contract-directly)).
- [ ] The **admin** identity for the admin contract (used to repoint child contracts after an upgrade) is available.
- [ ] Contract IDs are recorded below and in the team password manager.
- [ ] A keeper or cron job calls `ping` on the admin contract at least every few weeks so its instance TTL never lapses (see [§6](#6-keeping-contracts-alive-ttl)).

### Environment variables used in this runbook

```bash
export NETWORK=testnet                 # or: mainnet
export PAUSER=pauser                   # stellar keys identity name of the pauser
export ADMIN=admin                     # stellar keys identity name of the orchestrator admin

export ADMIN_ID=C...                   # admin / orchestrator contract
export PAYMENT_ID=C...                 # payment contract
export ESCROW_ID=C...                  # escrow contract
export REFUND_ID=C...                  # refund contract

export PAUSER_ADDR=$(stellar keys address $PAUSER)
export ADMIN_ADDR=$(stellar keys address $ADMIN)
```

To add identities, use `stellar keys add <name> --secret-key` (you will be prompted) or `stellar keys add <name> --seed-phrase`. Never paste secrets into shell history or chat.

---

## 1. Triage (first 5 minutes)

1. **Declare the incident** in the private security channel and name an incident lead. From this point, only the lead decides when to pause and when to resume.
2. **Record the facts:** the time, the suspected contract(s), the transaction hashes, and the attacker addresses.
3. **Decide the pause scope:**

| Situation | Action |
|---|---|
| Active exploit, or root cause unknown | **Global emergency pause** ([§2.1](#21-global-emergency-pause-preferred)) |
| Bug confined to one entry point, funds not at immediate risk | Function-level pause on the affected contract ([§2.4](#24-targeted-function-level-pause)) |
| Suspected compromise of the pauser/admin key | Global pause with a **different** key if possible, then treat key rotation as part of recovery |

When in doubt, pause everything. Resuming is cheap. Lost funds usually cannot be recovered.

---

## 2. Containment: triggering the pause

### 2.1 Global emergency pause (preferred)

This pauses payment, escrow, and refund in a single atomic transaction:

```bash
stellar contract invoke \
  --id $ADMIN_ID \
  --source-account $PAUSER \
  --network $NETWORK \
  -- emergency_pause_all \
  --pauser $PAUSER_ADDR \
  --reason "INC-2026-XX: <short description>"
```

The `reason` string is written to each child contract's `PauseState` and pause history. Put the incident ID in it.

### 2.2 Common failures

| Error | Meaning | Fix |
|---|---|---|
| `Error(Contract, #2)` `NotInitialized` | Admin contract was never initialized | Use [§2.3](#23-fallback-pause-each-contract-directly) |
| `Error(Contract, #3)` `Unauthorized` | `--pauser` is not the stored pauser | Check `$PAUSER_ADDR`, use the correct identity |
| Auth / signature error | Source account didn't sign for the pauser | Make sure `--source-account` is the pauser identity |
| Child contract error (e.g. `Unauthorized`, `MultiSigNotInitialized`) | Pauser is not an admin on that child | The whole transaction reverted. Use [§2.3](#23-fallback-pause-each-contract-directly) |
| Entry archived | Admin contract instance TTL lapsed | `stellar contract restore --id $ADMIN_ID ...`, then retry (see [§6](#6-keeping-contracts-alive-ttl)) |

### 2.3 Fallback: pause each contract directly

If the orchestrator is unavailable, call `pause_contract` on each child with an identity that is an admin there. Run all three commands, even if one of them fails:

```bash
for ID in $PAYMENT_ID $ESCROW_ID $REFUND_ID; do
  stellar contract invoke \
    --id $ID \
    --source-account $PAUSER \
    --network $NETWORK \
    -- pause_contract \
    --admin $PAUSER_ADDR \
    --reason "INC-2026-XX: <short description>"
done
```

### 2.4 Targeted function-level pause

For a narrowly scoped bug, pause only the affected entry point. Each child contract supports this:

```bash
stellar contract invoke \
  --id $PAYMENT_ID \
  --source-account $PAUSER \
  --network $NETWORK \
  -- pause_function \
  --admin $PAUSER_ADDR \
  --function_name "<function_name>" \
  --reason "INC-2026-XX: <short description>"
```

---

## 3. Verify the pause took effect

Do not assume the pause worked. Check every contract. View calls use `--send=no`, so they simulate without submitting a transaction:

```bash
for ID in $PAYMENT_ID $ESCROW_ID $REFUND_ID; do
  echo "== $ID"
  stellar contract invoke --id $ID --source-account $PAUSER --network $NETWORK --send=no \
    -- get_pause_state
done
```

Each result must show `"globally_paused": true`. For a function-level pause, the function must appear in `paused_functions`. Also check that `paused_by` and `pause_reason` match what you submitted.

Escrow also keeps a queryable pause history:

```bash
stellar contract invoke --id $ESCROW_ID --source-account $PAUSER --network $NETWORK --send=no \
  -- get_pause_history --limit 10 --offset 0
```

As a final check, try a state-changing call as a regular user in simulation (for example `create_payment` with `--send=no`). It should be rejected while paused.

---

## 4. Post-incident inspection

Once the contracts are paused, the goal is to understand what happened **without changing state**.

### 4.1 Collect on-chain evidence

```bash
# Contract events emitted since a given ledger (payment, escrow, refund, admin)
stellar events --network $NETWORK --start-ledger <LEDGER_BEFORE_INCIDENT> \
  --id $PAYMENT_ID --id $ESCROW_ID --id $REFUND_ID --id $ADMIN_ID --output json > incident-events.json
```

For individual transactions, look up each suspicious hash in a block explorer (e.g. stellar.expert) or query it through RPC `getTransaction`. Save the full result XDR with the incident notes.

### 4.2 Snapshot affected state

Record the state of every affected record before any recovery action. Use the read-only getters, for example:

```bash
stellar contract invoke --id $PAYMENT_ID --source-account $PAUSER --network $NETWORK --send=no \
  -- get_payment --payment_id <ID>
stellar contract invoke --id $ESCROW_ID  --source-account $PAUSER --network $NETWORK --send=no \
  -- get_escrow --escrow_id <ID>
stellar contract invoke --id $REFUND_ID  --source-account $PAUSER --network $NETWORK --send=no \
  -- get_refund --refund_id <ID>
```

Also record the schema version of each contract (`get_schema_version`) and the wasm hash currently deployed:

```bash
stellar contract fetch --id $PAYMENT_ID --network $NETWORK -o payment-deployed.wasm
sha256sum payment-deployed.wasm
```

Use `stellar contract info interface --id <ID> --network $NETWORK` to confirm which interface is live.

### 4.3 Checklist

- [ ] Root cause identified and reproduced in a unit test (`cargo test` in `core/` or `orchestrator/`)
- [ ] Complete list of affected payment / escrow / refund IDs
- [ ] Token balances held by each contract reconciled against the stored records
- [ ] Decision made: can the fix be applied with configuration or admin calls, or does it need new code ([§5.2](#52-upgrading-a-contract))?

---

## 5. Recovery

### 5.1 Configuration-only fixes

If the fix is a parameter change (limits, fees, allow-lists, and so on), apply it with the relevant admin function while the contracts are still paused. Then go to [§5.4](#54-resuming-operations).

### 5.2 Upgrading a contract

The child contracts do **not** expose an in-place `upgrade` entry point. A code fix is therefore rolled out by deploying a new contract instance and repointing the orchestrator to it:

1. **Fix and test.** Add a regression test that reproduces the exploit, then run the full suite:
   ```bash
   make test          # builds and tests core/ and orchestrator/
   ```
2. **Build and upload the new wasm:**
   ```bash
   cd core && stellar contract build
   stellar contract upload \
     --wasm target/wasm32v1-none/release/payments.wasm \
     --source-account $ADMIN --network $NETWORK
   # → prints the new WASM_HASH
   ```
3. **Deploy and initialize the new instance:**
   ```bash
   NEW_PAYMENT_ID=$(stellar contract deploy --wasm-hash <WASM_HASH> \
     --source-account $ADMIN --network $NETWORK)
   stellar contract invoke --id $NEW_PAYMENT_ID --source-account $ADMIN --network $NETWORK \
     -- initialize --admin <CHILD_ADMIN_ADDR>
   ```
   Make sure the pauser is an admin of the new instance. Otherwise, future emergency pauses through the orchestrator will fail.
4. **Deploy the new instance paused**, so it cannot be used before it is verified:
   ```bash
   stellar contract invoke --id $NEW_PAYMENT_ID --source-account $PAUSER --network $NETWORK \
     -- pause_contract --admin $PAUSER_ADDR --reason "INC-2026-XX: staged upgrade"
   ```
5. **Repoint the orchestrator** (admin key required):
   ```bash
   stellar contract invoke --id $ADMIN_ID --source-account $ADMIN --network $NETWORK \
     -- set_payment_contract --admin $ADMIN_ADDR --payment_contract $NEW_PAYMENT_ID
   # likewise: set_escrow_contract / set_refund_contract
   ```
6. **Migrate data** if the storage layout changed. Follow [STORAGE_VERSIONING.md](./STORAGE_VERSIONING.md) (`migrate_schema`, and `migrate_escrow` / `migrate_escrow_batch` for escrow).
7. **Update integrations.** Frontends, indexers, and dependent contracts must use the new contract ID.

The old instance stays paused permanently. Funds held there must be released through its existing admin or dispute paths before it is abandoned.

### 5.3 Restoring archived entries

If any entry was archived during the incident window, restore it before resuming:

```bash
stellar contract restore --id <CONTRACT_ID> --source-account $ADMIN --network $NETWORK
# for a specific persistent key:
stellar contract restore --id <CONTRACT_ID> --key-xdr <KEY_XDR> --durability persistent \
  --source-account $ADMIN --network $NETWORK
```

### 5.4 Resuming operations

Only the incident lead approves resuming. First confirm that the fix is deployed and verified. Then:

```bash
stellar contract invoke \
  --id $ADMIN_ID \
  --source-account $PAUSER \
  --network $NETWORK \
  -- emergency_unpause_all \
  --pauser $PAUSER_ADDR
```

`emergency_unpause_all` is idempotent. It is safe to rerun if you are unsure whether it succeeded, and a single call clears the pause no matter how many times the contracts were paused.

For function-level pauses, use `unpause_function --admin $PAUSER_ADDR --function_name "<name>"` on each affected contract.

Repeat [§3](#3-verify-the-pause-took-effect) and confirm `"globally_paused": false` on every contract. Then run a small end-to-end transaction on the live network.

---

## 6. Keeping contracts alive (TTL)

On Soroban, a contract whose instance TTL lapses is archived and must be restored before anyone can call it. **This includes the emergency pause.**

- Every administrative call on the admin contract extends its instance TTL automatically.
- Anyone can extend it explicitly with the permissionless `ping` endpoint:
  ```bash
  stellar contract invoke --id $ADMIN_ID --source-account <any-funded-identity> --network $NETWORK -- ping
  ```
- Child contract instances can be extended with the CLI:
  ```bash
  stellar contract extend --id $PAYMENT_ID --ledgers-to-extend 535680 \
    --source-account $ADMIN --network $NETWORK
  ```

Schedule a `ping` call (and `extend` calls for the child contracts) at least monthly.

---

## 7. Post-mortem

Within 5 business days of the incident, publish an internal post-mortem that covers:

- A timeline, from detection to pause, verification, fix, and resume, with ledger numbers and tx hashes
- The root cause and why existing tests did not catch it
- User impact (affected IDs and amounts) and the remediation plan
- Follow-up actions, such as new tests, monitoring, and runbook changes

Update this runbook with anything that slowed the response down.

---

## See Also

- [Admin contract README](../orchestrator/contracts/admin/README.md)
- [Storage versioning](./STORAGE_VERSIONING.md)
- [Deployment](./DEPLOYMENT.md)
- [Security policy](../SECURITY.md)
