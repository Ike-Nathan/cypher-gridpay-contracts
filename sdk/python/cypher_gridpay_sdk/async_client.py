from __future__ import annotations

import asyncio
import hashlib
import json
import secrets
from contextlib import asynccontextmanager
from typing import Any, AsyncContextManager, Callable, Protocol, Sequence, TypeVar

from pydantic import BaseModel
from stellar_sdk import Keypair, SorobanServerAsync, scval, xdr
from stellar_sdk.contract import ContractClientAsync
from stellar_sdk.contract.exceptions import TransactionStillPendingError
from stellar_sdk.exceptions import ConnectionError as StellarConnectionError

from .models import ContractModel

T = TypeVar("T", bound=ContractModel)


def new_invoice_secret() -> bytes:
    return secrets.token_bytes(32)


def ephemeral_invoice_commitment(vault: str, secret: bytes) -> bytes:
    if len(secret) != 32:
        raise ValueError("Invoice secret must be exactly 32 bytes")
    preimage = scval.to_address(vault).to_xdr_bytes()
    preimage += scval.to_bytes(secret).to_xdr_bytes()
    return hashlib.sha256(preimage).digest()

_ENUM_FIELDS = {
    "auto_resolve_in_favor_of",
    "currency",
    "reason_code",
    "status",
}
_UINT32_FIELDS = {"bps", "fee_bps", "share_bps", "trip_count", "version"}
_MODEL_INTEGER_TYPES = {
    "Escrow": {"amount": "i128", "fee_bps": "i128"},
    "Payment": {"amount": "i128", "refunded_amount": "i128"},
    "Refund": {
        "amount": "i128",
        "original_payment_amount": "i128",
    },
}
_UINT64_FIELDS = {
    "id",
    "payment_id",
    "escrow_id",
    "refund_id",
    "created_at",
    "expires_at",
    "requested_at",
    "approved_at",
    "rejected_at",
    "processed_at",
    "appeal_deadline",
    "release_timestamp",
    "dispute_started_at",
    "last_activity_at",
    "hold_period_until",
    "expiry_timestamp",
    "escalated_at",
    "escalation_timeout",
    "evidence_deadline",
    "paused_at",
    "tripped_at",
    "resets_at",
    "start_ts",
    "stop_ts",
    "nonce",
    "interval",
    "duration",
    "expiration_duration",
    "min_hold_period",
}
_FUNCTION_ARGUMENT_NAMES = {
    "create_payment": (
        "customer",
        "merchant",
        "amount",
        "token",
        "currency",
        "expiration_duration",
        "metadata",
    ),
    "create_private_payment": (
        "customer",
        "ephemeral_invoice_commitment",
        "amount",
        "token",
        "currency",
        "expiration_duration",
        "metadata",
    ),
    "open_streaming_channel": (
        "customer",
        "merchant",
        "token",
        "deposit",
        "flow_rate_per_sec",
        "start_ts",
        "stop_ts",
    ),
    "complete_private_payment": ("vault", "secret", "payment_id"),
    "set_relayer_stipend_bps": ("admin", "relayer_stipend_bps"),
    "get_relayer_stipend": ("relayer", "token"),
    "claim_relayer_stipend": ("relayer", "token"),
    "get_streaming_claimable": ("channel_id",),
    "get_streaming_channel": ("channel_id",),
    "claim_streaming_channel": ("merchant", "channel_id"),
    "close_streaming_channel": ("customer", "channel_id"),
    "create_escrow": (
        "customer",
        "merchant",
        "amount",
        "token",
        "release_timestamp",
        "min_hold_period",
        "expiry_timestamp",
        "auto_refund_on_expiry",
    ),
    "request_refund": (
        "merchant",
        "payment_id",
        "customer",
        "amount",
        "original_payment_amount",
        "token",
        "reason",
        "reason_code",
        "payment_created_at",
    ),
    "get_payment": ("payment_id",),
    "complete_payment": ("admin", "payment_id"),
    "get_escrow": ("escrow_id",),
    "get_refund": ("refund_id",),
    "get_pause_state": (),
    "get_circuit_breaker_state": (),
}


