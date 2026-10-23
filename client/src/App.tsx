import { useState } from "react";
import {
  fixed,
  formatNumber,
  time,
  type Coverage,
  type Snapshot,
  type Summary,
} from "./domain";
import { useRemote } from "./useRemote";
import Book from "./Book";
import History from "./History";
import Import from "./Import";

function Dashboard({ symbol, revision }: { symbol: string; revision: number }) {
  const [depth, setDepth] = useState(10);
  const [levels, setLevels] = useState("10");
  const [depthError, setDepthError] = useState("");
  const book = useRemote<Snapshot>(
    `/v1/book/${symbol}?levels=${depth}`,
    revision,
  );
  const summary = useRemote<Summary>(`/v1/summary/${symbol}`, revision);
  const coverage = useRemote<Coverage>(`/v1/range/${symbol}`, revision);
  const metrics = summary.data;
  const cards = metrics
    ? [
        {
          name: "best bid",
          value: fixed(metrics.bestBid),
          detail: "buy price",
          style: "positive",
        },
        {
          name: "best ask",
          value: fixed(metrics.bestAsk),
          detail: "sell price",
          style: "negative",
        },
        {
          name: "spread",
          value: fixed(metrics.spread),
          detail: "ask minus bid",
          style: "",
        },
        {
          name: "mid price",
          value: formatNumber(metrics.midPrice),
          detail: "average of best bid and ask",
          style: "",
        },
      ]
    : [];
  return (
    <>
      {summary.error && (
        <p className="notice error" role="alert">
          {summary.error}
        </p>
      )}
      <section
        className="metrics"
        aria-label="Latest order book metrics"
        aria-busy={summary.loading}
      >
        {summary.loading ? (
          <p className="placeholder" role="status">
            Loading metrics…
          </p>
        ) : (
          cards.map((card) => (
            <article className="metric" key={card.name}>
              <p className="eyebrow">{card.name}</p>
              <p className={`metric-value ${card.style}`}>{card.value}</p>
              <p className="muted">{card.detail}</p>
            </article>
          ))
        )}
      </section>
      <div className="dashboard-grid">
        <section className="panel order-book" aria-busy={book.loading}>
          <div className="section-heading">
            <div>
              <p className="eyebrow">latest snapshot</p>
              <h2>
                order book <span className="muted">/ {symbol}</span>
              </h2>
            </div>
            <form
              className="depth-form"
              onSubmit={(e) => {
                e.preventDefault();
                const value = Number(levels);
                if (!Number.isInteger(value) || value < 1 || value > 100) {
                  setDepthError("depth must be an integer from 1 to 100.");
                  return;
                }
                setDepth(value);
                setDepthError("");
              }}
              noValidate
            >
              <label>
                depth
                <input
                  type="number"
                  min="1"
                  max="100"
                  value={levels}
                  onChange={(e) => setLevels(e.target.value)}
                />
              </label>
              <button className="small-button" type="submit">
                apply
              </button>
            </form>
          </div>
          {depthError && (
            <p className="notice error" role="alert">
              {depthError}
            </p>
          )}
          {book.error && (
            <p className="notice error" role="alert">
              {book.error}
            </p>
          )}
          {book.loading ? (
            <p className="placeholder" role="status">
              loading order book…
            </p>
          ) : (
            book.data && <Book snapshot={book.data} />
          )}
        </section>
        <aside className="panel balance-panel">
          <p className="eyebrow">buy and sell balance</p>
          <h2>order book volumes</h2>
          {metrics && (
            <>
              <div className="balance-value">
                {formatNumber(metrics.imbalance)}
                <span>imbalance</span>
              </div>
              <div
                className="balance-bar"
                role="img"
                aria-label={`buy share ${formatNumber((metrics.imbalance + 1) * 50)} percent`}
              >
                <span style={{ width: `${(metrics.imbalance + 1) * 50}%` }} />
              </div>
              <dl className="volume-list">
                <div>
                  <dt>
                    <i className="dot bid-dot" />
                    buy
                  </dt>
                  <dd className="positive">{fixed(metrics.bidQuantity)}</dd>
                </div>
                <div>
                  <dt>
                    <i className="dot ask-dot" />
                    sell
                  </dt>
                  <dd className="negative">{fixed(metrics.askQuantity)}</dd>
                </div>
              </dl>
              <p className="footnote">
                based on all levels of the latest snapshot, regardless of the
                selected depth.
              </p>
            </>
          )}
          <div className="coverage">
            <p className="eyebrow">data coverage</p>
            {coverage.error && (
              <p className="notice error" role="alert">
                {coverage.error}
              </p>
            )}
            {coverage.data && (
              <>
                <p>
                  <strong>{coverage.data.count}</strong> unique snapshots
                </p>
                <p className="footnote">
                  {time(coverage.data.firstMs)}
                  <br />
                  {time(coverage.data.lastMs)} UTC
                </p>
              </>
            )}
          </div>
        </aside>
      </div>
      {coverage.data && (
        <History
          key={symbol}
          symbol={symbol}
          coverage={coverage.data}
          revision={revision}
        />
      )}
    </>
  );
}

