//! minisign's file format (jedisct1/minisign): ed25519 over the BLAKE2b-512 of the file
//! ("ED", prehashed), keys and signatures as base64 lines with a key id.
//!
//! Public key file: an untrusted comment line, then base64 of `"Ed" || key_id(8) || pk(32)`.
//! Signature file: an untrusted comment line, base64 of `"ED" || key_id(8) || sig(64)`, a
//! `trusted comment: ...` line, and base64 of the global signature over `sig || trusted comment`.

use anyhow::{bail, Context, Result};
use base64::Engine;
use blake2::digest::consts::U64;
use blake2::{Blake2b, Digest};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

pub struct PublicKey {
    pub key_id: [u8; 8],
    pub key: VerifyingKey,
}

impl PublicKey {
    /// From the text of a `.pub` file or the bare base64 line.
    pub fn parse(text: &str) -> Result<Self> {
        let line = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))
            .context("no key line")?;
        let raw = B64.decode(line).context("public key base64")?;
        if raw.len() != 42 || &raw[..2] != b"Ed" {
            bail!("not a minisign Ed public key");
        }
        let mut key_id = [0; 8];
        key_id.copy_from_slice(&raw[2..10]);
        let key =
            VerifyingKey::from_bytes(raw[10..42].try_into().unwrap()).context("public key")?;
        Ok(Self { key_id, key })
    }

    pub fn to_text(&self, comment: &str) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend_from_slice(&self.key_id);
        raw.extend_from_slice(self.key.as_bytes());
        format!("untrusted comment: {comment}\n{}\n", B64.encode(raw))
    }
}

pub struct SecretKey {
    pub key_id: [u8; 8],
    pub key: SigningKey,
}

impl SecretKey {
    /// A fresh key; the id is random like minisign's.
    pub fn generate() -> Self {
        use rand_core::RngCore;
        let mut rng = rand_core::OsRng;
        let mut key_id = [0; 8];
        rng.fill_bytes(&mut key_id);
        Self {
            key_id,
            key: SigningKey::generate(&mut rng),
        }
    }

    /// Our own storage: `<key id hex><seed hex>` (minisign's encrypted secret key format
    /// is not read).
    pub fn from_hex(hex: &str) -> Result<Self> {
        let raw = decode_hex(hex.trim()).context("secret key hex")?;
        if raw.len() != 40 {
            bail!("a secret key is 40 hex bytes (8 id + 32 seed)");
        }
        let mut key_id = [0; 8];
        key_id.copy_from_slice(&raw[..8]);
        Ok(Self {
            key_id,
            key: SigningKey::from_bytes(raw[8..40].try_into().unwrap()),
        })
    }

    pub fn to_hex(&self) -> String {
        let mut raw = self.key_id.to_vec();
        raw.extend_from_slice(&self.key.to_bytes());
        encode_hex(&raw)
    }

    pub fn public(&self) -> PublicKey {
        PublicKey {
            key_id: self.key_id,
            key: self.key.verifying_key(),
        }
    }

    /// The `.minisig` text for `data`.
    pub fn sign(&self, data: &[u8], trusted_comment: &str, untrusted_comment: &str) -> String {
        let sig = self.key.sign(&prehash(data));
        let mut raw = b"ED".to_vec();
        raw.extend_from_slice(&self.key_id);
        raw.extend_from_slice(&sig.to_bytes());
        let mut global = sig.to_bytes().to_vec();
        global.extend_from_slice(trusted_comment.as_bytes());
        let global_sig = self.key.sign(&global);
        format!(
            "untrusted comment: {untrusted_comment}\n{}\ntrusted comment: {trusted_comment}\n{}\n",
            B64.encode(raw),
            B64.encode(global_sig.to_bytes())
        )
    }
}

/// Checks a `.minisig` against `data`; returns the trusted comment.
pub fn verify(public: &PublicKey, signature: &str, data: &[u8]) -> Result<String> {
    let lines: Vec<&str> = signature.lines().map(str::trim).collect();
    if lines.len() < 4 {
        bail!("a signature file has four lines");
    }
    let raw = B64.decode(lines[1]).context("signature base64")?;
    if raw.len() != 74 {
        bail!("bad signature length");
    }
    let prehashed = match &raw[..2] {
        b"ED" => true,
        b"Ed" => false,
        _ => bail!("unknown signature algorithm"),
    };
    if raw[2..10] != public.key_id {
        bail!("signature key id does not match the public key");
    }
    let sig = Signature::from_bytes(raw[10..74].try_into().unwrap());
    let trusted = lines[2]
        .strip_prefix("trusted comment:")
        .context("trusted comment line")?
        .trim_start()
        .to_string();
    let global = B64.decode(lines[3]).context("global signature base64")?;
    let global_sig = Signature::from_slice(&global).context("global signature")?;
    let mut global_msg = sig.to_bytes().to_vec();
    global_msg.extend_from_slice(trusted.as_bytes());
    public
        .key
        .verify(&global_msg, &global_sig)
        .context("trusted comment signature")?;
    if prehashed {
        public.key.verify(&prehash(data), &sig)
    } else {
        public.key.verify(data, &sig)
    }
    .context("file signature")?;
    Ok(trusted)
}

fn prehash(data: &[u8]) -> [u8; 64] {
    let mut h = Blake2b::<U64>::new();
    h.update(data);
    h.finalize().into()
}

pub fn encode_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        bail!("odd hex length");
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).context("hex digit"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip_and_tamper() {
        let sk = SecretKey::generate();
        let pk = PublicKey::parse(&sk.public().to_text("minisign public key")).unwrap();
        let sig = sk.sign(b"release bytes", "timestamp:1 file:SHA256SUMS", "signature");
        assert_eq!(
            verify(&pk, &sig, b"release bytes").unwrap(),
            "timestamp:1 file:SHA256SUMS"
        );
        assert!(verify(&pk, &sig, b"release byte").is_err());
        let other = SecretKey::generate().public();
        assert!(verify(&other, &sig, b"release bytes").is_err());
        let mut forged = sig.replace("timestamp:1", "timestamp:2");
        assert!(verify(&pk, &forged, b"release bytes").is_err());
        forged = SecretKey::from_hex(&sk.to_hex())
            .unwrap()
            .sign(b"release bytes", "x", "y");
        assert!(verify(&pk, &forged, b"release bytes").is_ok());
    }
}