class IdempotencyStore(Protocol):
    def lock(self, key: str) -> AsyncContextManager[None]: ...

    async def get(self, key: str) -> tuple[str, str] | None: ...

    async def set(self, key: str, fingerprint: str, result_xdr: str) -> None: ...


class MemoryIdempotencyStore:
    def __init__(self) -> None:
        self._records: dict[str, tuple[str, str]] = {}
        self._locks: dict[str, asyncio.Lock] = {}

    @asynccontextmanager
    async def lock(self, key: str):
        lock = self._locks.setdefault(key, asyncio.Lock())
        async with lock:
            yield

    async def get(self, key: str) -> tuple[str, str] | None:
        return self._records.get(key)

    async def set(self, key: str, fingerprint: str, result_xdr: str) -> None:
        self._records[key] = (fingerprint, result_xdr)


class RedisIdempotencyStore:
    """Redis-backed store for sharing idempotency records across workers."""

    def __init__(self, redis: Any, prefix: str = "gridpay:idempotency:", ttl: int = 86_400):
        self.redis = redis
        self.prefix = prefix
        self.ttl = ttl

    def lock(self, key: str) -> AsyncContextManager[None]:
        return self.redis.lock(f"{self.prefix}{key}:lock", timeout=600, blocking_timeout=60)

    async def get(self, key: str) -> tuple[str, str] | None:
        value = await self.redis.get(f"{self.prefix}{key}")
        if value is None:
            return None
        record = json.loads(value)
        return record["fingerprint"], record["result_xdr"]

    async def set(self, key: str, fingerprint: str, result_xdr: str) -> None:
        value = json.dumps({"fingerprint": fingerprint, "result_xdr": result_xdr})
        await self.redis.set(f"{self.prefix}{key}", value, ex=self.ttl)


class IdempotencyMiddleware:
    def __init__(self, store: IdempotencyStore | None = None, max_attempts: int = 3):
        self.store = store or MemoryIdempotencyStore()
        self.max_attempts = max_attempts

    async def run(
        self,
        key: str,
        fingerprint: str,
        operation: Callable[[], Any],
    ) -> xdr.SCVal:
        if not key.strip():
            raise ValueError("An idempotency_key is required for every submission")
        async with self.store.lock(key):
            cached = await self.store.get(key)
            if cached:
                cached_fingerprint, result_xdr = cached
                if cached_fingerprint != fingerprint:
                    raise ValueError("Idempotency key was already used for a different request")
                return xdr.SCVal.from_xdr(result_xdr)

            for attempt in range(self.max_attempts):
                try:
                    result = await operation()
                    await self.store.set(key, fingerprint, result.to_xdr())
                    return result
                except (
                    ConnectionError,
                    TimeoutError,
                    OSError,
                    StellarConnectionError,
                    TransactionStillPendingError,
                ):
                    if attempt + 1 == self.max_attempts:
                        raise
                    await asyncio.sleep(0.25 * (2**attempt))
        raise RuntimeError("Idempotent submission did not produce a result")


class SorobanContractAsyncClient:
    def __init__(
        self,
        contract_id: str,
        rpc_url: str,
        network_passphrase: str,
        signer: Keypair | None = None,
        idempotency_store: IdempotencyStore | None = None,
    ) -> None:
        self.signer = signer
        self.source = signer.public_key if signer else None
        self.contract = ContractClientAsync(contract_id, rpc_url, network_passphrase)
        self.server: SorobanServerAsync = self.contract.server
        self.idempotency = IdempotencyMiddleware(idempotency_store)

    async def call(
        self,
        function_name: str,
        *arguments: Any,
        result_model: type[T] | None = None,
        argument_types: Sequence[str] | None = None,
    ) -> T | Any:
        assembled = await self._invoke(function_name, arguments, argument_types)
        return _decode_result(assembled.result(), result_model)

    async def submit(
        self,
        function_name: str,
        *arguments: Any,
        idempotency_key: str,
        argument_types: Sequence[str] | None = None,
        result_model: type[T] | None = None,
    ) -> T | Any:
        if self.signer is None or self.source is None:
            raise RuntimeError("A signing Keypair is required for state-changing calls")
        encoded = _encode_arguments(function_name, arguments, argument_types)
        fingerprint = _fingerprint(function_name, encoded)
        assembled = await self.contract.invoke(
            function_name,
            encoded,
            source=self.source,
            signer=self.signer,
        )
        assembled.sign(force=True)
        result = await self.idempotency.run(idempotency_key, fingerprint, assembled.submit)
        return _decode_result(result, result_model)

    async def close(self) -> None:
        await self.server.close()

    async def _invoke(
        self,
        function_name: str,
        arguments: Sequence[Any],
        argument_types: Sequence[str] | None,
    ):
        encoded = _encode_arguments(function_name, arguments, argument_types)
        assembled = await self.contract.invoke(
            function_name,
            encoded,
            source=self.source or "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            signer=self.signer,
        )
        return assembled


