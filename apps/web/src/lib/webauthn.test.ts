import { describe, expect, it } from "vitest";
import { assertionRequest, creationRequest, fromBase64Url, toBase64Url, type CreationOptionsJson } from "./webauthn";

describe("base64url", () => {
  it("round-trips arbitrary bytes without padding", () => {
    const bytes = new Uint8Array([0, 251, 255, 62, 63, 1, 2]);
    const enc = toBase64Url(bytes);
    expect(enc).not.toMatch(/[+/=]/);
    expect(Array.from(fromBase64Url(enc))).toEqual(Array.from(bytes));
  });
});

describe("option conversion", () => {
  const creation: CreationOptionsJson = {
    challengeId: "c1",
    publicKey: {
      challenge: toBase64Url(new Uint8Array([1, 2, 3])),
      rp: { id: "localhost", name: "Directory" },
      user: { id: toBase64Url(new TextEncoder().encode("user-1")), name: "Owner", displayName: "Owner" },
      pubKeyCredParams: [{ type: "public-key", alg: -7 }],
      authenticatorSelection: { residentKey: "required", userVerification: "required" },
      excludeCredentials: [{ type: "public-key", id: toBase64Url(new Uint8Array([9])) }],
      attestation: "none",
      timeout: 300000,
    },
  };

  it("decodes binary fields for navigator.credentials.create", () => {
    const pk = creationRequest(creation).publicKey!;
    expect(Array.from(new Uint8Array(pk.challenge as ArrayBuffer))).toEqual([1, 2, 3]);
    expect(new TextDecoder().decode(pk.user.id as Uint8Array)).toBe("user-1");
    expect(Array.from(pk.excludeCredentials![0].id as Uint8Array)).toEqual([9]);
    expect(pk.authenticatorSelection?.userVerification).toBe("required");
  });

  it("decodes binary fields for navigator.credentials.get", () => {
    const pk = assertionRequest({
      challengeId: "c2",
      publicKey: { challenge: "AQID", rpId: "localhost", allowCredentials: [], userVerification: "required", timeout: 1 },
    }).publicKey!;
    expect(Array.from(pk.challenge as Uint8Array)).toEqual([1, 2, 3]);
    expect(pk.rpId).toBe("localhost");
  });
});
