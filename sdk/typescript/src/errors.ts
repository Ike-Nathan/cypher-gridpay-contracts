import { xdr } from "@stellar/stellar-sdk";

export type ContractDomain = "payment" | "escrow" | "refund" | "admin";

export type ContractErrorInfo = {
  code: number;
  name: string;
  message: string;
  resolution: string;
};

const errorCatalog: Record<ContractDomain, Record<number, Omit<ContractErrorInfo, "code">>> = {
  payment: {
    100: { name: "Unauthorized", message: "Admin authentication required", resolution: "Use a registered admin account." },
    101: { name: "MetadataTooLarge", message: "Payment metadata exceeds size limit", resolution: "Reduce the metadata size." },
    102: { name: "NotesTooLarge", message: "Payment notes exceed size limit", resolution: "Reduce the notes size." },
    103: { name: "InvalidCurrency", message: "Unsupported currency specified", resolution: "Use a supported currency." },
    104: { name: "InvalidBatchSize", message: "Batch size is invalid", resolution: "Use a batch size between 1 and 100." },
    105: { name: "BatchPartialFailure", message: "Some payments in the batch failed", resolution: "Check individual payment statuses." },
    106: { name: "RateLimitExceeded", message: "Rate limit exceeded for this address", resolution: "Wait for the rate limit window to reset." },
    107: { name: "DailyVolumeExceeded", message: "Daily volume limit exceeded", resolution: "Wait until the next daily window." },
    108: { name: "AddressFlagged", message: "Address has been flagged for suspicious activity", resolution: "Contact support to resolve the flag." },
    109: { name: "AddressAlreadyFlagged", message: "Address is already flagged", resolution: "No action is needed." },
    110: { name: "AmountExceedsLimit", message: "Payment amount exceeds the maximum allowed", resolution: "Reduce the payment amount." },
    111: { name: "MultiSigNotInitialized", message: "Multi-signature is not initialized", resolution: "Initialize multi-signature configuration first." },
    112: { name: "InsufficientAdmins", message: "Not enough admin signatures", resolution: "Collect the required admin signatures." },
    113: { name: "NotAnAdmin", message: "Caller is not an admin", resolution: "Use an admin account." },
    114: { name: "AlreadyApproved", message: "Admin has already approved this proposal", resolution: "No action is needed." },
    115: { name: "OracleCallFailed", message: "Price oracle call failed", resolution: "Retry or check the oracle configuration." },
    116: { name: "ContractPaused", message: "Contract is paused", resolution: "Retry after the contract is unpaused." },
    117: { name: "FunctionPaused", message: "This function is paused", resolution: "Use an alternative function or retry later." },
    118: { name: "InvalidTierThresholds", message: "Tier threshold configuration is invalid", resolution: "Correct the tier threshold values." },
    119: { name: "OracleFeedStale", message: "Oracle price feed is stale", resolution: "Wait for the next oracle update." },
    120: { name: "OracleNotConfigured", message: "Oracle is not configured", resolution: "Configure the oracle first." },
    121: { name: "InvalidAmount", message: "Payment amount is invalid", resolution: "Use a positive amount." },
    122: { name: "VerificationLevelNotFound", message: "Verification level not found", resolution: "Use a valid verification level." },
    123: { name: "TierLimitsNotConfigured", message: "Tier limits are not configured", resolution: "Configure tier limits first." },
    124: { name: "InvalidInterval", message: "Invalid interval specified", resolution: "Use a valid interval." },
    125: { name: "InvalidBps", message: "Invalid basis points value", resolution: "Use basis points between 0 and 10,000." },
    126: { name: "SchemaAlreadyAtTarget", message: "Schema is already at target version", resolution: "No migration is needed." },
  },
  escrow: {
    1: { name: "EscrowNotFound", message: "Escrow not found", resolution: "Verify the escrow ID." },
    2: { name: "EscrowAlreadyExists", message: "Escrow already exists", resolution: "Use a unique escrow ID." },
    3: { name: "EscrowExpired", message: "Escrow has expired", resolution: "Create a new escrow." },
    4: { name: "EscrowNotExpired", message: "Escrow has not expired yet", resolution: "Wait for expiration." },
    5: { name: "Unauthorized", message: "Not authorized to perform this action", resolution: "Use an authorized account." },
    6: { name: "InvalidAmount", message: "Invalid amount specified", resolution: "Use a positive amount." },
    7: { name: "InvalidRecipient", message: "Invalid recipient address", resolution: "Use a valid address." },
    8: { name: "EscrowNotActive", message: "Escrow is not active", resolution: "Check the escrow status." },
    9: { name: "EscrowAlreadyReleased", message: "Escrow has already been released", resolution: "No action is needed." },
    10: { name: "EscrowAlreadyRefunded", message: "Escrow has already been refunded", resolution: "No action is needed." },
    11: { name: "DisputeWindowClosed", message: "Dispute window has closed", resolution: "Contact support." },
    12: { name: "EvidenceSubmissionClosed", message: "Evidence submission is closed", resolution: "Contact support." },
    13: { name: "InvalidEvidenceHash", message: "Invalid evidence hash", resolution: "Provide a valid evidence hash." },
    14: { name: "ReleaseConditionsNotMet", message: "Release conditions are not met", resolution: "Fulfill the required release conditions." },
    15: { name: "TimelockNotExpired", message: "Timelock has not expired", resolution: "Wait for the timelock to expire." },
    16: { name: "ClawbackInProgress", message: "A clawback is already in progress", resolution: "Wait for clawback completion." },
    17: { name: "ClawbackNotAuthorized", message: "Clawback is not authorized", resolution: "Obtain admin authorization." },
    18: { name: "BatchSizeExceeded", message: "Batch size exceeds the limit", resolution: "Reduce the batch size." },
    19: { name: "MultiPartyThresholdNotMet", message: "Multi-party threshold is not met", resolution: "Collect more signatures." },
    20: { name: "VestingNotStarted", message: "Vesting has not started", resolution: "Wait for vesting to start." },
  },
  refund: {
    1: { name: "RefundNotFound", message: "Refund not found", resolution: "Verify the refund ID." },
    2: { name: "RefundAlreadyProcessed", message: "Refund has already been processed", resolution: "No action is needed." },
    3: { name: "RefundNotEligible", message: "Refund is not eligible", resolution: "Check the eligibility criteria." },
    4: { name: "RefundWindowExpired", message: "Refund window has expired", resolution: "Contact support." },
    5: { name: "Unauthorized", message: "Not authorized", resolution: "Use an authorized account." },
    6: { name: "InvalidAmount", message: "Invalid refund amount", resolution: "Use a valid amount." },
    7: { name: "MerchantNotEligible", message: "Merchant is not eligible for refunds", resolution: "Contact support." },
    8: { name: "RefundAlreadyExists", message: "Refund already exists for this payment", resolution: "No action is needed." },
    9: { name: "ArbitrationInProgress", message: "Arbitration is already in progress", resolution: "Wait for the arbitration result." },
    10: { name: "ArbitrationTimeout", message: "Arbitration has timed out", resolution: "Contact support." },
    11: { name: "InsufficientStake", message: "Insufficient arbitration stake", resolution: "Increase the stake amount." },
    12: { name: "PolicyNotConfigured", message: "Refund policy is not configured", resolution: "Configure the policy first." },
    13: { name: "CircuitBreakerTripped", message: "Circuit breaker has been tripped", resolution: "Wait for the circuit breaker to reset." },
    14: { name: "RateLimitExceeded", message: "Rate limit exceeded", resolution: "Wait for the rate limit to reset." },
    15: { name: "CooldownActive", message: "Refund cooldown is active", resolution: "Wait for the cooldown period." },
  },
  admin: {
    1: { name: "AlreadyInitialized", message: "Contract is already initialized", resolution: "No action is needed." },
    2: { name: "NotInitialized", message: "Contract is not initialized", resolution: "Initialize the contract first." },
    3: { name: "Unauthorized", message: "Admin authentication required", resolution: "Use an admin account." },
  },
};