def _encode_arguments(
    function_name: str,
    arguments: Sequence[Any],
    argument_types: Sequence[str] | None,
) -> list[xdr.SCVal]:
    names = _FUNCTION_ARGUMENT_NAMES.get(function_name, ())
    encoded = []
    for index, value in enumerate(arguments):
        hint = argument_types[index] if argument_types and index < len(argument_types) else None
        field_name = names[index] if index < len(names) else ""
        encoded.append(_to_scval(value, field_name, hint))
    return encoded


def _to_scval(value: Any, field_name: str = "", type_hint: str | None = None) -> xdr.SCVal:
    if isinstance(value, BaseModel):
        fields = {
            name: _to_scval(
                getattr(value, name),
                name,
                _MODEL_INTEGER_TYPES.get(type(value).__name__, {}).get(name),
            )
            for name in type(value).model_fields
        }
        return scval.to_struct(fields)
    if value is None:
        return scval.to_void()
    if isinstance(value, bool):
        return scval.to_bool(value)
    if isinstance(value, int):
        kind = type_hint
        if kind is None:
            if field_name in _UINT32_FIELDS or field_name.endswith("_bps"):
                kind = "u32"
            elif field_name in _UINT64_FIELDS or field_name.endswith(("_id", "_at", "_ts")):
                kind = "u64"
            else:
                kind = "i128"
        converters = {
            "i32": scval.to_int32,
            "u32": scval.to_uint32,
            "i64": scval.to_int64,
            "u64": scval.to_uint64,
            "i128": scval.to_int128,
            "u128": scval.to_uint128,
        }
        try:
            return converters[kind](value)
        except KeyError as exc:
            raise ValueError(f"Unsupported integer ScVal type: {kind}") from exc
    if isinstance(value, bytes):
        return scval.to_bytes(value)
    if isinstance(value, (list, tuple)):
        return scval.to_vec([_to_scval(item) for item in value])
    if isinstance(value, dict):
        return scval.to_struct({str(key): _to_scval(item, str(key)) for key, item in value.items()})
    if isinstance(value, str):
        if field_name in _ENUM_FIELDS:
            return scval.to_enum(value, None)
        try:
            return scval.to_address(value)
        except (TypeError, ValueError):
            return scval.to_string(value)
    raise TypeError(f"Cannot convert {type(value).__name__} to a Soroban ScVal")


def _decode_result(value: xdr.SCVal, model: type[T] | None) -> T | Any:
    native = _normalize_native(scval.to_native(value))
    if model is None:
        return native
    if not isinstance(native, dict):
        raise TypeError(f"Expected a Soroban struct for {model.__name__}")
    return model.model_validate(native)


def _normalize_native(value: Any) -> Any:
    if isinstance(value, dict):
        return {str(key): _normalize_native(item) for key, item in value.items()}
    if isinstance(value, list):
        if len(value) == 1 and isinstance(value[0], str):
            return value[0]
        return [_normalize_native(item) for item in value]
    if hasattr(value, "address"):
        return value.address
    return value


def _fingerprint(function_name: str, arguments: Sequence[xdr.SCVal]) -> str:
    digest = hashlib.sha256(function_name.encode("utf-8"))
    for argument in arguments:
        digest.update(argument.to_xdr_bytes())
    return digest.hexdigest()
