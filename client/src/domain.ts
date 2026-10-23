export type Level = { price: string; quantity: string };
export type Snapshot = {
  symbol: string;
  timestampMs: string;
  sequence: string;
  bids: Level[];
  asks: Level[];
};
export type Summary = {
  timestampMs: string;
  bestBid: string;
  bestAsk: string;
  spread: string;
  midPrice: number;
  bidQuantity: string;
  askQuantity: string;
  imbalance: number;
};
export type Coverage = { firstMs: string; lastMs: string; count: string };
export type Point = { timestampMs: string; sequence: string; price: number };
export type History = { snapshots: Snapshot[]; truncated: boolean };
export type Series = { points: Point[]; truncated: boolean };
export type Batch = { snapshots: Snapshot[] };

export const formatNumber = (value: number) =>
  new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 6 }).format(value);
export const time = (value: string) =>
  new Date(Number(value)).toLocaleString("ru-RU", {
    timeZone: "UTC",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    fractionalSecondDigits: 3,
  });

export function fixed(value: string): string {
  const n = BigInt(value);
  const whole = (n / 1_000_000n).toLocaleString("ru-RU");
  const fraction = (n % 1_000_000n)
    .toString()
    .padStart(6, "0")
    .replace(/0+$/, "");
  return fraction ? `${whole},${fraction}` : whole;
}

export function dateInput(value: string): string {
  return new Date(Number(value)).toISOString().slice(0, 23);
}

export function period(from: string, to: string, limit: number): string {
  const start = Date.parse(`${from}Z`);
  const end = Date.parse(`${to}Z`);
  if (
    !Number.isFinite(start) ||
    !Number.isFinite(end) ||
    start < 0 ||
    start >= end ||
    end > 253_402_300_799_999
  ) {
    throw new Error(
      "invalid range: the start must be before the end. all times are utc.",
    );
  }
  if (!Number.isInteger(limit) || limit < 1 || limit > 100)
    throw new Error("limit must be an integer from 1 to 100.");
  return new URLSearchParams({
    from_ms: String(start),
    to_ms: String(end),
    limit: String(limit),
  }).toString();
}

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("expected a json object.");
  return value as Record<string, unknown>;
}

function integer(
  value: unknown,
  max: bigint,
  name: string,
  positive = true,
): string {
  if (typeof value !== "string" || !/^\d+$/.test(value))
    throw new Error(`${name}: provide an integer as a quoted string.`);
  const n = BigInt(value);
  if (n < (positive ? 1n : 0n) || n > max)
    throw new Error(`${name}: number is outside the allowed range.`);
  return value;
}

function levels(value: unknown, name: string): Level[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 100)
    throw new Error(`${name}: expected 1 to 100 levels.`);
  const seen = new Set<string>();
  return value.map((item) => {
    const row = object(item);
    const price = integer(row.price, 10n ** 15n, `${name}, price`);
    const quantity = integer(row.quantity, 10n ** 15n, `${name}, quantity`);
    const key = BigInt(price).toString();
    if (seen.has(key)) throw new Error(`${name}: duplicate price.`);
    seen.add(key);
    return { price, quantity };
  });
}

export function parseBatch(text: string): Batch {
  if (new TextEncoder().encode(text).length > 1_048_576)
    throw new Error("json size must not exceed 1 mib.");
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("invalid json. check quotes and commas.");
  }
  const input = object(value).snapshots;
  if (!Array.isArray(input) || input.length < 1 || input.length > 100)
    throw new Error("snapshots must contain 1 to 100 entries.");
  const identities = new Set<string>();
  const snapshots = input.map((item) => {
    const row = object(item);
    if (typeof row.symbol !== "string" || !/^[A-Z0-9-]{1,32}$/.test(row.symbol))
      throw new Error(
        "symbol must contain 1 to 32 uppercase ascii letters, digits or hyphens.",
      );
    const timestampMs = integer(
      row.timestampMs,
      253_402_300_799_999n,
      "timestamp",
    );
    const sequence = integer(
      row.sequence,
      18_446_744_073_709_551_615n,
      "Sequence",
      false,
    );
    const bids = levels(row.bids, "Bids");
    const asks = levels(row.asks, "Asks");
    const bestBid = bids.reduce((a, b) =>
      BigInt(a.price) > BigInt(b.price) ? a : b,
    );
    const bestAsk = asks.reduce((a, b) =>
      BigInt(a.price) < BigInt(b.price) ? a : b,
    );
    if (BigInt(bestBid.price) >= BigInt(bestAsk.price))
      throw new Error("best bid must be lower than best ask.");
    const key = `${row.symbol}/${BigInt(timestampMs)}/${BigInt(sequence)}`;
    if (identities.has(key))
      throw new Error(
        "duplicate snapshot identity: symbol, timestamp and sequence.",
      );
    identities.add(key);
    return { symbol: row.symbol, timestampMs, sequence, bids, asks };
  });
  return { snapshots };
}
