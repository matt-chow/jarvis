import { useCallback, useEffect, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { deviceLabel } from "../components/AuthScreen";
import { Button, Panel } from "../components/ui";
import { reportIfUnauthorized } from "../hooks/useSession";
import { localDb } from "../lib/db";
import { api, ApiError, isNetworkError, type DeviceToken, type PasskeyInfo, type TokenScope } from "../lib/api";
import { KINDS, KIND_LABELS, planImport, type ExportPlan, type ImportKind, type ImportReport } from "../lib/backup";
import type { ClientError } from "../lib/errors";
import { formatDate } from "../lib/format";
import { readLaunchChoice, writeLaunchChoice, type LaunchChoice } from "../lib/launch";
import { readTheme, writeTheme, type Theme } from "../lib/theme";
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

/** Per-device: where the Home Screen app opens (lib/launch.ts). Browser tabs always open Home. */
function LaunchPanel() {
  const [choice, setChoice] = useState<LaunchChoice>(readLaunchChoice);
  const [theme, setTheme] = useState<Theme>(readTheme);
  return (
    <Panel title="this device" className="lg:col-span-2">
      <label className="mb-3 flex flex-wrap items-center gap-3 text-sm">
        <span>Theme</span>
        <select
          value={theme}
          onChange={(e) => {
            const next = e.target.value as Theme;
            setTheme(next);
            writeTheme(next);
          }}
          aria-label="Theme"
          className="min-h-9 rounded-sm border border-line bg-sunken/40 px-2 text-sm focus:border-accent"
        >
          <option value="system">Match the system</option>
          <option value="light">Light</option>
          <option value="night">Night</option>
        </select>
      </label>
      <label className="flex flex-wrap items-center gap-3 text-sm">
        <span>Home Screen app opens on</span>
        <select
          value={choice}
          onChange={(e) => {
            const next = e.target.value as LaunchChoice;
            setChoice(next);
            writeLaunchChoice(next);
          }}
          aria-label="Home Screen app opens on"
          className="min-h-9 rounded-sm border border-line bg-sunken/40 px-2 text-sm focus:border-accent"
        >
          <option value="ask">Ask (JARVIS)</option>
          <option value="home">Home</option>
        </select>
      </label>
      <p className="mt-2 text-sm text-muted">
        Applies when you open Directory from its Home Screen icon. Browser tabs always open Home, and a link to another page is kept. On
        iPhone the Home Screen app keeps its own settings, so choose this inside the app.
      </p>
    </Panel>
  );
}

export function Settings({ online, session }: ModuleProps) {
  const [passkeys, setPasskeys] = useState<PasskeyInfo[]>();
  const [tokens, setTokens] = useState<DeviceToken[]>();
  const [tokenScope, setTokenScope] = useState<TokenScope>("full");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [tokenName, setTokenName] = useState("");
  const [newSecret, setNewSecret] = useState<{ name: string; secret: string; scope: TokenScope }>();

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
      const { token, secret } = await api.createToken(name, tokenScope);
      setTokens((ts) => [...(ts ?? []), token]);
      setNewSecret({ name: token.name, secret, scope: token.scope });
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
          <SignOut session={session} />
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
        <p className="text-sm text-muted">
          For native apps and scripts. A full token can read and write data but can't manage credentials. A JARVIS-only token can only talk to
          JARVIS (the iPhone Shortcut in docs/JARVIS_SHORTCUTS.md), and changes still need a spoken "yes".
        </p>
        <form onSubmit={createToken} className="mt-3 flex flex-wrap gap-2">
          <input
            value={tokenName}
            onChange={(e) => setTokenName(e.target.value)}
            placeholder="Token name (e.g. iPhone)"
            aria-label="Token name"
            maxLength={64}
            className="min-h-9 min-w-0 flex-1 rounded-sm border border-line bg-sunken/40 px-3 text-sm focus:border-accent"
          />
          <select
            value={tokenScope}
            onChange={(e) => setTokenScope(e.target.value as TokenScope)}
            aria-label="Token access"
            className="min-h-9 rounded-sm border border-line bg-sunken/40 px-2 text-sm focus:border-accent"
          >
            <option value="full">Full access</option>
            <option value="jarvis">JARVIS only (Shortcuts)</option>
          </select>
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
            {newSecret.scope === "jarvis" && (
              <p className="mt-2 text-sm text-muted">
                In the iPhone Shortcut, POST JSON <span className="font-mono text-xs">{"{ text, today }"}</span> to{" "}
                <span className="font-mono text-xs break-all">{`${location.origin}/api/v1/jarvis/ask`}</span> with the header{" "}
                <span className="font-mono text-xs">Authorization: Bearer</span> and this token, then speak the reply's{" "}
                <span className="font-mono text-xs">speech</span>. Full steps: docs/JARVIS_SHORTCUTS.md.
              </p>
            )}
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
                    {t.scope === "jarvis" ? "JARVIS only · " : ""}created {formatDate(t.created_at)}
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

      <LaunchPanel />

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

      <Diagnostics online={online} />
    </div>
  );
}

