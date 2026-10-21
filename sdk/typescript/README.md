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

For errors returned by Soroban RPC, decode the contract domain and numeric
contract error code directly. Unknown codes remain available on the returned
error's `code` field.

```ts
import { decodeSorobanContractError } from "@cypher-gridpay/contracts-sdk";

try {
  await client.completePayment(paymentId);
} catch (error) {
  const contractError = decodeSorobanContractError(error, "payment");
  if (contractError) {
    console.error(contractError.code, contractError.errorName, contractError.resolution);
  }
  throw error;
}
```

## Events and transactions

`parseContractEvent` converts Soroban XDR topics and values into a named,
typed event. `pollContractEvents` reads one RPC page at a time and only saves
the cursor after every event callback succeeds, so applications can resume
without acknowledging unprocessed events.

`executeSorobanTransaction` provides the common simulate, restore, assemble,
submit, and exponential-backoff polling flow. Supply callbacks for signing,
transaction assembly, `RestoreFootprintOp` submission, and optional fee-bump
construction so the application retains control of keys and fee sources.

## Why generated bindings

The CLI emits `.d.ts` declarations derived from the contract's actual interface,
so a breaking contract change surfaces as a TypeScript compile error in your app
rather than a runtime failure in production. `npm run typecheck` on your
application is the cheapest possible contract-migration test.

## License

MIT
