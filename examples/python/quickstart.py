#!/usr/bin/env python3
"""Async Soroban quickstart for a Python merchant backend."""

import asyncio
import os
import sys

from stellar_sdk import Keypair

try:
    from cypher_gridpay_sdk import Payment, SorobanContractAsyncClient
except ImportError:  # pragma: no cover
    sys.exit("Install the SDK first: pip install -e sdk/python")

RPC_URL = os.environ.get("STELLAR_RPC_URL", "https://soroban-testnet.stellar.org")
NETWORK_PASSPHRASE = os.environ.get(
    "STELLAR_NETWORK_PASSPHRASE", "Test SDF Network ; September 2015"
)
CONTRACT_ID = os.environ.get("PAYMENT_CONTRACT_ID", "")
SECRET_KEY = os.environ.get("STELLAR_SECRET_KEY", "")


async def main() -> int:
    if not CONTRACT_ID:
        print("Set PAYMENT_CONTRACT_ID to the deployed payment contract.")
        return 0

    signer = Keypair.from_secret(SECRET_KEY) if SECRET_KEY else None
    client = SorobanContractAsyncClient(
        contract_id=CONTRACT_ID,
        rpc_url=RPC_URL,
        network_passphrase=NETWORK_PASSPHRASE,
        signer=signer,
    )
    try:
        payment_id = os.environ.get("PAYMENT_ID")
        if payment_id is None:
            print("Set PAYMENT_ID to query a payment.")
            return 0

        payment = await client.call("get_payment", int(payment_id), result_model=Payment)
        print(f"payment={payment.id} amount={payment.amount} status={payment.status}")

        if os.environ.get("GRIDPAY_COMPLETE", "").lower() not in ("1", "true", "yes"):
            return 0
        if signer is None:
            print("Set STELLAR_SECRET_KEY to submit a state-changing transaction.")
            return 1
        idempotency_key = os.environ.get("GRIDPAY_IDEMPOTENCY_KEY", "")
        if not idempotency_key:
            print("Set GRIDPAY_IDEMPOTENCY_KEY before submitting.")
            return 1

        result = await client.submit(
            "complete_payment",
            signer.public_key,
            int(payment_id),
            idempotency_key=idempotency_key,
        )
        print(f"payment completed: {result}")
        return 0
    finally:
        await client.close()


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
