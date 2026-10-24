import { FeeBumpTransaction, Transaction, TransactionBuilder } from "@stellar/stellar-sdk";

export type TransactionStatus = "SUCCESS" | "FAILED" | "NOT_FOUND" | "PENDING" | string;

export type SimulationResult = {
  restorePreamble?: unknown;
  error?: string;
};

export type SubmissionResult = {
  status: string;
  hash?: string;
  errorResult?: unknown;
};

export type TransactionResult<T> = {
  status: TransactionStatus;
  latestLedger?: number;
  result?: T;
};

export interface TransactionRpc<Tx, Sim extends SimulationResult, Send extends SubmissionResult, Result> {
  simulateTransaction(transaction: Tx): Promise<Sim>;
  sendTransaction(transaction: Tx): Promise<Send>;
  getTransaction(hash: string): Promise<TransactionResult<Result>>;
}

export type TransactionExecutionOptions<Tx, Sim extends SimulationResult, Send extends SubmissionResult, Result> = {
  rpc: TransactionRpc<Tx, Sim, Send, Result>;
  transaction: Tx;
  sign(transaction: Tx): Promise<Tx> | Tx;
  assemble(transaction: Tx, simulation: Sim): Tx;
  restore?(preamble: NonNullable<Sim["restorePreamble"]>): Promise<void>;
  feeBump?(transaction: Tx, sendResult: Send): Promise<Tx> | Tx;
  shouldFeeBump?(sendResult: Send): boolean;
  maxFeeBumpAttempts?: number;
  maxPollAttempts?: number;
  initialPollDelayMs?: number;
  maxPollDelayMs?: number;
  sleep?: (milliseconds: number) => Promise<void>;
};

export class SorobanTransactionError extends Error {
  constructor(message: string, readonly details?: unknown) {
    super(message);
    this.name = "SorobanTransactionError";
  }
}

export function buildFeeBumpTransaction(
  transaction: Transaction,
  feeSource: string,
  baseFee: string,
  networkPassphrase: string,
): FeeBumpTransaction {
  return TransactionBuilder.buildFeeBumpTransaction(
    feeSource,
    baseFee,
    transaction,
    networkPassphrase,
  );
}

export async function executeSorobanTransaction<
  Tx,
  Sim extends SimulationResult,
  Send extends SubmissionResult,
  Result,
>(options: TransactionExecutionOptions<Tx, Sim, Send, Result>): Promise<TransactionResult<Result>> {
  const sleep = options.sleep ?? ((milliseconds: number) => new Promise<void>((resolve, reject) => {
    const timer = (globalThis as typeof globalThis & {
      setTimeout?: (callback: () => void, delay: number) => unknown;
    }).setTimeout;
    if (timer === undefined) {
      reject(new Error("A sleep implementation is required in this runtime"));
      return;
    }
    timer(resolve, milliseconds);
  }));
  const maxPollAttempts = options.maxPollAttempts ?? 8;
  const maxFeeBumpAttempts = options.maxFeeBumpAttempts ?? 1;
  let transaction = options.transaction;

  let simulation = await options.rpc.simulateTransaction(transaction);
  if (simulation.error !== undefined) {
    throw new SorobanTransactionError(`Soroban simulation failed: ${simulation.error}`, simulation);
  }
  const restorePreamble = simulation.restorePreamble;
  if (restorePreamble !== undefined && restorePreamble !== null) {
    if (options.restore === undefined) {
      throw new SorobanTransactionError("Archived ledger entries require a RestoreFootprintOp callback", restorePreamble);
    }
    await options.restore(restorePreamble as NonNullable<Sim["restorePreamble"]>);
    simulation = await options.rpc.simulateTransaction(transaction);
    if (simulation.error !== undefined || simulation.restorePreamble !== undefined) {
      throw new SorobanTransactionError("Soroban simulation still requires restoration or failed after restore", simulation);
    }
  }

  transaction = await options.sign(options.assemble(transaction, simulation));
  let sendResult: Send | undefined;
  for (let attempt = 0; attempt <= maxFeeBumpAttempts; attempt += 1) {
    sendResult = await options.rpc.sendTransaction(transaction);
    if (sendResult.status === "PENDING" && sendResult.hash !== undefined) break;
    if (
      attempt === maxFeeBumpAttempts ||
      options.feeBump === undefined ||
      options.shouldFeeBump?.(sendResult) !== true
    ) {
      throw new SorobanTransactionError(`Soroban transaction submission failed: ${sendResult.status}`, sendResult);
    }
    transaction = await options.sign(await options.feeBump(transaction, sendResult));
  }

  if (sendResult?.hash === undefined) {
    throw new SorobanTransactionError("Soroban RPC accepted no transaction hash", sendResult);
  }

  let delay = options.initialPollDelayMs ?? 500;
  const maxDelay = options.maxPollDelayMs ?? 8_000;
  for (let attempt = 0; attempt < maxPollAttempts; attempt += 1) {
    const result = await options.rpc.getTransaction(sendResult.hash);
    if (result.status === "SUCCESS") return result;
    if (result.status === "FAILED") {
      throw new SorobanTransactionError("Soroban transaction failed on-ledger", result);
    }
    await sleep(delay);
    delay = Math.min(delay * 2, maxDelay);
  }

  throw new SorobanTransactionError(`Timed out waiting for transaction ${sendResult.hash}`, { hash: sendResult.hash });
}