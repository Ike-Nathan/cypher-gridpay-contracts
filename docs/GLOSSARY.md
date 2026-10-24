# Glossary

## Admin Orchestrator

The top-level coordinator contract located in `orchestrator/contracts/admin` that manages cross-contract operations, emergency pauses, dynamic address updates, and access control across the Payment, Escrow, and Refund contracts.

## Arbitrator

A registered, trusted entity or account capable of resolving disputed escrows and appeals within the refund contract. Arbitrators maintain on-chain reputation scores and can be subject to stake slashing for fraudulent or non-responsive rulings.

## Circuit Breaker

An automated safety mechanism that halts contract state changes or limits transaction volume when certain volatility or aggregate refund thresholds are breached within a rolling window.

## Clawback

An admin-controlled emergency fund-recovery mechanism for the escrow contract. When normal resolution paths (release, dispute, refund) are unavailable — for example, due to fraud, a compliance hold, or an irrecoverable deadlock — a multisig admin can forcibly recover the full escrow balance and transfer it to their own address.

### Lifecycle

Clawback follows a strict three-phase sequence:

1. **Initiate** — An admin calls `initiate_clawback(admin, escrow_id, reason_hash, delay_seconds)`. The `reason_hash` is a 32-byte Keccak-256 (or equivalent) hash of an off-chain document that records the justification. The mandatory `delay_seconds` must be at least 86,400 seconds (24 hours), giving all parties a window to contest or seek remediation before funds move. A unique `request_id` is returned and stored on-chain.

2. **Execute** — After the delay elapses, any admin can call `execute_clawback(admin, request_id)`. The full escrow amount is transferred from the contract to the **admin's address** (not to the original customer), and the escrow status is updated to `Resolved`.

3. **Cancel** — Any admin can call `cancel_clawback(admin, request_id)` at any time before execution to abort the request. Cancellation is final — a cancelled request cannot be re-activated. To retry, a new initiation must be filed.

### Key constraints

- Only registered multisig admins may initiate, execute, or cancel a clawback.
- Only one active (non-executed, non-cancelled) clawback request may exist per escrow at a time. A second initiation for the same escrow while a live request exists returns `AlreadyProcessed`.
- Executing before the delay elapses returns `EscrowError::TimelockNotElapsed`.
- No fees are deducted — the entire locked amount transfers to the admin.

### Error reference

| Error | Cause |
|-------|-------|
| `AlreadyProcessed` | A clawback request for this escrow is already active |
| `NotReady` | The mandatory delay has not yet elapsed |
| `Unauthorized` | Caller is not a registered admin |

---

## Core Protocol Terms

### Escrow

A smart contract mechanism that holds tokens in escrow until predefined conditions are met. In Cypher GridPay, escrows are created when a customer initiates a payment and are released to the merchant upon completion or refunded to the customer if the payment is cancelled or disputed.

### Payment Channel

An off-chain mechanism for high-frequency micropayments between two parties. Balance updates are signed off-chain and only the final state is submitted on-chain. Each update includes a sequence number to prevent replay attacks.

### Settlement

The process of converting a locked payment into a final transfer to the merchant. Settlement occurs when the payment is completed by an admin, triggering the release of escrowed funds minus any applicable fees.

### Multi-Sig (Multi-Signature)

A security mechanism requiring multiple authorized signatures before executing sensitive operations. The Cypher GridPay protocol uses configurable multi-sig thresholds for admin operations like fee sweeps, contract pause, and admin changes.

### Fee Sweep

The accumulated protocol fees (from payment processing) that can be withdrawn by authorized admins. Sweeps are strictly limited to the accumulated fee balance and cannot touch merchant escrowed or unfinalized funds.

### Rate Limit

A sliding-window mechanism that restricts the number of payments a single address can create within a given time period. Rate limits prevent ledger spam and denial-of-service attacks.

### Circuit Breaker

An automatic halt mechanism that pauses contract operations when anomalous conditions are detected (e.g., excessive refund volume, oracle failure). Circuit breakers protect funds during potential attack scenarios.

### Arbitration

A dispute resolution process where a neutral third party (arbitrator) evaluates evidence from both sides and determines the outcome of a refund or escrow dispute. Arbitration requires staking and has time-bound deadlines.

### Vesting Schedule

A time-based release mechanism for escrowed funds. Vesting schedules can be linear, cliff-based, or custom, and can be accelerated by authorized parties under specific conditions.

### Schema Version

A numeric identifier stored in contract storage that tracks the current data layout version. Schema versioning enables safe data migrations when contract logic is upgraded.

### Checks-Effects-Interactions

A security pattern where all state checks are performed first, then state changes are applied, and finally external calls are made. This pattern prevents reentrancy attacks.

### Reentrancy

An attack where a malicious contract calls back into the original contract before the first invocation completes, potentially manipulating state or draining funds. Prevented by following the Checks-Effects-Interactions pattern.

### Front-Running

The practice of observing pending transactions in the mempool and submitting a competing transaction with higher gas to execute first. In Cypher GridPay, front-running mitigations include admin authorization requirements and deadline extensions for dispute evidence.

### Griefing

An attack where an attacker causes inconvenience or financial loss to other users without direct benefit to themselves. Examples include submitting frivolous disputes or spamming the ledger.

### Oracle

An external data source that provides information to smart contracts, such as token price feeds. Cypher GridPay uses oracles for currency conversion and includes staleness checks to prevent manipulation.

### Deadline Extension

An automatic extension of dispute evidence submission deadlines when new evidence is submitted close to the deadline. This prevents front-running attacks on evidence submission.