/** Errors this account's browsers reported (D-027). */
function Diagnostics({ online }: { online: boolean }) {
  const [errors, setErrors] = useState<ClientError[]>();
  const [problem, setProblem] = useState<string>();
  const [open, setOpen] = useState<string>();

  const load = useCallback(() => {
    api.clientErrors().then(setErrors, (e: unknown) => {
      reportIfUnauthorized(e);
      setProblem(describe(e));
    });
  }, []);
  useEffect(() => {
    if (online) load();
  }, [online, load]);

  const clear = async () => {
    try {
      await api.clearClientErrors();
      setErrors([]);
    } catch (e) {
      reportIfUnauthorized(e);
      setProblem(describe(e));
    }
  };

  return (
    <Panel
      title={`diagnostics${errors ? ` · ${errors.length}` : ""}`}
      className="lg:col-span-2"
      actions={
        <span className="flex gap-1.5">
          <Button className="min-h-7 px-2 text-[10px]" disabled={!online} onClick={load}>
            refresh
          </Button>
          <Button className="min-h-7 px-2 text-[10px]" variant="danger" disabled={!online || !errors?.length} onClick={() => void clear()}>
            clear
          </Button>
        </span>
      }
    >
      <p className="text-sm text-muted">Errors from your browsers, newest first (the latest 200 are kept). Useful when something misbehaves.</p>
      {problem && (
        <p role="alert" className="mt-2 text-sm text-bad">
          {problem}
        </p>
      )}
      {!online ? (
        <p className="mt-3 text-sm text-muted">Needs a connection.</p>
      ) : !errors ? (
        <p className="mt-3 text-sm text-muted">Loading…</p>
      ) : errors.length === 0 ? (
        <p className="mt-3 text-sm text-muted">No errors reported.</p>
      ) : (
        <ul className="mt-3 divide-y divide-line">
          {errors.map((e) => (
            <li key={e.id} className="py-2">
              <button type="button" aria-expanded={open === e.id} onClick={() => setOpen(open === e.id ? undefined : e.id)} className="w-full text-left">
                <span className="block font-mono text-[11px] text-muted">
                  {formatDate(e.at)} · {e.module || "app"}
                </span>
                <span className="block text-sm break-words">{e.message}</span>
              </button>
              {open === e.id && (
                <pre className="mt-2 max-h-48 overflow-auto rounded-sm bg-sunken/60 p-2 font-mono text-[10px] whitespace-pre-wrap text-muted">
                  {e.stack || "(no stack)"}
                  {"\n"}
                  {e.user_agent}
                </pre>
              )}
            </li>
          ))}
        </ul>
      )}
    </Panel>
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

/** Sign out, optionally forgetting this device's saved data (D-028). */
function SignOut({ session }: { session: ModuleProps["session"] }) {
  const small = "min-h-7 px-2 text-[10px]";
  const [confirm, setConfirm] = useState<number>();
  const ask = async () => {
    const pending = await localDb()
      .then((db) => db.outbox())
      .then((o) => o.length)
      .catch(() => 0);
    setConfirm(pending);
  };
  if (confirm !== undefined) {
    return (
      <span className="flex flex-wrap items-center justify-end gap-1.5">
        <span role="alert" className="text-xs text-muted">
          {confirm > 0 ? `${confirm} change${confirm === 1 ? " hasn't" : "s haven't"} synced yet and will be lost. ` : ""}Delete everything saved in this browser?
        </span>
        <Button className={small} onClick={() => setConfirm(undefined)}>
          keep
        </Button>
        <Button variant="danger" className={small} onClick={() => void session.signOut({ forget: true })}>
          sign out and forget
        </Button>
      </span>
    );
  }
  return (
    <span className="flex gap-1.5">
      <Button onClick={() => void session.signOut()} className={small}>
        sign out
      </Button>
      <Button onClick={() => void ask()} className={small} title="Also delete the offline copy of your data in this browser">
        sign out and forget this device
      </Button>
    </span>
  );
}
