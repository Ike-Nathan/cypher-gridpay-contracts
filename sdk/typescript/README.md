# @cypher-gridpay/contracts-sdk

Auto-generated, type-safe TypeScript client bindings for the Cypher GridPay
Stellar Soroban smart contracts.

This package is **generated output**. Everything under `src/generated/` is
produced by [`scripts/generate-ts-bindings.sh`](../../scripts/generate-ts-bindings.sh)
from the compiled contract WASM — do not hand-edit it.

## Install

```bash
npm install @cypher-gridpay/contracts-sdk @stellar/stellar-sdk
```

`@stellar/stellar-sdk` is a peer dependency (`>=13`); the SDK does not bundle it.

## Generate from source

```bash
git clone https://github.com/johnephraim949-web/cypher-gridpay-contracts
cd cypher-gridpay-contracts/sdk/typescript

../../scripts/generate-ts-bindings.sh   # builds contracts, then regenerates
npm install
npm run build
```

## Usage

Each contract is exposed as a namespace with its generated client:

```ts
import {
  PaymentsClient,
  payments,
} from "@cypher-gridpay/contracts-sdk";
import {
  Contract,
  Networks,
  Keypair,
  Server,
  nativeToScVal,
} from "@stellar/stellar-sdk";

const server = new Server(Networks.TESTNET);
const keypair = Keypair.random();

const client = new PaymentsClient({
  contractId: paymentContractId,
  publicKey: keypair.publicKey(),
  server,
  networkPassphrase: Networks.TESTNET,
  source: keypair,
});

const payment = await client.getPayment(paymentId);
```

Read-only usage — pass no `source` so the client will not sign:

```ts
const client = new PaymentsClient({
  contractId: paymentContractId,
  publicKey: viewerPublicKey,
  server,
  networkPassphrase: Networks.TESTNET,
});

const payment = await client.getPayment(paymentId);
```

## Signing payment-channel settlements

`settle_channel` verifies an ed25519 signature over a domain-separated digest
(the full layout is in [SECURITY.md](../../SECURITY.md#61-signed-messages-and-domain-separation)):

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

Computing it client-side:

```ts
import { Address, Networks, hash, nativeToScVal } from "@stellar/stellar-sdk";

function channelSettleDigest(
  networkPassphrase: string,
  contractId: string,
  channelId: bigint,
  merchantAmount: bigint,
  nonce: bigint,
): Buffer {
  const domainHash = hash(Buffer.concat([
    Buffer.from("CYPHER_GRIDPAY_DOMAIN_V1", "ascii"),
    hash(Buffer.from(networkPassphrase)),               // network_id
    new Address(contractId).toScVal().toXDR(),          // 40 bytes
    Buffer.from("channel_settle", "ascii"),
  ]));
  const structHash = hash(Buffer.concat([
    nativeToScVal(channelId, { type: "u64" }).toXDR(),
    nativeToScVal(merchantAmount, { type: "i128" }).toXDR(),
    nativeToScVal(nonce, { type: "u64" }).toXDR(),
  ]));
  return hash(Buffer.concat([Buffer.from([0x19, 0x01]), domainHash, structHash]));
}

const signature = customerKeypair.sign(
  channelSettleDigest(Networks.TESTNET, paymentContractId, channelId, amount, nonce),
);
```

To check the encoding, compare it with the contract's own
`channel_settle_digest(channel_id, merchant_amount, nonce)` (a read-only call).

## Error handling

Contract errors surface as `Error` instances whose `code` is the generated
`ErrorCode` enum member. Switch on the enum rather than string-matching:

```ts
import { payments } from "@cypher-gridpay/contracts-sdk";

try {
  await client.completePayment(paymentId);
} catch (err) {
  const code = (err as { code?: unknown }).code;
  if (code === payments.errors.PaymentNotFound) {
    // ...
  }
  throw err;
}
```

## Why generated bindings

The CLI emits `.d.ts` declarations derived from the contract's actual interface,
so a breaking contract change surfaces as a TypeScript compile error in your app
rather than a runtime failure in production. `npm run typecheck` on your
application is the cheapest possible contract-migration test.

## License

MIT

## Strongly-Typed ContractError Decoder & Diagnostics

The SDK provides automated decoders to parse raw Soroban simulation reverts (`ScErrorType::Contract`) and on-chain transaction errors into structured, user-friendly diagnostic objects with remediation guidance:

```typescript
import {
  decodeGridPayError,
  isContractError,
  isRetryableError,
  formatDiagnosticError,
} from "@cypher-gridpay/contracts-sdk";

try {
  await client.submitPayment(params);
} catch (rawError) {
  const decoded = decodeGridPayError(rawError, "payment");
  if (decoded) {
    console.error(`[${decoded.contract.toUpperCase()}::${decoded.name} (#${decoded.code})]`);
    console.error(`Category: ${decoded.category}`);
    console.error(`Message: ${decoded.message}`);
    console.error(`Remediation: ${decoded.resolution}`);

    if (decoded.retryable) {
      console.log("Transient failure detected — eligible for retry after backoff.");
    }
  } else {
    console.error(formatDiagnosticError(rawError));
  }
}
```

### Supported Contract Catalogs:
- **PaymentContract**: `BasicError` (100–126), `PaymentError` (200–224), `SubscriptionError` (300–318), `ProposalError` (400–406), `FeatureError` (500–541)
- **EscrowContract**: `BasicError` (100–114), `EscrowError` (200–230), `ActionError` (300–316)
- **RefundContract**: `CoreError` (1–33), `ExtError` (34–51)
- **AdminContract**: `AdminError` (1–5)