export class SorobanContractError extends Error {
  readonly code: number;
  readonly domain?: ContractDomain;
  readonly errorName: string;
  readonly resolution?: string;

  constructor(code: number, domain?: ContractDomain) {
    const info = domain === undefined ? undefined : errorCatalog[domain][code];
    super(info?.message ?? `Contract call failed with error code ${code}`);
    this.name = "SorobanContractError";
    this.code = code;
    this.domain = domain;
    this.errorName = info?.name ?? "UnknownContractError";
    this.resolution = info?.resolution;
  }
}

function findCode(value: unknown, seen = new Set<object>()): number | undefined {
  if (typeof value === "string") {
    const match = value.match(/Error\s*\(\s*Contract\s*,\s*#?\s*(\d+)\s*\)/i)
      ?? value.match(/contract\s+error\s*(?:code)?\s*[:#]?\s*(\d+)/i);
    return match === null ? undefined : Number(match[1]);
  }
  if (value === null || typeof value !== "object" || seen.has(value)) return undefined;
  seen.add(value);

  const xdrValue = value as Record<string, unknown> & {
    switch?: () => { name?: string };
    error?: () => unknown;
    contractCode?: () => unknown;
    code?: () => unknown;
  };
  const unionName = xdrValue.switch?.().name;
  if (unionName === "scvError" && xdrValue.error !== undefined) {
    const code = findCode(xdrValue.error(), seen);
    if (code !== undefined) return code;
  }
  if (unionName === "sceContract") {
    const contractCode = xdrValue.contractCode?.() ?? xdrValue.code?.();
    const code = typeof contractCode === "number"
      ? contractCode
      : typeof contractCode === "object" && contractCode !== null && "value" in contractCode
        ? Number((contractCode as { value: unknown }).value)
        : undefined;
    if (code !== undefined && Number.isInteger(code) && code >= 0) return code;
  }

  const record = value as Record<string, unknown>;
  const directCode = record.code ?? record.errorCode;
  if (typeof directCode === "number" && Number.isInteger(directCode) && directCode >= 0) {
    const type = String(record.type ?? record.errorType ?? "").toLowerCase();
    if (type === "contract" || record.contractError === true || record.errorCode !== undefined) return directCode;
  }

  for (const key of ["message", "error", "cause", "result", "resultXdr", "errorResultXdr"]) {
    if ((key === "resultXdr" || key === "errorResultXdr") && typeof record[key] === "string") {
      try {
        const result = xdr.TransactionResult.fromXDR(record[key] as string, "base64");
        const found = findCode(result, seen);
        if (found !== undefined) return found;
      } catch {
        // The field may contain a different XDR type or a non-XDR value.
      }
    }
    const found = findCode(record[key], seen);
    if (found !== undefined) return found;
  }

  for (const method of ["result", "results", "tr", "invokeHostFunctionResult", "success", "events", "event", "body", "v0", "returnValue", "innerResultPair"]) {
    const accessor = (value as Record<string, unknown>)[method];
    if (typeof accessor === "function") {
      const found = findCode((accessor as () => unknown).call(value), seen);
      if (found !== undefined) return found;
    }
  }
  return undefined;
}

export function decodeSorobanContractError(
  error: unknown,
  domain?: ContractDomain,
): SorobanContractError | undefined {
  const code = findCode(error);
  return code === undefined ? undefined : new SorobanContractError(code, domain);
}

export function getContractErrorInfo(code: number, domain: ContractDomain): ContractErrorInfo {
  const info = errorCatalog[domain][code];
  return info === undefined
    ? { code, name: "UnknownContractError", message: `Contract call failed with error code ${code}`, resolution: "Check the contract error reference for this contract." }
    : { code, ...info };
}