import { scValToNative, xdr } from "@stellar/stellar-sdk";

export type SorobanEventRecord = {
  id: string;
  pagingToken?: string;
  type: string;
  ledger: number;
  ledgerClosedAt: string;
  contractId?: string | { toString(): string };
  topic: xdr.ScVal[];
  value: xdr.ScVal;
};

export type ParsedContractEvent<T = Record<string, unknown>> = {
  id: string;
  contractId?: string;
  ledger: number;
  ledgerClosedAt: string;
  name: string;
  topics: unknown[];
  data: T;
};

export function parseContractEvent<T = Record<string, unknown>>(
  event: SorobanEventRecord,
): ParsedContractEvent<T> {
  const decodedTopics = event.topic.map((topic) => scValToNative(topic));
  const eventName = decodedTopics[0];
  if (typeof eventName !== "string") {
    throw new TypeError("Soroban contract event is missing its event-name topic");
  }

  const decodedValue: unknown = scValToNative(event.value);
  const data = decodedValue instanceof Map
    ? Object.fromEntries(decodedValue.entries())
    : decodedValue;

  if (data === null || typeof data !== "object" || Array.isArray(data)) {
    throw new TypeError(`Soroban event ${eventName} has a non-struct value`);
  }

  return {
    id: event.id,
    contractId: event.contractId?.toString(),
    ledger: event.ledger,
    ledgerClosedAt: event.ledgerClosedAt,
    name: eventName,
    topics: decodedTopics.slice(1),
    data: data as T,
  };
}

export type ContractEventFilter = {
  type?: "contract" | "system" | "diagnostic";
  contractIds?: string[];
  topics?: string[][];
};

export type GetEventsRequest = {
  startLedger?: number;
  endLedger?: number;
  cursor?: string;
  limit?: number;
  filters: ContractEventFilter[];
};

export type GetEventsResponse = {
  events: SorobanEventRecord[];
  cursor: string;
  latestLedger: number;
};

export interface EventRpc {
  getEvents(request: GetEventsRequest): Promise<GetEventsResponse>;
}

export interface EventCheckpointStore {
  get(): Promise<string | undefined>;
  set(cursor: string): Promise<void>;
}

export async function pollContractEvents<T = Record<string, unknown>>(options: {
  rpc: EventRpc;
  checkpoints: EventCheckpointStore;
  filters: ContractEventFilter[];
  startLedger?: number;
  limit?: number;
  onEvent(event: ParsedContractEvent<T>): Promise<void> | void;
}): Promise<{ cursor?: string; latestLedger: number; eventsProcessed: number }> {
  const cursor = await options.checkpoints.get();
  const response = await options.rpc.getEvents({
    startLedger: options.startLedger,
    filters: options.filters,
    cursor,
    limit: options.limit,
  });

  for (const event of response.events) {
    await options.onEvent(parseContractEvent<T>(event));
  }

  const nextCursor = response.cursor || response.events.at(-1)?.pagingToken || cursor;
  if (nextCursor !== undefined && nextCursor !== cursor) {
    await options.checkpoints.set(nextCursor);
  }

  return {
    cursor: nextCursor,
    latestLedger: response.latestLedger,
    eventsProcessed: response.events.length,
  };
}