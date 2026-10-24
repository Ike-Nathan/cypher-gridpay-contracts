from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field


class ContractModel(BaseModel):
    model_config = ConfigDict(extra="ignore", populate_by_name=True)


class Payment(ContractModel):
    id: int
    customer: str
    merchant: str
    amount: int
    token: str
    currency: str
    status: str
    created_at: int
    expires_at: int
    metadata: str
    notes: str
    refunded_amount: int


class Escrow(ContractModel):
    id: int
    customer: str
    merchant: str
    amount: int
    token: str
    status: str
    created_at: int
    release_timestamp: int
    dispute_started_at: int
    last_activity_at: int
    escalation_level: int
    hold_period_until: int
    fee_bps: int
    expiry_timestamp: int
    auto_refund_on_expiry: bool
    escalated_at: int | None
    escalation_timeout: int
    auto_resolve_in_favor_of: str
    evidence_deadline: int | None


class Refund(ContractModel):
    id: int
    payment_id: int
    merchant: str
    customer: str
    amount: int
    original_payment_amount: int
    token: str
    original_token: str
    status: str
    requested_at: int
    reason: str
    reason_code: str
    approved_at: int | None
    rejected_at: int | None
    processed_at: int | None
    rejected_by: str | None
    appeal_deadline: int | None
    expires_at: int | None


class SystemStatus(ContractModel):
    globally_paused: bool = False
    paused_functions: list[str] = Field(default_factory=list)
    paused_at: int = 0
    paused_by: str | None = None
    pause_reason: str = ""
    circuit_breaker_tripped: bool = False
    circuit_breaker_tripped_at: int | None = None
    circuit_breaker_resets_at: int | None = None
    circuit_breaker_trip_count: int = 0

    @classmethod
    def from_contract_states(
        cls,
        pause_state: dict[str, object],
        circuit_breaker_state: dict[str, object] | None = None,
    ) -> "SystemStatus":
        breaker = circuit_breaker_state or {}
        return cls(
            **pause_state,
            circuit_breaker_tripped=bool(breaker.get("tripped", False)),
            circuit_breaker_tripped_at=breaker.get("tripped_at"),
            circuit_breaker_resets_at=breaker.get("resets_at"),
            circuit_breaker_trip_count=int(breaker.get("trip_count", 0)),
        )
