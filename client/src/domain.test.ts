import { describe, expect, it } from "vitest";
import { fixed, parseBatch, period } from "./domain";

const snapshot = {
  symbol: "BTC-TEST",
  timestampMs: "1791540000000",
  sequence: "18446744073709551615",
  bids: [{ price: "100000001", quantity: "2000000" }],
  asks: [{ price: "101000001", quantity: "3000000" }],
};
const parse = (value: unknown) => parseBatch(JSON.stringify(value));

describe("batch validation and precision", () => {
  it("preserves uint64 and decimal precision", () => {
    expect(parse({ snapshots: [snapshot] }).snapshots[0].sequence).toBe(
      "18446744073709551615",
    );
    expect(fixed("100000000000000001")).toBe(
      `100${String.fromCharCode(160)}000${String.fromCharCode(160)}000${String.fromCharCode(160)}000,000001`,
    );
  });
  it.each([
    { ...snapshot, sequence: "18446744073709551616" },
    { ...snapshot, sequence: Number("9007199254740993") },
    { ...snapshot, symbol: "btc-test" },
    { ...snapshot, timestampMs: "0" },
    { ...snapshot, asks: [] },
    { ...snapshot, bids: [snapshot.bids[0], snapshot.bids[0]] },
    { ...snapshot, asks: snapshot.bids },
    { ...snapshot, bids: [{ price: "0", quantity: "1000000" }] },
  ])("rejects invalid snapshot %#", (row) =>
    expect(() => parse({ snapshots: [row] })).toThrow(),
  );
  it("rejects duplicate identities and oversized batches", () => {
    expect(() =>
      parse({
        snapshots: [
          snapshot,
          { ...snapshot, sequence: `0${snapshot.sequence}` },
        ],
      }),
    ).toThrow("duplicate snapshot identity");
    expect(() =>
      parse({ snapshots: Array.from({ length: 101 }, () => snapshot) }),
    ).toThrow("1 to 100");
    expect(() => parseBatch(" ".repeat(1_048_577))).toThrow("1 mib");
  });
});

describe("utc range", () => {
  it("preserves milliseconds and exclusive upper bound", () => {
    expect(
      period("2026-10-09T10:00:00.000", "2026-10-09T10:00:00.001", 10),
    ).toBe("from_ms=1791540000000&to_ms=1791540000001&limit=10");
  });
  it("rejects empty or reversed ranges and invalid limits", () => {
    expect(() => period("", "", 100)).toThrow();
    expect(() => period("2026-10-09T10:00", "2026-10-09T09:00", 100)).toThrow();
    expect(() => period("2026-10-09T10:00", "2026-10-09T11:00", 0)).toThrow();
    expect(() => period("2026-10-09T10:00", "2026-10-09T11:00", 1.5)).toThrow();
  });
});
