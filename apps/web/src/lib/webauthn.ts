/**
 * Browser side of passkey ceremonies: converts server options (base64url) into
 * WebAuthn API inputs and encodes results back into the JSON the API expects.
 */

export interface CreationOptionsJson {
  challengeId: string;
  publicKey: {
    challenge: string;
    rp: { id: string; name: string };
    user: { id: string; name: string; displayName: string };
    pubKeyCredParams: { type: "public-key"; alg: number }[];
    authenticatorSelection: AuthenticatorSelectionCriteria;
    excludeCredentials: { type: "public-key"; id: string }[];
    attestation: AttestationConveyancePreference;
    timeout: number;
  };
}

export interface RequestOptionsJson {
  challengeId: string;
  publicKey: {
    challenge: string;
    rpId: string;
    allowCredentials: { type: "public-key"; id: string }[];
    userVerification: UserVerificationRequirement;
    timeout: number;
  };
}

export interface RegistrationJson {
  challengeId: string;
  credentialId: string;
  clientDataJson: string;
  authenticatorData: string;
  publicKey: string;
  algorithm: number;
  name?: string;
}

export interface AssertionJson {
  challengeId: string;
  credentialId: string;
  clientDataJson: string;
  authenticatorData: string;
  signature: string;
  userHandle?: string;
}

export function toBase64Url(buf: ArrayBuffer | Uint8Array): string {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromBase64Url(s: string): Uint8Array<ArrayBuffer> {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  const bin = atob(b64);
  const out = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function passkeysSupported(): boolean {
  return typeof window !== "undefined" && "PublicKeyCredential" in window && window.isSecureContext;
}

export function creationRequest(o: CreationOptionsJson): CredentialCreationOptions {
  const pk = o.publicKey;
  return {
    publicKey: {
      challenge: fromBase64Url(pk.challenge),
      rp: pk.rp,
      user: { id: fromBase64Url(pk.user.id), name: pk.user.name, displayName: pk.user.displayName },
      pubKeyCredParams: pk.pubKeyCredParams,
      authenticatorSelection: pk.authenticatorSelection,
      excludeCredentials: pk.excludeCredentials.map((c) => ({ type: c.type, id: fromBase64Url(c.id) })),
      attestation: pk.attestation,
      timeout: pk.timeout,
    },
  };
}

export function assertionRequest(o: RequestOptionsJson): CredentialRequestOptions {
  const pk = o.publicKey;
  return {
    publicKey: {
      challenge: fromBase64Url(pk.challenge),
      rpId: pk.rpId,
      allowCredentials: pk.allowCredentials.map((c) => ({ type: c.type, id: fromBase64Url(c.id) })),
      userVerification: pk.userVerification,
      timeout: pk.timeout,
    },
  };
}

export function encodeRegistration(challengeId: string, cred: PublicKeyCredential, name?: string): RegistrationJson {
  const res = cred.response as AuthenticatorAttestationResponse;
  const publicKey = res.getPublicKey();
  if (!publicKey) throw new Error("This authenticator's key type is not supported.");
  return {
    challengeId,
    credentialId: toBase64Url(cred.rawId),
    clientDataJson: toBase64Url(res.clientDataJSON),
    authenticatorData: toBase64Url(res.getAuthenticatorData()),
    publicKey: toBase64Url(publicKey),
    algorithm: res.getPublicKeyAlgorithm(),
    name,
  };
}

export function encodeAssertion(challengeId: string, cred: PublicKeyCredential): AssertionJson {
  const res = cred.response as AuthenticatorAssertionResponse;
  return {
    challengeId,
    credentialId: toBase64Url(cred.rawId),
    clientDataJson: toBase64Url(res.clientDataJSON),
    authenticatorData: toBase64Url(res.authenticatorData),
    signature: toBase64Url(res.signature),
    userHandle: res.userHandle ? toBase64Url(res.userHandle) : undefined,
  };
}

/** Readable message for WebAuthn DOMExceptions (cancel, timeout, wrong device…). */
export function describePasskeyError(e: unknown): string {
  if (e instanceof DOMException) {
    if (e.name === "NotAllowedError") return "The passkey prompt was cancelled or timed out.";
    if (e.name === "InvalidStateError") return "This device already has a passkey for Directory.";
    if (e.name === "SecurityError") return "Passkeys need HTTPS or http://localhost.";
  }
  return e instanceof Error ? e.message : String(e);
}
