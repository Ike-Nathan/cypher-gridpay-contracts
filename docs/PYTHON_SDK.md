# Python SDK

The Python SDK wraps Soroban with `SorobanServerAsync` through Stellar SDK's async contract client. Responses are converted from ScVal/XDR into native Python values and validated with Pydantic v2 models for `Payment`, `Escrow`, `Refund`, and `SystemStatus`.

## Installation

```bash
pip install -e sdk/python

# Optional: shared idempotency storage for FastAPI/Celery workers
pip install -e 'sdk/python[redis]'

# Generate the lower-level contract bindings when contract WASM is built
./scripts/generate-python-bindings.sh
```

Generated bindings are placed under `sdk/python/cypher_gridpay_sdk/bindings`. The maintained async wrapper and models are not overwritten by regeneration.

## Async usage

```python
import asyncio
import os

from stellar_sdk import Keypair
from cypher_gridpay_sdk import Payment, SorobanContractAsyncClient

async def main():
    client = SorobanContractAsyncClient(
        contract_id=os.environ["PAYMENT_CONTRACT_ID"],
        rpc_url="https://soroban-testnet.stellar.org",
        network_passphrase="Test SDF Network ; September 2015",
        signer=Keypair.from_secret(os.environ["STELLAR_SECRET_KEY"]),
    )
    try:
        payment = await client.call("get_payment", 42, result_model=Payment)
        print(payment.status, payment.amount)

        await client.submit(
            "complete_payment",
            client.signer.public_key,
            payment.id,
            idempotency_key="order-42-completion-v1",
        )
    finally:
        await client.close()

asyncio.run(main())
```

`call()` simulates the contract invocation and does not submit a transaction. `submit()` requires a signing key and an idempotency key. The SDK converts Python integers, addresses, bytes, enums, lists, dictionaries, and Pydantic models to Soroban ScVals; pass `argument_types` when an integer's XDR width cannot be inferred from its argument name.

## FastAPI and Celery

Use one Redis-backed store across worker processes so completed results are shared. The in-memory default is intended for local development and tests only.

```python
from redis.asyncio import Redis
from stellar_sdk import Keypair
from cypher_gridpay_sdk import RedisIdempotencyStore, SorobanContractAsyncClient

redis = Redis.from_url("redis://localhost:6379/0")
client = SorobanContractAsyncClient(
    contract_id="C...",
    rpc_url="https://soroban-testnet.stellar.org",
    network_passphrase="Test SDF Network ; September 2015",
    signer=Keypair.from_secret("S..."),
    idempotency_store=RedisIdempotencyStore(redis),
)

async def complete_payment(payment_id: int, request_id: str):
    return await client.submit(
        "complete_payment",
        client.signer.public_key,
        payment_id,
        idempotency_key=f"payment-complete:{request_id}",
    )
```

Keep idempotency keys stable across retries and unique to one logical operation. Reusing a key with different function arguments raises `ValueError`. Retries resubmit the same signed transaction envelope; they do not construct a fresh transaction for each attempt.

## Models

`Payment`, `Escrow`, and `Refund` mirror the corresponding contract records. `SystemStatus` combines the common pause state with optional refund circuit-breaker fields for service health endpoints. Contract enum variants are represented as strings in the Pydantic models.

For private invoices, use `new_invoice_secret()` and
`ephemeral_invoice_commitment(vault_address, secret)` from the SDK. Keep the returned
secret off-chain until `complete_private_payment` reveals it; never reuse a secret.

See [examples/python/quickstart.py](../examples/python/quickstart.py) for a complete executable example.
