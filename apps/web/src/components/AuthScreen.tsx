import { useState, type FormEvent } from "react";
import { api, ApiError, type SessionUser } from "../lib/api";
import {
  assertionRequest,
  creationRequest,
  describePasskeyError,
  encodeAssertion,
  encodeRegistration,
  passkeysSupported,
} from "../lib/webauthn";
import { Button, Panel } from "./ui";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const API_MESSAGES: Record<string, string> = {
  setup_token_invalid: "That setup token is not correct.",
  setup_unavailable: "Directory already has an owner. Sign in instead.",
  auth_not_configured: "Sign-in is not configured on the server (DIRECTORY_RP_ID / DIRECTORY_ORIGINS / setup token).",
  authentication_failed: "That passkey was not accepted.",
  challenge_invalid: "The sign-in request expired. Try again.",
  rate_limited: "Too many attempts. Wait a few minutes and try again.",
};

function describe(e: unknown): string {
  if (e instanceof ApiError) return API_MESSAGES[e.code] ?? `Request failed (${e.code}).`;
  return describePasskeyError(e);
}

interface Props {
  setupRequired: boolean;
  unavailable?: string;
  onSignedIn: (user: SessionUser) => void;
  onRetry: () => void;
}

export function AuthScreen({ setupRequired, unavailable, onSignedIn, onRetry }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [setupToken, setSetupToken] = useState("");
  const [displayName, setDisplayName] = useState("");
  const supported = passkeysSupported();

  async function run(fn: () => Promise<SessionUser>) {
    setBusy(true);
    setError(undefined);
    try {
      onSignedIn(await fn());
    } catch (e) {
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  }

  const signIn = () =>
    run(async () => {
      const opts = await api.loginOptions();
      const cred = (await navigator.credentials.get(assertionRequest(opts))) as PublicKeyCredential | null;
      if (!cred) throw new Error("No passkey was selected.");
      return api.loginVerify(encodeAssertion(opts.challengeId, cred));
    });

  const setup = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const opts = await api.setupOptions(setupToken.trim(), displayName.trim());
      const cred = (await navigator.credentials.create(creationRequest(opts))) as PublicKeyCredential | null;
      if (!cred) throw new Error("No passkey was created.");
      return api.setupVerify(encodeRegistration(opts.challengeId, cred, deviceLabel()));
    });
  };

  return (
    <div className="flex min-h-full items-center justify-center px-4 py-10">
      <div className="w-full max-w-md">
        <p className="cursor mb-6 text-center font-mono text-sm font-semibold tracking-[0.25em]">DIRECTORY</p>
        {unavailable ? (
          <Panel title="offline">
            <p className="text-sm">Directory can't reach its server, and this device has no saved session.</p>
            <p className="mt-2 font-mono text-xs text-muted">{unavailable}</p>
            <Button className="mt-4" onClick={onRetry}>
              retry
            </Button>
          </Panel>
        ) : setupRequired ? (
          <Panel title="first-run setup">
            <form onSubmit={setup} className="flex flex-col gap-3">
              <p className="text-sm text-muted">
                Create the owner account. You'll sign in with a passkey (Windows Hello, Face ID, Touch ID, or a security
                key). The setup token is the <code className="font-mono text-ink">DIRECTORY_SETUP_TOKEN</code> secret.
              </p>
              <label className="flex flex-col gap-1">
                <span className="label">your name</span>
                <input
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  maxLength={64}
                  autoComplete="name"
                  className="min-h-10 rounded-sm border border-line bg-sunken/40 px-3 text-sm outline-none focus:border-accent"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="label">setup token</span>
                <input
                  type="password"
                  value={setupToken}
                  onChange={(e) => setSetupToken(e.target.value)}
                  autoComplete="off"
                  className="min-h-10 rounded-sm border border-line bg-sunken/40 px-3 font-mono text-sm outline-none focus:border-accent"
                />
              </label>
              <Button type="submit" variant="primary" disabled={busy || !supported || !setupToken.trim()}>
                {busy ? "waiting for passkey…" : "create passkey"}
              </Button>
            </form>
          </Panel>
        ) : (
          <Panel title="sign in">
            <p className="text-sm text-muted">Use the passkey you registered for Directory.</p>
            <Button className="mt-4 w-full" variant="primary" onClick={() => void signIn()} disabled={busy || !supported}>
              {busy ? "waiting for passkey…" : "sign in with passkey"}
            </Button>
          </Panel>
        )}
        {!supported && !unavailable && (
          <p className="mt-3 text-sm text-bad">
            This browser can't use passkeys here. Use a current browser over HTTPS (or http://localhost).
          </p>
        )}
        {error && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

/** A friendly default name for a new passkey, e.g. "Windows · Edge". */
export function deviceLabel(ua = navigator.userAgent): string {
  const os = /iPhone|iPad/.test(ua)
    ? "iOS"
    : /Android/.test(ua)
      ? "Android"
      : /Windows/.test(ua)
        ? "Windows"
        : /Mac OS X/.test(ua)
          ? "macOS"
          : "Device";
  const browser = /Edg\//.test(ua) ? "Edge" : /Chrome\//.test(ua) ? "Chrome" : /Firefox\//.test(ua) ? "Firefox" : /Safari\//.test(ua) ? "Safari" : "";
  return browser ? `${os} · ${browser}` : os;
}
