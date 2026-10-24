# Security Policy & Threat Model

This document describes the security model, threat matrix, vulnerability reporting procedures, and bug bounty guidelines for the **Cypher GridPay** smart contracts repository.

---

## 1. Threat Model & Security Architecture

Cypher GridPay operates on Stellar Soroban smart contracts. The threat model is built around defense-in-depth across five distinct operational vectors:

### 1.1 Threat Vectors & Mitigations
| Threat Vector | Potential Impact | Inherent Mitigations & Controls |
|---|---|---|
| **Unauthorized State Mutation & Access Bypass** | Unauthorized fund release, fee parameter manipulation, or admin hijacking. | Strict `require_auth()` and role-based access control (RBAC); separated maintainer, arbiter, and user roles. |
| **Reentrancy & Cross-Contract Calls** | State desynchronization during token transfers or escrow callbacks. | Checks-Effects-Interactions (CEI) pattern; atomic transaction lifecycle enforcement. |
| **Arithmetic & Invariant Deviations** | Overflow/underflow or fee calculation truncations leading to unbalanced balance sheets. | Rust 2021 checked arithmetic (`checked_add`, `checked_mul`, `checked_sub`); non-zero denominator assertions. |
| **Time-Lock & Dispute Exploitation** | Griefing attacks on timelocks or premature refund claims before deadline expiration. | Monotonic block timestamp verification (`env.ledger().timestamp()`); non-malleable timeout intervals. |
| **Event Spoofing & Indexer Desync** | Emitting misleading topics or payloads causing off-chain indexer/webhook poisoning. | Structured Soroban topic emission with validated contract IDs and monotonic event indexing. |

---

## 2. Reporting a Vulnerability

If you discover a security vulnerability in this repository, please report it responsibly and do not disclose the issue publicly until a fix has been tested and deployed.

### 2.1 Reporting Channels
- **Security Email:** `security@facilpay.com`
- **PGP Key ID:** `0x4A8C9E1B2D3F4051` (Fingerprint available on request)
- **Response SLA:** We acknowledge all vulnerability reports within **24–48 hours**.

### 2.2 What to Include
When submitting a vulnerability report, please provide:
1. **Summary & Impact Assessment** — Clear description of the exploit scenario and potential financial/contract state impact.
2. **Affected Contracts & Functions** — Target contract IDs, source files, and specific functions (e.g., `payment`, `refund`, `arbitration`).
3. **Severity Classification** — Suggested CVSS v3.1 / DREAD scoring.
4. **Reproducible Proof of Concept (PoC)** — Unit test, Soroban CLI script, or Rust test case demonstrating execution.
5. **Remediation Recommendation** — Suggested code fix or mitigation steps.

---

## 3. Bug Bounty Program & Reward Tiers

We welcome independent security researchers and auditors to review our smart contract infrastructure. 

### 3.1 Severity Matrix & Reward Ranges
| Severity Tier | Definition / Impact | Typical Reward Range |
|---|---|---|
| **Critical** | Direct theft of user escrowed funds, permanent fund locking without recourse, or complete contract takeover. | **$2,500 – $5,000 USD** |
| **High** | Temporary fund freezing, unauthorized parameter tampering, or state corruption requiring contract redeployment. | **$1,000 – $2,500 USD** |
| **Medium** | Griefing attacks causing gas/fee exhaustion, logic flaws violating minor state invariants, or denial of service on specific functions. | **$300 – $1,000 USD** |
| **Low / Informational** | Non-exploitable logic discrepancies, missing event logs, or code quality improvements with security implications. | **$100 – $300 USD** |

*Note: Rewards are distributed via native crypto assets (USDC, XLM, or USDT) upon verified triage and fix deployment.*

---

## 4. Supported Versions & Scope

### 4.1 In-Scope Assets
- All Soroban Rust smart contracts in `contracts/` directory on `main`.
- Escrow lifecycle logic, multisig governance, and fee distribution algorithms.

### 4.2 Out-of-Scope
- Issues in third-party dependencies unless direct improper usage in contract code is proven.
- Theoretical attacks without executable proof of concept.
- Social engineering, phishing, or attacks against centralized hosting infrastructure.

---

## 5. Coordinated Vulnerability Disclosure Timeline

| Phase | Target SLA | Description |
|---|---|---|
| **1. Triage & Confirmation** | 48 Hours | Report acknowledged, severity determined, initial PoC reproduced in isolated sandbox. |
| **2. Patch Engineering** | 3 – 7 Days | Fix developed, verified against mutation tests, and reviewed by core maintainers. |
| **3. Downstream Notice** | 3 Days prior to release | Integrators, indexers, and frontend nodes receive confidential hotfix advisories. |
| **4. Public Advisory** | On Release | Release published with CVE / GHSA advisory and researcher credited. |

---

## 6. Security Best Practices for Integrators

1. **Verify Contract Hashes:** Always pin the WASM release hash (`wasm_hash.txt`) matching deployed on-chain bytecode.
2. **Handle Rejections Gracefully:** Listen to typed contract error codes (`ContractError`) rather than generic RPC failures.
3. **Subscribe to On-Chain Events:** Monitor Soroban emitted topics to verify transaction finality before releasing off-chain goods.

### 6.1 Signed messages and domain separation

Every off-chain signature the contracts verify with `ed25519_verify` is over
a digest bound to a canonical domain: the network, the contract instance and
the entry point. A signature therefore cannot be replayed on another network
(Testnet to Mainnet), against a redeployed contract, or at another entry
point. The contract always uses its own address and the current network when
recomputing the digest, so a signature made for any other domain fails
verification.

```text
domain_hash = sha256(
      "CYPHER_GRIDPAY_DOMAIN_V1"   24 bytes, ASCII (protocol + domain version)
   || network_id                  32 bytes: sha256(network passphrase)
   || contract_address            40 bytes: XDR ScVal of the contract address
   || action_tag                  ASCII entry-point tag, e.g. "channel_settle"
)

struct_hash = action-specific (below)

digest = sha256( 0x19 || 0x01 || domain_hash || struct_hash )   66-byte pre-image
```

The signer signs the 32-byte `digest` with ed25519.

| Action tag | Entry point | `struct_hash` |
|---|---|---|
| `channel_settle` | `settle_channel(channel_id, merchant_amount, nonce, signature)` | `sha256(xdr(u64 channel_id) \|\| xdr(i128 merchant_amount) \|\| xdr(u64 nonce))` |

`xdr(...)` is the XDR encoding of the value as a Soroban `ScVal` (what
`to_xdr` produces on-chain): 12 bytes for a `u64`, 20 bytes for an `i128`.

The payment contract exposes `get_domain_separator()` and
`channel_settle_digest(channel_id, merchant_amount, nonce)` so integrators can
check their encoding against the chain. A new signed action must get its own
tag, and changing the layout means a new `DOMAIN_V*` tag.

---
*Last Updated: September 2026*
