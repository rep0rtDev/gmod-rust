//! Packet framing with Source-style acks: every packet carries its sequence, the newest sequence
//! seen from the other side and a 32-bit history of the ones before it. Reliable messages stay in
//! the resend queue until a packet that carried them is acked; the receiver drops duplicates by
//! the message's own id.

use crate::wire::{Reader, Truncated, Writer};
use std::collections::{BTreeMap, BTreeSet};

pub const MAGIC: u16 = 0x474D; // "GM"

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub seq: u16,
    pub ack: u16,
    pub ack_bits: u32,
}

impl Header {
    pub const SIZE: usize = 10;

    pub fn write(&self, w: &mut Writer) {
        w.u16(MAGIC);
        w.u16(self.seq);
        w.u16(self.ack);
        w.u32(self.ack_bits);
    }

    pub fn read(r: &mut Reader) -> Result<Self, Truncated> {
        if r.u16()? != MAGIC {
            return Err(Truncated);
        }
        Ok(Header {
            seq: r.u16()?,
            ack: r.u16()?,
            ack_bits: r.u32()?,
        })
    }
}

fn seq_newer(a: u16, b: u16) -> bool {
    ((a > b) && (a - b <= 32768)) || ((a < b) && (b - a > 32768))
}

/// The largest packet a channel builds (below the usual path MTU; `Udp` receives up to 1400).
pub const MAX_PACKET: usize = 1200;
/// Reliable messages longer than this go as fragments of this size, reassembled in order.
pub const FRAGMENT: usize = 1024;
/// Packets `build_burst` makes per call at most (the ack history covers 33 sequences).
pub const MAX_BURST: usize = 8;

/// Message flags on the wire.
const RELIABLE: u8 = 1;
/// A reliable fragment that the next reliable message continues.
const MORE: u8 = 2;

struct Pending {
    flags: u8,
    bytes: Vec<u8>,
    /// packets that carried it
    carried: Vec<u16>,
}

/// One direction of a connection: outgoing sequence numbers, the ack state for the other side and
/// the reliable resend queue. Messages are opaque bytes with a reliability flag and an id; the
/// reliable ones are delivered in id order (a message after a lost one waits), long ones travel
/// as fragments and every packet stays under `MAX_PACKET` unless a single unreliable message is
/// larger by itself.
#[derive(Default)]
pub struct Channel {
    pub local_seq: u16,
    pub remote_seq: Option<u16>,
    received: BTreeSet<u16>,
    /// reliable messages in flight by id
    pending: BTreeMap<u64, Pending>,
    next_reliable_id: u64,
    /// the reliable id delivered next; later ones wait here
    next_recv_id: u64,
    early: BTreeMap<u64, (u8, Vec<u8>)>,
    /// fragments of the reliable message being reassembled
    partial: Vec<u8>,
    pub stats: Stats,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub reliable_resends: u64,
}

impl Channel {
    /// Queues a reliable message; it goes out with every packet (as far as the packet budget
    /// allows, oldest first) until acked. Returns the id of its last fragment.
    pub fn send_reliable(&mut self, bytes: Vec<u8>) -> u64 {
        let mut chunks: Vec<&[u8]> = bytes.chunks(FRAGMENT).collect();
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        let last = chunks.len() - 1;
        let mut id = 0;
        for (i, c) in chunks.iter().enumerate() {
            id = self.next_reliable_id;
            self.next_reliable_id += 1;
            self.pending.insert(
                id,
                Pending {
                    flags: if i == last { RELIABLE } else { RELIABLE | MORE },
                    bytes: c.to_vec(),
                    carried: Vec::new(),
                },
            );
        }
        id
    }

    fn ack_fields(&self) -> (u16, u32) {
        let Some(newest) = self.remote_seq else {
            return (0, 0);
        };
        let mut bits = 0u32;
        for i in 0..32u16 {
            let s = newest.wrapping_sub(i + 1);
            if self.received.contains(&s) {
                bits |= 1 << i;
            }
        }
        (newest, bits)
    }

    /// Builds a packet: header, reliable messages (pending, oldest first, as many as fit next to
    /// the unreliable payloads, at least one), then the unreliable payloads.
    /// Each message is `[flags u8][id varint if reliable][bytes]`.
    pub fn build(&mut self, unreliable: &[Vec<u8>]) -> Vec<u8> {
        self.build_skipping(unreliable, &mut BTreeSet::new())
    }

