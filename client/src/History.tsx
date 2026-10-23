import { useState, type FormEvent } from "react";
import {
  dateInput,
  fixed,
  formatNumber,
  period,
  time,
  type Coverage,
  type History as HistoryData,
  type Point,
  type Series,
} from "./domain";
import { message } from "./api";
import { useRemote } from "./useRemote";
import Book from "./Book";

function Chart({ points }: { points: Point[] }) {
  const prices = points.map((point) => point.price);
  const low = Math.min(...prices);
  const high = Math.max(...prices);
  const first = Number(points[0].timestampMs);
  const last = Number(points[points.length - 1].timestampMs);
  const x = (point: Point) =>
    last === first
      ? 440
      : 60 + ((Number(point.timestampMs) - first) / (last - first)) * 760;
  const y = (point: Point) =>
    high === low ? 140 : 230 - ((point.price - low) / (high - low)) * 180;
  const coordinates = points
    .map((point) => `${x(point)},${y(point)}`)
    .join(" ");
  return (
    <div className="chart">
      <div className="chart-legend">
        <span className="legend-line" /> Mid price{" "}
        <strong>{formatNumber(points[points.length - 1].price)}</strong>
        <span className="muted">{points.length} points</span>
      </div>
      <svg
        viewBox="0 0 880 285"
        role="img"
        aria-label={`mid price history. minimum ${formatNumber(low)}, maximum ${formatNumber(high)}. ${points.length} points.`}
      >
        <defs>
          <linearGradient id="area" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#a59bb8" stopOpacity="0.16" />
            <stop offset="100%" stopColor="#a59bb8" stopOpacity="0" />
          </linearGradient>
        </defs>
        {[50, 140, 230].map((line, i) => (
          <g key={line}>
            <line
              x1="60"
              x2="820"
              y1={line}
              y2={line}
              stroke="#e6e2eb"
              strokeDasharray="3 5"
            />
            <text x="50" y={line + 4} textAnchor="end">
              {formatNumber(high - ((high - low) * i) / 2)}
            </text>
          </g>
        ))}
        <polygon points={`60,240 ${coordinates} 820,240`} fill="url(#area)" />
        <polyline
          points={coordinates}
          fill="none"
          stroke="#a59bb8"
          strokeWidth="2.5"
          strokeLinejoin="round"
        />
        {points.map((point) => (
          <circle
            key={`${point.timestampMs}-${point.sequence}`}
            cx={x(point)}
            cy={y(point)}
            r="4"
            fill="#a59bb8"
          >
            <title>
              {time(point.timestampMs)} UTC · {formatNumber(point.price)} ·
              sequence {point.sequence}
            </title>
          </circle>
        ))}
        <text x="60" y="270">
          {time(points[0].timestampMs)}
        </text>
        <text x="820" y="270" textAnchor="end">
          {time(points[points.length - 1].timestampMs)} UTC
        </text>
      </svg>
    </div>
  );
}

function Results({
  symbol,
  query,
  revision,
}: {
  symbol: string;
  query: string;
  revision: number;
}) {
  const history = useRemote<HistoryData>(
    `/v1/history/${symbol}?${query}`,
    revision,
  );
  const series = useRemote<Series>(
    `/v1/mid-prices/${symbol}?${query}`,
    revision,
  );
  const [selected, setSelected] = useState("");
  const error = history.error || series.error;
  if (error)
    return (
      <p className="notice error" role="alert">
        {error}
      </p>
    );
  if (!history.data || !series.data)
    return (
      <p className="placeholder" role="status">
        loading history…
      </p>
    );
  if (history.data.snapshots.length === 0)
    return (
      <p className="placeholder">
        no snapshots in this range. try different dates.
      </p>
    );
  const current = history.data.snapshots.find(
    (row) => `${row.timestampMs}/${row.sequence}` === selected,
  );
  const truncated = history.data.truncated || series.data.truncated;
  return (
    <>
      {truncated && (
        <p className="notice" role="status">
          showing the first {history.data.snapshots.length} snapshots. results
          truncated: narrow the range or increase the limit.
        </p>
      )}
      <Chart points={series.data.points} />
      {!truncated && (
        <a
          className="text-link"
          href={`/v1/plot/${symbol}?${query}`}
          target="_blank"
          rel="noreferrer"
        >
          open server chart as svg ↗
        </a>
      )}
      <div className="history-table-wrap">
        <table className="history-table">
          <caption>snapshots in range</caption>
          <thead>
            <tr>
              <th>time, utc</th>
              <th>Sequence</th>
              <th>best bid</th>
              <th>best ask</th>
              <th>order book</th>
            </tr>
          </thead>
          <tbody>
            {history.data.snapshots.map((row) => (
              <tr key={`${row.timestampMs}/${row.sequence}`}>
                <td>{time(row.timestampMs)}</td>
                <td>{row.sequence}</td>
                <td className="positive">{fixed(row.bids[0].price)}</td>
                <td className="negative">{fixed(row.asks[0].price)}</td>
                <td>
                  <button
                    className="small-button"
                    aria-pressed={current === row}
                    onClick={() =>
                      setSelected(`${row.timestampMs}/${row.sequence}`)
                    }
                  >
                    open
                    <span className="sr-only"> snapshot {row.sequence}</span>
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {current && (
        <section className="historical-book">
          <div className="section-heading">
            <h3>snapshot #{current.sequence}</h3>
            <button className="small-button" onClick={() => setSelected("")}>
              close snapshot
            </button>
          </div>
          <Book snapshot={current} />
        </section>
      )}
    </>
  );
}

export default function History({
  symbol,
  coverage,
  revision,
}: {
  symbol: string;
  coverage: Coverage;
  revision: number;
}) {
  const defaults = {
    from: dateInput(coverage.firstMs),
    to: dateInput(String(Number(coverage.lastMs) + 1)),
  };
  const [from, setFrom] = useState(defaults.from);
  const [to, setTo] = useState(defaults.to);
  const [limit, setLimit] = useState(100);
  const [query, setQuery] = useState(() =>
    period(defaults.from, defaults.to, 100),
  );
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState("");
  function submit(event: FormEvent) {
    event.preventDefault();
    try {
      setQuery(period(from, to, limit));
      setAttempt((v) => v + 1);
      setError("");
    } catch (e) {
      setError(message(e));
    }
  }
  return (
    <section className="panel history-panel">
      <div className="section-heading">
        <div>
          <p className="eyebrow">price history</p>
          <h2>mid price history</h2>
        </div>
        <span className="tag">UTC</span>
      </div>
      <form className="period-form" onSubmit={submit} noValidate>
        <label>
          from, inclusive
          <input
            type="datetime-local"
            step="0.001"
            required
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
        </label>
        <label>
          to, exclusive
          <input
            type="datetime-local"
            step="0.001"
            required
            value={to}
            onChange={(e) => setTo(e.target.value)}
          />
        </label>
        <label className="limit-field">
          limit
          <input
            type="number"
            min="1"
            max="100"
            required
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
          />
        </label>
        <button className="primary" type="submit">
          show
        </button>
        <button
          className="small-button"
          type="button"
          onClick={() => {
            setFrom(defaults.from);
            setTo(defaults.to);
            setLimit(100);
            setQuery(period(defaults.from, defaults.to, 100));
            setAttempt((v) => v + 1);
            setError("");
          }}
        >
          full range
        </button>
      </form>
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
      <Results
        key={`${symbol}:${query}`}
        symbol={symbol}
        query={query}
        revision={revision + attempt}
      />
    </section>
  );
}