export default function App() {
  const [revision, setRevision] = useState(0);
  const [selected, setSelected] = useState("");
  const [tab, setTab] = useState<"market" | "import">("market");
  const health = useRemote<{ status: string }>("/health", revision);
  const symbols = useRemote<{ symbols: string[] }>("/v1/symbols", revision);
  const symbol = symbols.data?.symbols.includes(selected)
    ? selected
    : (symbols.data?.symbols[0] ?? "");
  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label="l2 home">
          <span className="brand-icon">
            L<span>2</span>
          </span>
          <span>
            MARKET DATA<span className="brand-sub">ORDER BOOK EXPLORER</span>
          </span>
        </a>
        <div
          className={`connection ${health.error ? "offline" : ""}`}
          role="status"
        >
          <i className="dot" />
          {health.loading
            ? "Checking connection…"
            : health.error
              ? "Disconnected"
              : "Server and database connected"}
        </div>
      </header>
      <main>
        <div className="page-heading">
          <div>
            <p className="eyebrow">market data / l2</p>
            <h1>
              market overview<span className="heading-dot">.</span>
            </h1>
            <p className="description">
              full order book snapshots. prices, volumes and history.
            </p>
          </div>
          <button
            className="refresh-button"
            onClick={() => setRevision((v) => v + 1)}
            disabled={symbols.loading || health.loading}
          >
            ↻ <span>refresh data</span>
          </button>
        </div>
        {health.error && (
          <p className="notice error" role="alert">
            {health.error}
          </p>
        )}
        <nav className="workspace-nav" aria-label="sections">
          <div className="tabs">
            <button
              aria-current={tab === "market" ? "page" : undefined}
              className={tab === "market" ? "active" : ""}
              onClick={() => setTab("market")}
            >
              market overview
            </button>
            <button
              aria-current={tab === "import" ? "page" : undefined}
              className={tab === "import" ? "active" : ""}
              onClick={() => setTab("import")}
            >
              import snapshots <span>↗</span>
            </button>
          </div>
          <span className="workspace-note">l2 snapshots · utc time</span>
        </nav>
        {tab === "import" ? (
          <Import
            onImported={(value) => {
              setSelected(value);
              setRevision((v) => v + 1);
            }}
          />
        ) : (
          <>
            <div className="instrument-bar">
              <label>
                instrument
                <select
                  value={symbol}
                  onChange={(e) => setSelected(e.target.value)}
                  disabled={symbols.loading || !symbols.data?.symbols.length}
                >
                  <option value="" disabled>
                    {symbols.loading ? "loading…" : "select an instrument"}
                  </option>
                  {symbols.data?.symbols.map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
              </label>
              <p className="muted">
                {symbols.data
                  ? `${symbols.data.symbols.length} instruments available`
                  : "loading instruments"}
              </p>
              <span className="tag">full snapshots</span>
            </div>
            {symbols.error && (
              <p className="notice error" role="alert">
                {symbols.error}
              </p>
            )}
            {symbols.loading ? (
              <p className="placeholder" role="status">
                loading instruments…
              </p>
            ) : symbol ? (
              <Dashboard key={symbol} symbol={symbol} revision={revision} />
            ) : (
              !symbols.error && (
                <section className="panel empty">
                  <p className="eyebrow">getting started</p>
                  <h2>no snapshots yet</h2>
                  <p className="description">
                    import a json file or use the sample to explore order books
                    and history.
                  </p>
                  <button className="primary" onClick={() => setTab("import")}>
                    import your first snapshots ↗
                  </button>
                </section>
              )
            )}
          </>
        )}
      </main>
      <footer>
        <span>L2 / MARKET DATA</span>
        <span>Rust · ClickHouse · React</span>
        <span>data refreshes on request</span>
      </footer>
    </div>
  );
}
