//! WebAuthn (passkey) verification primitives.
//!
//! Clients send the credential public key as SPKI DER (from
//! `AuthenticatorAttestationResponse.getPublicKey()`) together with the raw
//! authenticator data, so no CBOR parsing is needed. Attestation is "none":
//! registration is gated by the setup token or an existing session instead.

use crate::encoding::{b64url_decode, constant_time_eq, sha256};
use serde::Deserialize;

pub const ALG_ES256: i64 = -7;
pub const ALG_RS256: i64 = -257;
/// Preference order advertised in `pubKeyCredParams`.
pub const SUPPORTED_ALGORITHMS: [i64; 2] = [ALG_ES256, ALG_RS256];

const FLAG_USER_PRESENT: u8 = 0x01;
const FLAG_USER_VERIFIED: u8 = 0x04;
const FLAG_ATTESTED_DATA: u8 = 0x40;
const MIN_RSA_BYTES: usize = 256; // 2048-bit

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebAuthnError {
    ClientData(&'static str),
    AuthenticatorData(&'static str),
    UnsupportedAlgorithm,
    InvalidPublicKey,
    InvalidSignature,
    /// Signature counter did not increase: possible cloned authenticator.
    SignCount,
}

#[derive(Debug, Clone)]
pub struct RelyingParty {
    /// Effective domain, e.g. `directory.example.workers.dev` or `localhost`.
    pub id: String,
    pub name: String,
    /// Exact origins allowed in client data, e.g. `https://directory.example.workers.dev`.
    pub origins: Vec<String>,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin", default)]
    cross_origin: bool,
}

pub fn verify_client_data(
    json: &[u8],
    expected_type: &str,
    challenge: &[u8],
    rp: &RelyingParty,
) -> Result<(), WebAuthnError> {
    let cd: ClientData =
        serde_json::from_slice(json).map_err(|_| WebAuthnError::ClientData("malformed"))?;
    if cd.kind != expected_type {
        return Err(WebAuthnError::ClientData("type"));
    }
    let got = b64url_decode(&cd.challenge).map_err(|_| WebAuthnError::ClientData("challenge"))?;
    if !constant_time_eq(&got, challenge) {
        return Err(WebAuthnError::ClientData("challenge"));
    }
    if !rp.origins.contains(&cd.origin) {
        return Err(WebAuthnError::ClientData("origin"));
    }
    if cd.cross_origin {
        return Err(WebAuthnError::ClientData("cross-origin"));
    }
    Ok(())
}

#[derive(Debug)]
pub struct AuthenticatorData<'a> {
    pub flags: u8,
    pub sign_count: u32,
    pub attested_credential_id: Option<&'a [u8]>,
}

/// Parse and check RP ID hash plus user presence and user verification.
pub fn parse_authenticator_data<'a>(
    data: &'a [u8],
    rp: &RelyingParty,
) -> Result<AuthenticatorData<'a>, WebAuthnError> {
    if data.len() < 37 {
        return Err(WebAuthnError::AuthenticatorData("too short"));
    }
    if !constant_time_eq(&data[..32], &sha256(rp.id.as_bytes())) {
        return Err(WebAuthnError::AuthenticatorData("rp id"));
    }
    let flags = data[32];
    if flags & FLAG_USER_PRESENT == 0 {
        return Err(WebAuthnError::AuthenticatorData("user not present"));
    }
    if flags & FLAG_USER_VERIFIED == 0 {
        return Err(WebAuthnError::AuthenticatorData("user not verified"));
    }
    let sign_count = u32::from_be_bytes([data[33], data[34], data[35], data[36]]);
    let attested_credential_id = if flags & FLAG_ATTESTED_DATA != 0 {
        // aaguid (16) | credentialIdLength (2) | credentialId | publicKey (CBOR)
        let header = data
            .get(53..55)
            .ok_or(WebAuthnError::AuthenticatorData("attested data"))?;
        let len = u16::from_be_bytes([header[0], header[1]]) as usize;
        Some(
            data.get(55..55 + len)
                .ok_or(WebAuthnError::AuthenticatorData("credential id"))?,
        )
    } else {
        None
    };
    Ok(AuthenticatorData {
        flags,
        sign_count,
        attested_credential_id,
    })
}

