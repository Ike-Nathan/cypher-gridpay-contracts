# Storage Restoration Guide

Soroban ledger entries have a limited time-to-live (TTL). When a contract's
persistent entries are not touched for long enough, they are **archived**: the
entry data is moved out of the live ledger state and must be restored before the
contract can read or write it again.

This guide explains how to detect archived entries and how to restore them with
the Stellar CLI before invoking a contract.

## When restoration is needed

- A contract invocation fails because a required ledger entry is archived
  (its TTL expired after a period of inactivity).
- You are about to invoke a contract whose persistent state has not been used
  for a long time and you want to avoid a failed call.
- You are recovering a contract after a long period of inactivity.

Archived entries are **not lost** — the data still exists and can be restored.
However, the contract cannot use an archived entry until it has been restored,
so **state must be restored before contract invocation**.

## Prerequisites

- Stellar CLI installed (`stellar --version`).
- The contract ID of the contract you are operating on.
- The ledger key(s) you need to restore.
- A funded source identity for the network you are targeting.

## 1. Inspect contract data

List the entries stored by a contract to find the keys you need:

```bash
stellar contract data \
  --id <CONTRACT_ID> \
  --network testnet
```

Inspect a specific ledger key, including its durability (persistent vs.
temporary) and current TTL:

```bash
stellar contract data \
  --id <CONTRACT_ID> \
  --key <LEDGER_KEY_XDR> \
  --network testnet
```

If an entry is archived, it will not appear in the live data listing and
invocations that depend on it will fail until it is restored.

## 2. Restore archived entries

Restore one or more archived ledger keys for a contract:

```bash
stellar contract restore \
  --id <CONTRACT_ID> \
  --key <LEDGER_KEY_XDR> \
  --network testnet \
  --source <IDENTITY>
```

Restore multiple keys in a single call by repeating `--key`:

```bash
stellar contract restore \
  --id <CONTRACT_ID> \
  --key <LEDGER_KEY_XDR_1> \
  --key <LEDGER_KEY_XDR_2> \
  --network testnet \
  --source <IDENTITY>
```

### Required arguments

| Argument    | Description                                                        |
| ----------- | ------------------------------------------------------------------ |
| `--id`      | Contract ID whose entries are being restored.                      |
| `--key`     | Ledger key (XDR) to restore. Repeat for multiple keys.             |
| `--network` | Target network (`testnet`, `mainnet`, or a custom network config). |
| `--source`  | Source identity used to sign and pay for the restore operation.    |

## 3. Verify and invoke

After restoring, re-run the inspection command to confirm the entry is live,
then proceed with your contract invocation:

```bash
stellar contract data \
  --id <CONTRACT_ID> \
  --key <LEDGER_KEY_XDR> \
  --network testnet

stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <IDENTITY> \
  -- <FUNCTION> <ARGS>
```

## Step-by-step summary

1. Identify the failing or at-risk contract and its contract ID.
2. Inspect contract data to locate the archived ledger key(s).
3. Run `stellar contract restore` with the contract ID, key(s), network, and
   source identity.
4. Verify the entry is live with `stellar contract data`.
5. Invoke the contract only after restoration succeeds.

## Related documentation

- [Storage Versioning Guide](STORAGE_VERSIONING.md)
