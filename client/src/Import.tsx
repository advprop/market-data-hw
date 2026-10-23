import { useRef, useState, type FormEvent } from "react";
import { message, request } from "./api";
import { parseBatch } from "./domain";

export default function Import({
  onImported,
}: {
  onImported: (symbol: string) => void;
}) {
  const [text, setText] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  async function example() {
    setBusy(true);
    setError("");
    setSuccess("");
    try {
      const batch = await request<unknown>("/snapshots.json");
      setText(JSON.stringify(batch, null, 2));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    setSuccess("");
    try {
      const batch = parseBatch(text);
      if (
        new TextEncoder().encode(token).length < 16 ||
        !/^[\x21-\x7e]+$/.test(token)
      )
        throw new Error(
          "enter the write token: at least 16 printable ascii characters, without spaces.",
        );
      setBusy(true);
      const result = await request<{ accepted: string }>("/v1/snapshots", {
        method: "POST",
        headers: { "Content-Type": "application/json", "x-write-token": token },
        body: JSON.stringify(batch),
      });
      setSuccess(
        `snapshots saved by server: ${result.accepted}. data is available in market overview.`,
      );
      onImported(batch.snapshots[0].symbol);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel import-panel" aria-busy={busy}>
      <div className="section-heading">
        <div>
          <p className="eyebrow">data</p>
          <h2>import snapshots</h2>
        </div>
        <span className="tag">json · up to 1 mib</span>
      </div>
      <p className="description">
        import a batch of full order books. the server validates the entire
        batch before saving. importing the same data again does not create
        duplicate snapshots.
      </p>
      <form onSubmit={submit} noValidate>
        <div className="import-tools">
          <button
            type="button"
            className="small-button"
            onClick={() => void example()}
            disabled={busy}
          >
            use sample data
          </button>
          <button
            type="button"
            className="small-button"
            onClick={() => fileRef.current?.click()}
            disabled={busy}
          >
            choose json file
          </button>
          <a className="text-link" href="/snapshots.json" download>
            download sample ↗
          </a>
        </div>
        <input
          className="sr-only"
          tabIndex={-1}
          ref={fileRef}
          type="file"
          accept=".json,application/json"
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            setError("");
            setSuccess("");
            if (file.size > 1_048_576) {
              setError("file size must not exceed 1 mib.");
              return;
            }
            try {
              setText(await file.text());
            } catch {
              setError("could not read the file.");
            }
            e.target.value = "";
          }}
        />
        <label>
          snapshot batch
          <textarea
            spellCheck={false}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={'{"snapshots": [{"symbol": "BTC-USDT", …}]}'}
            required
            disabled={busy}
          />
        </label>
        <p className="footnote">
          prices, quantities, timestamps and sequence are strings. price
          "62001000000" means 62 001. prices and quantities use a scale of 10⁶.
        </p>
        <div className="import-submit">
          <label>
            write token
            <input
              type="password"
              autoComplete="off"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="server write token"
              disabled={busy}
              required
            />
          </label>
          <button className="primary" type="submit" disabled={busy}>
            {busy ? "uploading…" : "upload to server"}
          </button>
        </div>
        <p className="footnote">
          the token stays in memory for this page only.
        </p>
        {error && (
          <p className="notice error" role="alert">
            {error}
          </p>
        )}
        {success && (
          <p className="notice success" role="status">
            {success}
          </p>
        )}
      </form>
    </section>
  );
}