/// Check that `spki` is a usable key for `algorithm`.
pub fn validate_public_key(algorithm: i64, spki: &[u8]) -> Result<(), WebAuthnError> {
    match algorithm {
        ALG_ES256 => es256_key(spki).map(|_| ()),
        ALG_RS256 => rs256_key(spki).map(|_| ()),
        _ => Err(WebAuthnError::UnsupportedAlgorithm),
    }
}

fn es256_key(spki: &[u8]) -> Result<p256::ecdsa::VerifyingKey, WebAuthnError> {
    use p256::pkcs8::DecodePublicKey;
    p256::ecdsa::VerifyingKey::from_public_key_der(spki)
        .map_err(|_| WebAuthnError::InvalidPublicKey)
}

fn rs256_key(spki: &[u8]) -> Result<rsa::RsaPublicKey, WebAuthnError> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::traits::PublicKeyParts;
    let key = rsa::RsaPublicKey::from_public_key_der(spki)
        .map_err(|_| WebAuthnError::InvalidPublicKey)?;
    if key.size() < MIN_RSA_BYTES {
        return Err(WebAuthnError::InvalidPublicKey);
    }
    Ok(key)
}

pub fn verify_signature(
    algorithm: i64,
    spki: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<(), WebAuthnError> {
    match algorithm {
        ALG_ES256 => {
            use p256::ecdsa::signature::Verifier;
            let key = es256_key(spki)?;
            let sig = p256::ecdsa::Signature::from_der(signature)
                .map_err(|_| WebAuthnError::InvalidSignature)?;
            // Authenticators may emit high-S signatures; both forms are valid ECDSA.
            let sig = sig.normalize_s().unwrap_or(sig);
            key.verify(message, &sig)
                .map_err(|_| WebAuthnError::InvalidSignature)
        }
        ALG_RS256 => {
            use rsa::signature::Verifier;
            let key = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(rs256_key(spki)?);
            let sig = rsa::pkcs1v15::Signature::try_from(signature)
                .map_err(|_| WebAuthnError::InvalidSignature)?;
            key.verify(message, &sig)
                .map_err(|_| WebAuthnError::InvalidSignature)
        }
        _ => Err(WebAuthnError::UnsupportedAlgorithm),
    }
}

/// Decoded `navigator.credentials.create()` result.
#[derive(Debug, Clone)]
pub struct Registration {
    pub credential_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub public_key: Vec<u8>,
    pub algorithm: i64,
}

/// Returns the initial signature counter.
pub fn verify_registration(
    reg: &Registration,
    challenge: &[u8],
    rp: &RelyingParty,
) -> Result<u32, WebAuthnError> {
    verify_client_data(&reg.client_data_json, "webauthn.create", challenge, rp)?;
    let ad = parse_authenticator_data(&reg.authenticator_data, rp)?;
    match ad.attested_credential_id {
        Some(id) if id == reg.credential_id.as_slice() => {}
        _ => return Err(WebAuthnError::AuthenticatorData("credential id")),
    }
    validate_public_key(reg.algorithm, &reg.public_key)?;
    Ok(ad.sign_count)
}

/// Decoded `navigator.credentials.get()` result.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub credential_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
    pub user_handle: Option<Vec<u8>>,
}

/// Verify an assertion against a stored credential; returns the new counter.
pub fn verify_assertion(
    assertion: &Assertion,
    challenge: &[u8],
    rp: &RelyingParty,
    algorithm: i64,
    public_key: &[u8],
    stored_sign_count: u32,
) -> Result<u32, WebAuthnError> {
    verify_client_data(&assertion.client_data_json, "webauthn.get", challenge, rp)?;
    let ad = parse_authenticator_data(&assertion.authenticator_data, rp)?;
    let mut message = assertion.authenticator_data.clone();
    message.extend_from_slice(&sha256(&assertion.client_data_json));
    verify_signature(algorithm, public_key, &message, &assertion.signature)?;
    // Counters are optional (many passkeys always report 0); when used they must grow.
    if (ad.sign_count != 0 || stored_sign_count != 0) && ad.sign_count <= stored_sign_count {
        return Err(WebAuthnError::SignCount);
    }
    Ok(ad.sign_count)
}

