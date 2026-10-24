# Error Codes Reference

This document catalogs all contract error variants across the Cypher GridPay protocol contracts, providing a standard mapping of error integers to user-friendly messages.

## Payment Contract

### BasicError

| Code | Name | Message | Resolution |
|------|------|---------|------------|
| 100 | Unauthorized | Admin authentication required | Ensure the caller is a registered admin |
| 101 | MetadataTooLarge | Payment metadata exceeds size limit | Reduce metadata size |
| 102 | NotesTooLarge | Payment notes exceed size limit | Reduce notes size |
| 103 | InvalidCurrency | Unsupported currency specified | Use a supported currency (XLM, USDC, USDT, BTC, ETH) |
| 104 | InvalidBatchSize | Batch size is invalid | Use a batch size between 1 and 100 |
| 105 | BatchPartialFailure | Some payments in batch failed | Check individual payment statuses |
| 106 | RateLimitExceeded | Rate limit exceeded for this address | Wait for the rate limit window to reset |
| 107 | DailyVolumeExceeded | Daily volume limit exceeded | Wait for the next day or contact admin |
| 108 | AddressFlagged | Address has been flagged for suspicious activity | Contact support to resolve flag |
| 109 | AddressAlreadyFlagged | Address is already flagged | No action needed |
| 110 | AmountExceedsLimit | Payment amount exceeds maximum allowed | Reduce payment amount |
| 111 | MultiSigNotInitialized | Multi-signature not initialized | Initialize multi-sig configuration first |
| 112 | InsufficientAdmins | Not enough admin signatures required | Collect required number of admin signatures |
| 113 | NotAnAdmin | Caller is not an admin | Use an admin account |
| 114 | AlreadyApproved | Admin has already approved this proposal | No action needed |
| 115 | OracleCallFailed | Price oracle call failed | Retry or check oracle configuration |
| 116 | ContractPaused | Contract is paused | Wait for contract to be unpaused |
| 117 | FunctionPaused | This function is paused | Use an alternative function |
| 118 | InvalidTierThresholds | Tier threshold configuration is invalid | Fix tier threshold values |
| 119 | OracleFeedStale | Oracle price feed is stale | Wait for next oracle update |
| 120 | OracleNotConfigured | Oracle is not configured | Configure oracle first |
| 121 | InvalidAmount | Payment amount is invalid | Use a positive amount |
| 122 | VerificationLevelNotFound | Verification level not found | Use a valid verification level |
| 123 | TierLimitsNotConfigured | Tier limits are not configured | Configure tier limits first |
| 124 | InvalidInterval | Invalid interval specified | Use a valid interval |
| 125 | InvalidBps | Invalid basis points value | Use bps between 0 and 10000 |
| 126 | SchemaAlreadyAtTarget | Schema is already at target version | No migration needed |

## Escrow Contract

### EscrowError

| Code | Name | Message | Resolution |
|------|------|---------|------------|
| 1 | EscrowNotFound | Escrow not found | Verify the escrow ID |
| 2 | EscrowAlreadyExists | Escrow already exists | Use a unique escrow ID |
| 3 | EscrowExpired | Escrow has expired | Create a new escrow |
| 4 | EscrowNotExpired | Escrow has not expired yet | Wait for expiration |
| 5 | Unauthorized | Not authorized to perform this action | Use an authorized account |
| 6 | InvalidAmount | Invalid amount specified | Use a positive amount |
| 7 | InvalidRecipient | Invalid recipient address | Use a valid address |
| 8 | EscrowNotActive | Escrow is not in active status | Check escrow status |
| 9 | EscrowAlreadyReleased | Escrow has already been released | No action needed |
| 10 | EscrowAlreadyRefunded | Escrow has already been refunded | No action needed |
| 11 | DisputeWindowClosed | Dispute window has closed | Contact support |
| 12 | EvidenceSubmissionClosed | Evidence submission is closed | Contact support |
| 13 | InvalidEvidenceHash | Invalid evidence hash | Provide a valid evidence hash |
| 14 | ReleaseConditionsNotMet | Release conditions not met | Fulfill release conditions |
| 15 | TimelockNotExpired | Timelock has not expired | Wait for timelock to expire |
| 16 | ClawbackInProgress | A clawback is already in progress | Wait for clawback to complete |
| 17 | ClawbackNotAuthorized | Clawback not authorized | Get admin authorization |
| 18 | BatchSizeExceeded | Batch size exceeds limit | Reduce batch size |
| 19 | MultiPartyThresholdNotMet | Multi-party threshold not met | Collect more signatures |
| 20 | VestingNotStarted | Vesting has not started | Wait for vesting start time |

## Refund Contract

### RefundError

| Code | Name | Message | Resolution |
|------|------|---------|------------|
| 1 | RefundNotFound | Refund not found | Verify the refund ID |
| 2 | RefundAlreadyProcessed | Refund already processed | No action needed |
| 3 | RefundNotEligible | Refund not eligible | Check eligibility criteria |
| 4 | RefundWindowExpired | Refund window has expired | Contact support |
| 5 | Unauthorized | Not authorized | Use an authorized account |
| 6 | InvalidAmount | Invalid refund amount | Use a valid amount |
| 7 | MerchantNotEligible | Merchant not eligible for refunds | Contact support |
| 8 | RefundAlreadyExists | Refund already exists for this payment | No action needed |
| 9 | ArbitrationInProgress | Arbitration is already in progress | Wait for arbitration result |
| 10 | ArbitrationTimeout | Arbitration has timed out | Contact support |
| 11 | InsufficientStake | Insufficient arbitration stake | Increase stake amount |
| 12 | PolicyNotConfigured | Refund policy not configured | Configure policy first |
| 13 | CircuitBreakerTripped | Circuit breaker has been tripped | Wait for circuit breaker to reset |
| 14 | RateLimitExceeded | Rate limit exceeded | Wait for rate limit reset |
| 15 | CooldownActive | Refund cooldown is active | Wait for cooldown period |

## Admin Contract

### AdminError

| Code | Name | Message | Resolution |
|------|------|---------|------------|
| 1 | AlreadyInitialized | Contract already initialized | No action needed |
| 2 | NotInitialized | Contract not initialized | Initialize contract first |
| 3 | Unauthorized | Admin authentication required | Use an admin account |

## Client Integration

When building frontend or backend integrations, handle errors by checking the error code and displaying the corresponding message to users. All error codes are deterministic and can be safely used for programmatic handling.

### TypeScript Example

```typescript
function handleContractError(error: any): string {
  const code = error?.code ?? error?.errorCode;
  const errorMap: Record<number, string> {
    106: 'Too many requests. Please wait a moment and try again.',
    107: 'Daily limit exceeded. Please try again tomorrow.',
    108: 'This address has been flagged. Contact support.',
    // ... add more mappings
  };
  return errorMap[code] ?? 'An unexpected error occurred. Please try again.';
}
```

### Python Example

```python
ERROR_MESSAGES = {
    106: "Too many requests. Please wait a moment and try again.",
    107: "Daily limit exceeded. Please try again tomorrow.",
    108: "This address has been flagged. Contact support.",
    # ... add more mappings
}

def handle_contract_error(error_code: int) -> str:
    return ERROR_MESSAGES.get(error_code, "An unexpected error occurred. Please try again.")
```
