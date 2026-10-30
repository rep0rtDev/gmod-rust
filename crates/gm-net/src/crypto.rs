//! Packet encryption: X25519 ephemeral keys exchanged in `Hello` / `Challenge`, ChaCha20-Poly1305
//! on every packet after that. Outer byte 0 = plaintext (handshake only), 1 = encrypted
//! (8-byte counter, ciphertext, 16-byte tag). Each side keeps sending plaintext until it has
//! opened one encrypted packet from the peer (a lost `Challenge` must still be re-sent readably)
//! and refuses plaintext from then on.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};

pub const PLAIN: u8 = 0;
pub const SEALED: u8 = 1;
/// Counter (8) + tag (16) + outer byte.
pub const OVERHEAD: usize = 25;

pub struct Ephemeral {
    secret: x25519_dalek::StaticSecret,
    pub public: [u8; 32],
}

impl Ephemeral {
    pub fn generate() -> Self {
        let secret = x25519_dalek::StaticSecret::random_from_rng(rand_core::OsRng);
        let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
        Ephemeral { secret, public }
    }

    /// Session key from the peer's public key; `client_public` / `server_public` fix the role
    /// order so both sides derive the same key.
    pub fn session(
        &self,
        peer: &[u8; 32],
        client_public: &[u8; 32],
        server_public: &[u8; 32],
        we_are_client: bool,
    ) -> Session {
        let shared = self
            .secret
            .diffie_hellman(&x25519_dalek::PublicKey::from(*peer));
        let mut h = Sha256::new();
        h.update(b"gmr-key1");
        h.update(shared.as_bytes());
        h.update(client_public);
        h.update(server_public);
        let key: [u8; 32] = h.finalize().into();
        let binding: [u8; 32] = Sha256::digest([b"gmr-bind1".as_slice(), &key].concat()).into();
        Session {
            cipher: ChaCha20Poly1305::new((&key).into()),
            send_ctr: 0,
            send_dir: if we_are_client { 1 } else { 2 },
            binding,
            peer_sealed: false,
        }
    }
}

pub struct Session {
    cipher: ChaCha20Poly1305,
    send_ctr: u64,
    /// Nonce direction word: 1 client -> server, 2 server -> client (one key, no nonce reuse).
    send_dir: u32,
    /// Hash of the session key; the authentication signature covers it (channel binding).
    pub binding: [u8; 32],
    /// The peer has sent at least one packet we could open: plaintext from it is over.
    pub peer_sealed: bool,
}

impl Session {
    pub fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let ctr = self.send_ctr;
        self.send_ctr += 1;
        let nonce = nonce(ctr, self.send_dir);
        let mut out = Vec::with_capacity(plain.len() + OVERHEAD);
        out.push(SEALED);
        out.extend_from_slice(&ctr.to_le_bytes());
        let ct = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: &out,
                },
            )
            .expect("chacha20poly1305 encrypt");
        out.extend_from_slice(&ct);
        out
    }

    /// The plaintext of a `SEALED` packet from the peer, `None` when it does not authenticate.
    pub fn open(&mut self, packet: &[u8]) -> Option<Vec<u8>> {
        if packet.len() < OVERHEAD || packet[0] != SEALED {
            return None;
        }
        let ctr = u64::from_le_bytes(packet[1..9].try_into().unwrap());
        let nonce = nonce(ctr, 3 - self.send_dir);
        let plain = self
            .cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &packet[9..],
                    aad: &packet[..9],
                },
            )
            .ok()?;
        self.peer_sealed = true;
        Some(plain)
    }
}

fn nonce(ctr: u64, dir: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(&ctr.to_le_bytes());
    n[8..].copy_from_slice(&dir.to_le_bytes());
    n
}

pub fn plain(body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 1);
    out.push(PLAIN);
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_derive_the_same_key_and_tampering_is_rejected() {
        let c = Ephemeral::generate();
        let s = Ephemeral::generate();
        let mut cs = c.session(&s.public, &c.public, &s.public, true);
        let mut ss = s.session(&c.public, &c.public, &s.public, false);
        assert_eq!(cs.binding, ss.binding);
        let p = cs.seal(b"hello");
        assert_eq!(p[0], SEALED);
        assert_eq!(ss.open(&p).unwrap(), b"hello");
        assert!(ss.peer_sealed);
        let mut bad = cs.seal(b"hello");
        bad[12] ^= 1;
        assert!(ss.open(&bad).is_none());
        // the other direction uses its own nonces
        let q = ss.seal(b"back");
        assert_eq!(cs.open(&q).unwrap(), b"back");
        // a session of another server cannot read it
        let o = Ephemeral::generate();
        let mut os = o.session(&c.public, &c.public, &o.public, false);
        assert!(os.open(&cs.seal(b"x")).is_none());
    }
}