#[cfg(test)]
pub(crate) mod testing {
    //! A software authenticator for tests.
    use super::*;
    use crate::encoding::b64url_encode;
    use p256::ecdsa::{signature::Signer, SigningKey};

    pub fn rp() -> RelyingParty {
        RelyingParty {
            id: "localhost".into(),
            name: "Directory".into(),
            origins: vec!["http://localhost:8787".into()],
        }
    }

    pub struct SoftKey {
        pub key: SigningKey,
        pub credential_id: Vec<u8>,
        pub counter: u32,
    }

    impl SoftKey {
        pub fn new(seed: u8) -> Self {
            let key = SigningKey::from_bytes(&[seed.max(1); 32].into()).unwrap();
            Self {
                key,
                credential_id: vec![seed, 0xc0, 0xde, 0x01],
                counter: 0,
            }
        }

        /// SPKI DER: fixed P-256 header followed by the uncompressed point.
        pub fn spki(&self) -> Vec<u8> {
            const HEADER: &str = "3059301306072a8648ce3d020106082a8648ce3d030107034200";
            let mut der: Vec<u8> = (0..HEADER.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&HEADER[i..i + 2], 16).unwrap())
                .collect();
            der.extend_from_slice(self.key.verifying_key().to_encoded_point(false).as_bytes());
            der
        }

        pub fn client_data(kind: &str, challenge: &[u8], origin: &str) -> Vec<u8> {
            format!(
                r#"{{"type":"{kind}","challenge":"{}","origin":"{origin}","crossOrigin":false}}"#,
                b64url_encode(challenge)
            )
            .into_bytes()
        }

        fn auth_data(&self, rp_id: &str, flags: u8, attested: bool) -> Vec<u8> {
            let mut d = sha256(rp_id.as_bytes()).to_vec();
            d.push(flags | if attested { FLAG_ATTESTED_DATA } else { 0 });
            d.extend_from_slice(&self.counter.to_be_bytes());
            if attested {
                d.extend_from_slice(&[0u8; 16]);
                d.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
                d.extend_from_slice(&self.credential_id);
                d.extend_from_slice(&[0xa0]); // placeholder CBOR key (unused by verifier)
            }
            d
        }

        pub fn register(&self, challenge: &[u8], rp: &RelyingParty) -> Registration {
            Registration {
                credential_id: self.credential_id.clone(),
                client_data_json: Self::client_data("webauthn.create", challenge, &rp.origins[0]),
                authenticator_data: self.auth_data(
                    &rp.id,
                    FLAG_USER_PRESENT | FLAG_USER_VERIFIED,
                    true,
                ),
                public_key: self.spki(),
                algorithm: ALG_ES256,
            }
        }

        pub fn assert_with(
            &self,
            challenge: &[u8],
            rp: &RelyingParty,
            flags: u8,
            user_handle: Option<Vec<u8>>,
        ) -> Assertion {
            let client_data_json = Self::client_data("webauthn.get", challenge, &rp.origins[0]);
            let authenticator_data = self.auth_data(&rp.id, flags, false);
            let mut msg = authenticator_data.clone();
            msg.extend_from_slice(&sha256(&client_data_json));
            let sig: p256::ecdsa::Signature = self.key.sign(&msg);
            Assertion {
                credential_id: self.credential_id.clone(),
                client_data_json,
                authenticator_data,
                signature: sig.to_der().as_bytes().to_vec(),
                user_handle,
            }
        }

