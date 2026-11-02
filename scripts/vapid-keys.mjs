// Generate a VAPID key pair for Web Push (D-049).
//   node scripts/vapid-keys.mjs
// Prints the private key (store it as the VAPID_PRIVATE_KEY secret, e.g.
// `npx wrangler secret put VAPID_PRIVATE_KEY --env staging`) and the public
// key (for reference; the Worker derives it). Nothing is written to disk.
import { generateKeyPairSync } from 'node:crypto';

const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
const priv = privateKey.export({ format: 'jwk' });
const pub = publicKey.export({ format: 'jwk' });
const raw = Buffer.concat([
  Buffer.from([4]),
  Buffer.from(pub.x, 'base64url'),
  Buffer.from(pub.y, 'base64url'),
]);

console.log(`VAPID_PRIVATE_KEY=${priv.d}`);
console.log(`public key: ${raw.toString('base64url')}`);
