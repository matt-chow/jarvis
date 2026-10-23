import { useCallback, useEffect, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { deviceLabel } from "../components/AuthScreen";
import { Button, Panel } from "../components/ui";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError, isNetworkError, type DeviceToken, type PasskeyInfo } from "../lib/api";
import { KINDS, KIND_LABELS, planImport, type ExportPlan, type ImportKind, type ImportReport } from "../lib/backup";
import { formatDate } from "../lib/format";
import { localDate } from "../lib/tasks";
import { creationRequest, describePasskeyError, encodeRegistration } from "../lib/webauthn";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.code === "last_passkey") return "You can't remove your only passkey.";
    if (e.code === "session_required") return "Sign in with a passkey to manage credentials.";
    return `Request failed (${e.code}).`;
  }
  if (isNetworkError(e)) return "Can't reach the server right now.";
  return describePasskeyError(e);
}

export function Settings({ online, session }: ModuleProps) {
  const [passkeys, setPasskeys] = useState<PasskeyInfo[]>();
  const [tokens, setTokens] = useState<DeviceToken[]>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [tokenName, setTokenName] = useState("");
  const [newSecret, setNewSecret] = useState<{ name: string; secret: string }>();

  const run = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const load = useCallback(
    () =>
      run(async () => {
        const [p, t] = await Promise.all([api.passkeys(), api.tokens()]);
        setPasskeys(p);
        setTokens(t);
      }),
    [run],
  );

  useEffect(() => {
    if (online) void load();
  }, [online, load]);

  const addPasskey = () =>
    run(async () => {
      const opts = await api.addPasskeyOptions();
      const cred = (await navigator.credentials.create(creationRequest(opts))) as PublicKeyCredential | null;
      if (!cred) throw new Error("No passkey was created.");
      const added = await api.addPasskeyVerify(encodeRegistration(opts.challengeId, cred, deviceLabel()));
      setPasskeys((ps) => [...(ps ?? []), added]);
    });

  const removePasskey = (id: string) =>
    run(async () => {
      await api.deletePasskey(id);
      setPasskeys((ps) => ps?.filter((p) => p.id !== id));
    });

  const createToken = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const name = tokenName.trim() || "Device";
      const { token, secret } = await api.createToken(name);
      setTokens((ts) => [...(ts ?? []), token]);
      setNewSecret({ name: token.name, secret });
      setTokenName("");
    });
  };

  const revokeToken = (id: string) =>
    run(async () => {
      await api.deleteToken(id);
      setTokens((ts) => ts?.filter((t) => t.id !== id));
    });

  const user = session.status.kind === "signed-in" ? session.status.user : undefined;
  const verified = session.status.kind === "signed-in" && session.status.verified;
  const small = "min-h-7 px-2 text-[10px]";

  return (
    <div className="grid gap-5 lg:grid-cols-2">
      <Panel
        title="account"
        className="lg:col-span-2"
        actions={
          <Button onClick={() => void session.signOut()} className={small}>
            sign out
          </Button>
        }
      >
        <p className="text-sm">
          Signed in as <span className="font-medium">{user?.displayName ?? "…"}</span>
        </p>
        {!verified && (
          <p className="mt-2 text-sm text-warn">
            Couldn't reach the server. Showing the session saved on this device; it will be re-checked when you reconnect.
          </p>
        )}
        {!online && <p className="mt-2 text-sm text-muted">Credential management needs a connection.</p>}
        {error && (
          <p role="alert" className="mt-2 text-sm text-bad">
            {error}
          </p>
        )}
      </Panel>

      <Panel
        title={`passkeys${passkeys ? ` · ${passkeys.length}` : ""}`}
        actions={
          <Button onClick={() => void addPasskey()} disabled={busy || !online} variant="primary" className={small}>
            + add
          </Button>
        }
      >
        {!passkeys ? (
          <p className="text-sm text-muted">{online ? "Loading…" : "Unavailable offline."}</p>
        ) : (
          <ul className="divide-y divide-line">
            {passkeys.map((p) => (
              <li key={p.id} className="flex items-center gap-3 py-2">
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm">{p.name}</span>
                  <span className="font-mono text-[11px] text-muted">
                    added {formatDate(p.created_at)}
                    {p.last_used_at ? ` · used ${formatDate(p.last_used_at)}` : ""}
                  </span>
                </span>
                <Button
                  variant="danger"
                  className={small}
                  disabled={busy || passkeys.length <= 1}
                  title={passkeys.length <= 1 ? "Your only passkey can't be removed" : undefined}
                  onClick={() => void removePasskey(p.id)}
                >
                  remove
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title={`device tokens${tokens ? ` · ${tokens.length}` : ""}`}>
        <p className="text-sm text-muted">For native apps and scripts. A token can read and write data but can't manage credentials.</p>
        <form onSubmit={createToken} className="mt-3 flex gap-2">
          <input
            value={tokenName}
            onChange={(e) => setTokenName(e.target.value)}
            placeholder="Token name (e.g. iPhone)"
            aria-label="Token name"
            maxLength={64}
            className="min-h-9 min-w-0 flex-1 rounded-sm border border-line bg-sunken/40 px-3 text-sm focus:border-accent"
          />
          <Button type="submit" disabled={busy || !online}>
            create
          </Button>
        </form>
        {newSecret && (
          <div role="status" className="mt-3 rounded-sm border border-accent bg-sunken/60 p-3">
            <p className="text-sm">
              Copy the token for <span className="font-medium">{newSecret.name}</span> now. It won't be shown again.
            </p>
            <code className="mt-2 block font-mono text-xs break-all select-all">{newSecret.secret}</code>
            <div className="mt-2 flex gap-2">
              <Button className={small} onClick={() => void navigator.clipboard?.writeText(newSecret.secret)}>
                copy
              </Button>
              <Button className={small} onClick={() => setNewSecret(undefined)}>
                done
              </Button>
            </div>
          </div>
        )}
        {tokens && tokens.length === 0 && <p className="mt-3 text-sm text-muted">No device tokens.</p>}
        {tokens && tokens.length > 0 && (
          <ul className="mt-3 divide-y divide-line">
            {tokens.map((t) => (
              <li key={t.id} className="flex items-center gap-3 py-2">
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm">{t.name}</span>
                  <span className="font-mono text-[11px] text-muted">
                    created {formatDate(t.created_at)}
                    {t.last_used_at ? ` · used ${formatDate(t.last_used_at)}` : " · never used"}
                  </span>
                </span>
                <Button variant="danger" className={small} disabled={busy} onClick={() => void revokeToken(t.id)}>
                  revoke
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title="your data" className="lg:col-span-2">
        <p className="text-sm text-muted">
          Download everything as one JSON file: tasks, projects, notes, journal, inbox, habits, events, playlists, links, and the file list. File
          contents stay in Files. Sign-in credentials are never included.
        </p>
        <Button className="mt-3" disabled={busy || !online} onClick={() => void run(downloadExport)}>
          export my data
        </Button>
        <Restore online={online} />
      </Panel>
    </div>
  );
}

type Totals = Record<ImportKind, ImportReport>;

/** Restore from an export file: adds what's missing, never overwrites (D-025). */
function Restore({ online }: { online: boolean }) {
  const [plan, setPlan] = useState<ExportPlan>();
  const [problem, setProblem] = useState<string>();
  const [progress, setProgress] = useState<string>();
  const [totals, setTotals] = useState<Partial<Totals>>();
  const [running, setRunning] = useState(false);

  const choose = async (file: File | undefined) => {
    setPlan(undefined);
    setTotals(undefined);
    setProblem(undefined);
    if (!file) return;
    try {
      setPlan(planImport(JSON.parse(await file.text())));
    } catch (e) {
      setProblem(e instanceof SyntaxError ? "That file isn't valid JSON." : String(e instanceof Error ? e.message : e));
    }
  };

  const restore = async (p: ExportPlan) => {
    setRunning(true);
    setProblem(undefined);
    const sum: Partial<Totals> = {};
    try {
      for (const [i, b] of p.batches.entries()) {
        setProgress(`Restoring ${KIND_LABELS[b.kind].toLowerCase()}… (batch ${i + 1} of ${p.batches.length})`);
        const r = await api.importBatch(b.kind, b.records);
        const t = (sum[b.kind] ??= { inserted: 0, skipped: 0, rejected: [] });
        t.inserted += r.inserted;
        t.skipped += r.skipped;
        t.rejected.push(...r.rejected);
        setTotals({ ...sum });
      }
      setProgress("Restore finished.");
    } catch (e) {
      reportIfUnauthorized(e);
      // Batches already sent are kept; running the restore again skips them.
      setProblem(`Restore stopped: ${describe(e)} Run it again to continue; restored records are skipped.`);
      setProgress(undefined);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="mt-5 border-t border-line pt-4">
      <p className="label mb-1">restore</p>
      <p className="text-sm text-muted">
        Add records from an export file to this account. Anything already here (same id, or a journal day that has an entry) is kept as
        is, so it's safe to run more than once.
      </p>
      <input
        type="file"
        accept="application/json,.json"
        aria-label="Export file to restore"
        disabled={!online || running}
        onChange={(e) => void choose(e.target.files?.[0])}
        className="mt-3 text-xs"
      />
      {problem && (
        <p role="alert" className="mt-2 text-sm text-bad">
          {problem}
        </p>
      )}
      {plan && (
        <div className="mt-3 text-sm">
          <p>
            This file has{" "}
            {KINDS.filter((k) => plan.counts[k] > 0)
              .map((k) => `${plan.counts[k]} ${KIND_LABELS[k].toLowerCase()}`)
              .join(", ") || "nothing to restore"}
            .
          </p>
          {plan.files > 0 && (
            <p className="mt-1 text-muted">
              It lists {plan.files} file{plan.files === 1 ? "" : "s"}, but file contents aren't in exports: upload those again in Files.
            </p>
          )}
          <Button className="mt-2" variant="primary" disabled={running || !online || plan.batches.length === 0} onClick={() => void restore(plan)}>
            {running ? "restoring…" : "restore"}
          </Button>
        </div>
      )}
      {progress && (
        <p role="status" className="mt-2 text-sm text-muted">
          {progress}
        </p>
      )}
      {totals && (
        <ul className="mt-2 font-mono text-[11px]">
          {KINDS.filter((k) => totals[k]).map((k) => {
            const t = totals[k]!;
            return (
              <li key={k}>
                {KIND_LABELS[k]}: {t.inserted} added, {t.skipped} already here
                {t.rejected.length > 0 && (
                  <span className="text-bad">
                    , {t.rejected.length} not restored ({t.rejected.slice(0, 3).map((r) => r.reason).join("; ")}
                    {t.rejected.length > 3 ? "; …" : ""})
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

async function downloadExport() {
  const blob = await api.exportData();
  const url = URL.createObjectURL(blob);
  const a = Object.assign(document.createElement("a"), { href: url, download: `directory-export-${localDate()}.json` });
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