        pub fn assert(&self, challenge: &[u8], rp: &RelyingParty) -> Assertion {
            self.assert_with(challenge, rp, FLAG_USER_PRESENT | FLAG_USER_VERIFIED, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    const CHALLENGE: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn registration_and_assertion_round_trip() {
        let rp = rp();
        let mut key = SoftKey::new(7);
        let reg = key.register(CHALLENGE, &rp);
        assert_eq!(verify_registration(&reg, CHALLENGE, &rp), Ok(0));

        let a = key.assert(CHALLENGE, &rp);
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &reg.public_key, 0),
            Ok(0)
        );

        key.counter = 5;
        let a = key.assert(CHALLENGE, &rp);
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &reg.public_key, 4),
            Ok(5)
        );
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &reg.public_key, 5),
            Err(WebAuthnError::SignCount)
        );
    }

    #[test]
    fn rejects_wrong_challenge_origin_type_and_rp() {
        let rp = rp();
        let key = SoftKey::new(3);
        let reg = key.register(CHALLENGE, &rp);
        assert_eq!(
            verify_registration(&reg, b"other", &rp),
            Err(WebAuthnError::ClientData("challenge"))
        );

        let mut evil = rp.clone();
        evil.origins = vec!["https://evil.example".into()];
        let reg = key.register(CHALLENGE, &evil);
        assert_eq!(
            verify_registration(&reg, CHALLENGE, &rp),
            Err(WebAuthnError::ClientData("origin"))
        );

        let a = key.assert(CHALLENGE, &rp);
        let spki = key.spki();
        let as_create = Assertion {
            client_data_json: SoftKey::client_data("webauthn.create", CHALLENGE, &rp.origins[0]),
            ..a.clone()
        };
        assert_eq!(
            verify_assertion(&as_create, CHALLENGE, &rp, ALG_ES256, &spki, 0),
            Err(WebAuthnError::ClientData("type"))
        );

        let mut other_rp = rp.clone();
        other_rp.id = "evil.example".into();
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &other_rp, ALG_ES256, &spki, 0),
            Err(WebAuthnError::AuthenticatorData("rp id"))
        );
    }

    #[test]
    fn requires_user_verification() {
        let rp = rp();
        let key = SoftKey::new(4);
        let a = key.assert_with(CHALLENGE, &rp, FLAG_USER_PRESENT, None);
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &key.spki(), 0),
            Err(WebAuthnError::AuthenticatorData("user not verified"))
        );
    }

    #[test]
    fn rejects_tampered_signature_and_foreign_key() {
        let rp = rp();
        let key = SoftKey::new(5);
        let mut a = key.assert(CHALLENGE, &rp);
        a.authenticator_data[36] ^= 1; // tamper with counter after signing
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &key.spki(), 0),
            Err(WebAuthnError::InvalidSignature)
        );
        let a = key.assert(CHALLENGE, &rp);
        let other = SoftKey::new(6).spki();
        assert_eq!(
            verify_assertion(&a, CHALLENGE, &rp, ALG_ES256, &other, 0),
            Err(WebAuthnError::InvalidSignature)
        );
    }

    #[test]
    fn registration_checks_credential_id_and_key() {
        let rp = rp();
        let key = SoftKey::new(8);
        let mut reg = key.register(CHALLENGE, &rp);
        reg.credential_id = vec![1, 2, 3];
        assert_eq!(
            verify_registration(&reg, CHALLENGE, &rp),
            Err(WebAuthnError::AuthenticatorData("credential id"))
        );
        let mut reg = key.register(CHALLENGE, &rp);
        reg.public_key = vec![0; 10];
        assert_eq!(
            verify_registration(&reg, CHALLENGE, &rp),
            Err(WebAuthnError::InvalidPublicKey)
        );
        let mut reg = key.register(CHALLENGE, &rp);
        reg.algorithm = -8;
        assert_eq!(
            verify_registration(&reg, CHALLENGE, &rp),
            Err(WebAuthnError::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn verifies_rs256() {
        use rsa::pkcs1v15::SigningKey;
        use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
        use rsa::signature::{SignatureEncoding, Signer};
        let private =
            rsa::RsaPrivateKey::from_pkcs8_pem(include_str!("testdata/rsa2048.pem")).unwrap();
        let spki = private
            .to_public_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec();
        let msg = b"authenticator data || client data hash";
        let sig = SigningKey::<sha2::Sha256>::new(private).sign(msg).to_vec();
        assert_eq!(verify_signature(ALG_RS256, &spki, msg, &sig), Ok(()));
        assert_eq!(
            verify_signature(ALG_RS256, &spki, b"different", &sig),
            Err(WebAuthnError::InvalidSignature)
        );
    }
}
