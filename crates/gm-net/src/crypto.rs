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
            recv_max: 0,
            recv_seen: 0,
            recv_any: false,
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
    /// Highest counter opened so far and a bitmap of the `REPLAY_WINDOW` counters below it,
    /// bit i = `recv_max - i` seen; a counter already seen or below the window is a replay.
    recv_max: u64,
    recv_seen: u128,
    recv_any: bool,
}

/// Counters this far below the highest opened one are still accepted when unseen
/// (reordering on the wire); older ones are dropped as replays.
pub const REPLAY_WINDOW: u64 = 128;

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
        if !self.accept_counter(ctr) {
            return None;
        }
        self.peer_sealed = true;
        Some(plain)
    }

    /// Records `ctr` as seen; false when it was already seen or is below the window.
    fn accept_counter(&mut self, ctr: u64) -> bool {
        if !self.recv_any {
            self.recv_any = true;
            self.recv_max = ctr;
            self.recv_seen = 1;
            return true;
        }
        if ctr > self.recv_max {
            let shift = ctr - self.recv_max;
            self.recv_seen = if shift >= REPLAY_WINDOW {
                0
            } else {
                self.recv_seen << shift
            };
            self.recv_seen |= 1;
            self.recv_max = ctr;
            return true;
        }
        let back = self.recv_max - ctr;
        if back >= REPLAY_WINDOW {
            return false;
        }
        let bit = 1u128 << back;
        if self.recv_seen & bit != 0 {
            return false;
        }
        self.recv_seen |= bit;
        true
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

    #[test]
    fn replayed_and_too_old_packets_are_dropped_reordered_ones_pass() {
        let c = Ephemeral::generate();
        let s = Ephemeral::generate();
        let mut cs = c.session(&s.public, &c.public, &s.public, true);
        let mut ss = s.session(&c.public, &c.public, &s.public, false);
        let p0 = cs.seal(b"0");
        let p1 = cs.seal(b"1");
        let p2 = cs.seal(b"2");
        assert_eq!(ss.open(&p2).unwrap(), b"2");
        // older but unseen counters inside the window arrive out of order and pass
        assert_eq!(ss.open(&p0).unwrap(), b"0");
        assert_eq!(ss.open(&p1).unwrap(), b"1");
        // the same packets again are replays
        assert!(ss.open(&p2).is_none());
        assert!(ss.open(&p0).is_none());
        // a counter further back than the window is dropped even when never seen
        let mut late = Vec::new();
        for i in 0..REPLAY_WINDOW + 2 {
            let p = cs.seal(&[i as u8]);
            if i == 3 || i == 4 {
                late.push(p);
            } else {
                assert!(ss.open(&p).is_some());
            }
        }
        // counter 3+3=6 is REPLAY_WINDOW+4 - 6 = 126 back: inside; 7 is 125 back: inside
        assert!(ss.open(&late[0]).is_some());
        assert!(ss.open(&late[1]).is_some());
        let far = cs.seal(b"far");
        for _ in 0..REPLAY_WINDOW {
            assert!(ss.open(&cs.seal(b"n")).is_some());
        }
        assert!(ss.open(&far).is_none(), "below the window");
    }
}
