import { useCallback, useEffect, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel } from "../components/ui";
import { ApiError, StorageClient, toObjectKey, type ObjectInfo } from "../lib/api";
import { formatBytes } from "../lib/format";

// Session-only: the token is forgotten when the tab closes. Replaced by real
// sign-in in Sprint 01.
const TOKEN_KEY = "directory:api-token";

function readToken() {
  try {
    return sessionStorage.getItem(TOKEN_KEY) ?? "";
  } catch {
    return "";
  }
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 401) return "Token rejected.";
    if (e.code === "auth_not_configured") return "Server has no API token configured (fails closed).";
    return `Request failed: ${e.code} (${e.status}).`;
  }
  return "Could not reach the API.";
}

/** R2 proof of concept: authenticated upload / list / download / delete. */
export function Files({ online }: ModuleProps) {
  const [token, setToken] = useState(readToken);
  const [draft, setDraft] = useState("");
  const [objects, setObjects] = useState<ObjectInfo[]>();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();

  const run = useCallback(
    async (fn: (c: StorageClient) => Promise<void>) => {
      if (!token) return;
      setBusy(true);
      setMessage(undefined);
      try {
        await fn(new StorageClient(token));
      } catch (e) {
        setMessage(describe(e));
        if (e instanceof ApiError && e.status === 401) setObjects(undefined);
      } finally {
        setBusy(false);
      }
    },
    [token],
  );

  const refresh = useCallback(() => run(async (c) => setObjects(await c.list())), [run]);

  useEffect(() => {
    if (token && online) void refresh();
  }, [token, online, refresh]);

  function saveToken(e: FormEvent) {
    e.preventDefault();
    const t = draft.trim();
    try {
      sessionStorage.setItem(TOKEN_KEY, t);
    } catch {
      /* storage blocked: keep in memory only */
    }
    setToken(t);
    setDraft("");
  }

  function forget() {
    try {
      sessionStorage.removeItem(TOKEN_KEY);
    } catch {
      /* ignore */
    }
    setToken("");
    setObjects(undefined);
  }

  async function onUpload(file: File) {
    await run(async (c) => {
      const info = await c.upload(toObjectKey(file.name), file);
      setMessage(`Uploaded ${info.key}.`);
      setObjects(await c.list());
    });
  }

  async function download(key: string) {
    await run(async (c) => {
      const url = URL.createObjectURL(await c.download(key));
      const a = Object.assign(document.createElement("a"), { href: url, download: key.split("/").pop() });
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }

  async function remove(key: string) {
    await run(async (c) => {
      await c.remove(key);
      setObjects((os) => os?.filter((o) => o.key !== key));
    });
  }

  if (!online) {
    return (
      <Panel title="private files">
        <p className="text-sm text-muted">Files live in private cloud storage and need a connection.</p>
      </Panel>
    );
  }

  if (!token) {
    return (
      <Panel title="private files · locked">
        <form onSubmit={saveToken} className="flex max-w-lg flex-col gap-3">
          <p className="text-sm text-muted">
            Storage is private and every request is authenticated by the server. Enter the API token configured as
            the <code className="font-mono text-ink">DIRECTORY_API_TOKEN</code> secret. It is kept for this tab only.
          </p>
          <input
            type="password"
            autoComplete="off"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="API token"
            aria-label="API token"
            className="min-h-10 rounded-sm border border-line bg-sunken/40 px-3 font-mono text-sm outline-none focus:border-accent"
          />
          <div>
            <Button type="submit" variant="primary" disabled={!draft.trim()}>
              unlock
            </Button>
          </div>
        </form>
      </Panel>
    );
  }

  return (
    <Panel
      title={`private files${objects ? ` · ${objects.length}` : ""}`}
      actions={
        <div className="flex gap-1.5">
          <Button onClick={() => void refresh()} disabled={busy} className="min-h-7 px-2 text-[10px]">
            refresh
          </Button>
          <Button onClick={forget} className="min-h-7 px-2 text-[10px]">
            lock
          </Button>
        </div>
      }
    >
      <label className="flex cursor-pointer flex-col items-center gap-1 rounded-sm border border-dashed border-line px-4 py-6 text-center hover:border-accent">
        <span className="font-mono text-sm">Choose a file to upload</span>
        <span className="text-xs text-muted">Max 10 MB · stored in a private R2 bucket</span>
        <input
          type="file"
          className="sr-only"
          disabled={busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) void onUpload(file);
          }}
        />
      </label>

      {message && <p className="mt-3 font-mono text-xs text-muted">{message}</p>}

      {objects && objects.length === 0 && <p className="mt-4 text-sm text-muted">No files yet.</p>}
      {objects && objects.length > 0 && (
        <ul className="mt-4 divide-y divide-line">
          {objects.map((o) => (
            <li key={o.key} className="flex flex-wrap items-center gap-x-4 gap-y-1 py-2">
              <span className="min-w-0 flex-1 truncate font-mono text-sm">{o.key}</span>
              <span className="font-mono text-xs text-muted">{formatBytes(o.size)}</span>
              <span className="flex gap-1.5">
                <Button onClick={() => void download(o.key)} disabled={busy} className="min-h-7 px-2 text-[10px]">
                  get
                </Button>
                <Button variant="danger" onClick={() => void remove(o.key)} disabled={busy} className="min-h-7 px-2 text-[10px]">
                  del
                </Button>
              </span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
