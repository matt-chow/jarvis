// Software WebAuthn authenticator (ES256) for end-to-end tests. Produces the
// same base64url payloads the web app sends from navigator.credentials.
import { createHash, generateKeyPairSync, randomBytes, sign } from "node:crypto";

const b64url = (buf) => Buffer.from(buf).toString("base64url");
const sha256 = (data) => createHash("sha256").update(data).digest();

const UP = 0x01;
const UV = 0x04;
const AT = 0x40;

export class SoftAuthenticator {
  constructor({ rpId, origin }) {
    this.rpId = rpId;
    this.origin = origin;
    const { privateKey, publicKey } = generateKeyPairSync("ec", { namedCurve: "P-256" });
    this.privateKey = privateKey;
    this.spki = publicKey.export({ type: "spki", format: "der" });
    this.credentialId = randomBytes(16);
    this.counter = 0;
    this.userHandle = undefined;
  }

  clientData(type, challenge) {
    return Buffer.from(JSON.stringify({ type, challenge, origin: this.origin, crossOrigin: false }));
  }

  authData(flags, attested) {
    const counter = Buffer.alloc(4);
    counter.writeUInt32BE(this.counter);
    const parts = [sha256(this.rpId), Buffer.from([flags | (attested ? AT : 0)]), counter];
    if (attested) {
      const len = Buffer.alloc(2);
      len.writeUInt16BE(this.credentialId.length);
      parts.push(Buffer.alloc(16), len, this.credentialId, Buffer.from([0xa0]));
    }
    return Buffer.concat(parts);
  }

  /** Respond to `CreationOptions` from /setup/options or /passkeys/options. */
  register(options, name = "Smoke test key") {
    this.userHandle = Buffer.from(options.publicKey.user.id, "base64url");
    return {
      challengeId: options.challengeId,
      credentialId: b64url(this.credentialId),
      clientDataJson: b64url(this.clientData("webauthn.create", options.publicKey.challenge)),
      authenticatorData: b64url(this.authData(UP | UV, true)),
      publicKey: b64url(this.spki),
      algorithm: -7,
      name,
    };
  }

  /** Respond to `RequestOptions` from /login/options. */
  assert(options, { flags = UP | UV } = {}) {
    this.counter += 1;
    const clientData = this.clientData("webauthn.get", options.publicKey.challenge);
    const authData = this.authData(flags, false);
    const signature = sign("sha256", Buffer.concat([authData, sha256(clientData)]), {
      key: this.privateKey,
      dsaEncoding: "der",
    });
    return {
      challengeId: options.challengeId,
      credentialId: b64url(this.credentialId),
      clientDataJson: b64url(clientData),
      authenticatorData: b64url(authData),
      signature: b64url(signature),
      userHandle: this.userHandle ? b64url(this.userHandle) : undefined,
    };
  }
}