    /// `build` and then, while reliable messages are left that no packet of this call carried,
    /// more packets of only those (up to `MAX_BURST` packets): a large transfer moves several
    /// fragments per tick instead of one.
    pub fn build_burst(&mut self, unreliable: &[Vec<u8>]) -> Vec<Vec<u8>> {
        let mut in_burst = BTreeSet::new();
        let mut out = vec![self.build_skipping(unreliable, &mut in_burst)];
        while out.len() < MAX_BURST && self.pending.keys().any(|id| !in_burst.contains(id)) {
            out.push(self.build_skipping(&[], &mut in_burst));
        }
        out
    }

    fn build_skipping(&mut self, unreliable: &[Vec<u8>], skip: &mut BTreeSet<u64>) -> Vec<u8> {
        let seq = self.local_seq;
        self.local_seq = self.local_seq.wrapping_add(1);
        let (ack, ack_bits) = self.ack_fields();
        let mut w = Writer::default();
        Header { seq, ack, ack_bits }.write(&mut w);
        let unreliable_len: usize = unreliable.iter().map(|m| m.len() + 10).sum();
        let mut first = true;
        for (id, p) in self.pending.iter_mut() {
            if skip.contains(id) {
                continue;
            }
            let len = p.bytes.len() + 12;
            if !first && w.buf.len() + len + unreliable_len > MAX_PACKET {
                break;
            }
            first = false;
            if !p.carried.is_empty() {
                self.stats.reliable_resends += 1;
            }
            p.carried.push(seq);
            skip.insert(*id);
            w.u8(p.flags);
            w.varint(*id);
            w.bytes(&p.bytes);
        }
        for m in unreliable {
            w.u8(0);
            w.bytes(m);
        }
        self.stats.packets_sent += 1;
        self.stats.bytes_sent += w.buf.len() as u64;
        w.buf
    }

    /// Parses a packet, updates acks and returns the new messages (duplicates of reliable
    /// messages already delivered are dropped) in order.
    pub fn receive(&mut self, packet: &[u8]) -> Result<Vec<Vec<u8>>, Truncated> {
        let mut r = Reader::new(packet);
        let h = Header::read(&mut r)?;
        self.stats.packets_received += 1;
        self.stats.bytes_received += packet.len() as u64;
        match self.remote_seq {
            None => self.remote_seq = Some(h.seq),
            Some(cur) if seq_newer(h.seq, cur) => self.remote_seq = Some(h.seq),
            _ => {}
        }
        self.received.insert(h.seq);
        if self.received.len() > 256 {
            let newest = self.remote_seq.unwrap_or(0);
            self.received
                .retain(|s| !seq_newer(newest.wrapping_sub(64), *s) || *s == newest);
        }
        // what the other side has seen of ours; (0, 0) is "nothing yet", so our packet 0 is only
        // acked by the bits of a later packet (one spare resend at most, duplicates drop by id)
        let mut acked: Vec<u16> = Vec::new();
        if h.ack != 0 || h.ack_bits != 0 {
            acked.push(h.ack);
            for i in 0..32u16 {
                if h.ack_bits & (1 << i) != 0 {
                    acked.push(h.ack.wrapping_sub(i + 1));
                }
            }
        }
        self.pending
            .retain(|_, p| !p.carried.iter().any(|s| acked.contains(s)));
        let mut out = Vec::new();
        while r.remaining() > 0 {
            let flags = r.u8()?;
            if flags & RELIABLE != 0 {
                let id = r.varint()?;
                let bytes = r.bytes()?;
                if id >= self.next_recv_id && !self.early.contains_key(&id) {
                    self.early.insert(id, (flags, bytes.to_vec()));
                }
                while let Some((flags, bytes)) = self.early.remove(&self.next_recv_id) {
                    self.next_recv_id += 1;
                    if flags & MORE != 0 {
                        self.partial.extend_from_slice(&bytes);
                    } else if self.partial.is_empty() {
                        out.push(bytes);
                    } else {
                        self.partial.extend_from_slice(&bytes);
                        out.push(std::mem::take(&mut self.partial));
                    }
                }
            } else {
                out.push(r.bytes()?.to_vec());
            }
        }
        Ok(out)
    }

    /// Reliable messages (fragments) queued and not yet acked.
    pub fn pending_reliable(&self) -> usize {
        self.pending.len()
    }
}
