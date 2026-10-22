import { useCallback, useEffect, useState } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel } from "../components/ui";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError, toObjectKey, type ObjectInfo } from "../lib/api";
import { formatBytes } from "../lib/format";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.code === "too_large") return "That file is larger than the 10 MB limit.";
    if (e.status === 401) return "Your session has ended. Sign in again.";
    return `Request failed: ${e.code} (${e.status}).`;
  }
  return "Could not reach the server.";
}

/** Private R2 storage: upload / list / download / delete, scoped to the owner. */
export function Files({ online }: ModuleProps) {
  const [objects, setObjects] = useState<ObjectInfo[]>();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ tone: "ok" | "bad"; text: string }>();

  const run = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    setMessage(undefined);
    try {
      await fn();
    } catch (e) {
      reportIfUnauthorized(e);
      setMessage({ tone: "bad", text: describe(e) });
    } finally {
      setBusy(false);
    }
  }, []);

  const refresh = useCallback(() => run(async () => setObjects(await api.listObjects())), [run]);

  useEffect(() => {
    if (online) void refresh();
  }, [online, refresh]);

  const upload = (file: File) =>
    run(async () => {
      const info = await api.upload(toObjectKey(file.name), file);
      setMessage({ tone: "ok", text: `Uploaded ${info.key}.` });
      setObjects(await api.listObjects());
    });

  const download = (key: string) =>
    run(async () => {
      const url = URL.createObjectURL(await api.download(key));
      const a = Object.assign(document.createElement("a"), { href: url, download: key.split("/").pop() });
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });

  const remove = (key: string) =>
    run(async () => {
      await api.removeObject(key);
      setObjects((os) => os?.filter((o) => o.key !== key));
    });

  if (!online) {
    return (
      <Panel title="private files">
        <p className="text-sm text-muted">Files live in private cloud storage and need a connection.</p>
      </Panel>
    );
  }

  const small = "min-h-7 px-2 text-[10px]";
  return (
    <Panel
      title={`private files${objects ? ` · ${objects.length}` : ""}`}
      actions={
        <Button onClick={() => void refresh()} disabled={busy} className={small}>
          refresh
        </Button>
      }
    >
      <label className="flex cursor-pointer flex-col items-center gap-1 rounded-sm border border-dashed border-line px-4 py-6 text-center hover:border-accent">
        <span className="font-mono text-sm">{busy ? "Working…" : "Choose a file to upload"}</span>
        <span className="text-xs text-muted">Max 10 MB · private to your account</span>
        <input
          type="file"
          className="sr-only"
          disabled={busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) void upload(file);
          }}
        />
      </label>

      {message && (
        <p role={message.tone === "bad" ? "alert" : "status"} className={`mt-3 text-sm ${message.tone === "bad" ? "text-bad" : "text-muted"}`}>
          {message.text}
        </p>
      )}

      {!objects && !message && <p className="mt-4 text-sm text-muted">Loading…</p>}
      {objects && objects.length === 0 && <p className="mt-4 text-sm text-muted">No files yet.</p>}
      {objects && objects.length > 0 && (
        <ul className="mt-4 divide-y divide-line">
          {objects.map((o) => (
            <li key={o.key} className="flex flex-wrap items-center gap-x-4 gap-y-1 py-2">
              <span className="min-w-0 flex-1 truncate font-mono text-sm">{o.key}</span>
              <span className="font-mono text-xs text-muted">{formatBytes(o.size)}</span>
              <span className="flex gap-1.5">
                <Button onClick={() => void download(o.key)} disabled={busy} className={small}>
                  get
                </Button>
                <Button variant="danger" onClick={() => void remove(o.key)} disabled={busy} className={small}>
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
