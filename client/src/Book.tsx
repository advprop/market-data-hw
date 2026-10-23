import { fixed, time, type Level, type Snapshot } from "./domain";

function Side({ rows, side }: { rows: Level[]; side: "bid" | "ask" }) {
  const maximum = rows.reduce(
    (max, row) => (BigInt(row.quantity) > max ? BigInt(row.quantity) : max),
    1n,
  );
  return (
    <table className={`book-table ${side}`}>
      <caption>{side === "bid" ? "buy / bids" : "sell / asks"}</caption>
      <thead>
        <tr>
          <th scope="col">price</th>
          <th scope="col">quantity</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr
            key={row.price}
            style={{
              backgroundSize: `${Number((BigInt(row.quantity) * 100n) / maximum)}% 100%`,
            }}
          >
            <td>{fixed(row.price)}</td>
            <td>{fixed(row.quantity)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default function Book({ snapshot }: { snapshot: Snapshot }) {
  return (
    <>
      <div className="book-sides">
        <Side rows={snapshot.bids} side="bid" />
        <Side rows={snapshot.asks} side="ask" />
      </div>
      <p className="footnote">
        {time(snapshot.timestampMs)} UTC{" "}
        <span>Sequence {snapshot.sequence}</span>
      </p>
    </>
  );
}
