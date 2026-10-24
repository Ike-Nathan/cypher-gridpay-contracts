# Formal Economic Invariants & Balance Conservation

This specification formally establishes the financial and economic invariants governing the Cypher GridPay smart contract protocol across the `core` (Payment, Escrow, Refund) and `orchestrator` workspaces.

---

## 📐 Primary Protocol Invariant: Balance Conservation

The foundational economic law of Cypher GridPay is strict conservation of value across all on-chain operations. Under no circumstances may tokens held by the protocol contracts exceed or fall short of active liabilities:

$$\text{Total Locked Contract Tokens} = \sum \text{Active Escrows} + \sum \text{Pending Settlements} + \sum \text{Merchant Accumulated Balances} + \text{Accumulated Protocol Fees}$$

### Mathematical Definition

For any supported token contract $T$:

$$\mathcal{B}_{\text{contract}}(T) = \sum_{e \in \mathcal{E}_{\text{active}}} \text{amount}(e) + \sum_{s \in \mathcal{S}_{\text{pending}}} \text{amount}(s) + \sum_{m \in \mathcal{M}} \text{accumulated}(m, T) + \mathcal{F}_{\text{accumulated}}(T)$$

Where:
- $\mathcal{B}_{\text{contract}}(T)$: Token balance held at `env.current_contract_address()` for token $T$.
- $\mathcal{E}_{\text{active}}$: The set of all escrows where $\text{status} \in \{\text{Locked}, \text{Disputed}\}$.
- $\mathcal{S}_{\text{pending}}$: The set of all settlements currently within the finality delay window.
- $\mathcal{M}$: The set of all registered merchants with deferred payout schedules.
- $\mathcal{F}_{\text{accumulated}}(T)$: Protocol fees collected from completed transactions that have not yet been swept via `sweep_fees`.

---

## 🛡️ Secondary Invariants

### 1. Solvency Invariant (No Deficit)
No contract operation (release, refund, payout, or fee sweep) may execute if the resulting contract balance would drop below the sum of remaining liabilities:
$$\forall t, \quad \mathcal{B}_{\text{contract}}(t) \ge \text{Total Unresolved Liabilities}(t)$$

### 2. No-Leakage & Non-Inflation Invariant
Tokens can only enter contract custody via explicit customer deposits or authorized payment transfers:
$$\Delta \mathcal{B}_{\text{contract}} = \text{Deposits} - \text{Disbursements}$$
No function creates, mints, or burns external tokens internally.

### 3. Non-Negative State Invariant
All quantitative accounting values in storage must be non-negative:
$$\forall e, \quad \text{amount}(e) \ge 0$$
$$\forall m, \quad \text{accumulated}(m) \ge 0$$
$$\mathcal{F}_{\text{accumulated}} \ge 0$$

### 4. Fee Sweep Upper Bound
Fee sweeps are strictly bounded by verified accumulated protocol revenue:
$$\text{sweep\_amount} \le \mathcal{F}_{\text{accumulated}}$$
Fee sweeps cannot draw from active escrow reserves or pending customer/merchant settlements.

### 5. Escrow State Invariance
An escrow transitioned to a terminal status ($\text{Released}, \text{Resolved}, \text{Cancelled}, \text{Refunded}$) is strictly immutable and cannot be re-entered or disbursed again:
$$\text{TerminalStatus}(e) \implies \Delta \text{amount}(e) = 0$$

---

## 🧪 Invariant Verification in Contract Testing

The test suites enforce these invariants through explicit balance conservation assertions across all lifecycle stages:

```rust
/// Formal invariant verification asserting token conservation across payment settlement.
pub fn assert_protocol_balance_conservation(
    env: &Env,
    token: &Address,
    expected_liabilities: i128,
) {
    let token_client = token::Client::new(env, token);
    let contract_balance = token_client.balance(&env.current_contract_address());
    assert_eq!(
        contract_balance, expected_liabilities,
        "Invariant violation: contract balance does not match total protocol liabilities"
    );
}
```
