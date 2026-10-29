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

/// One direction of a connection: outgoing sequence numbers, the ack state for the other side and
/// the reliable resend queue. Messages are opaque bytes with a reliability flag and an id.
#[derive(Default)]
pub struct Channel {
    pub local_seq: u16,
    pub remote_seq: Option<u16>,
    received: BTreeSet<u16>,
    /// reliable messages in flight: id -> (bytes, packets that carried it)
    pending: BTreeMap<u64, (Vec<u8>, Vec<u16>)>,
    next_reliable_id: u64,
    /// reliable ids seen from the other side (dedupe)
    seen_reliable: BTreeSet<u64>,
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
    /// Queues a reliable message; it goes out with every packet until acked.
    pub fn send_reliable(&mut self, bytes: Vec<u8>) -> u64 {
        let id = self.next_reliable_id;
        self.next_reliable_id += 1;
        self.pending.insert(id, (bytes, Vec::new()));
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

    /// Builds a packet: header, reliable messages (pending), then the unreliable payloads.
    /// Each message is `[flags u8][id varint if reliable][bytes]`.
    pub fn build(&mut self, unreliable: &[Vec<u8>]) -> Vec<u8> {
        let seq = self.local_seq;
        self.local_seq = self.local_seq.wrapping_add(1);
        let (ack, ack_bits) = self.ack_fields();
        let mut w = Writer::default();
        Header { seq, ack, ack_bits }.write(&mut w);
        for (id, (bytes, carried)) in self.pending.iter_mut() {
            if !carried.is_empty() {
                self.stats.reliable_resends += 1;
            }
            carried.push(seq);
            w.u8(1);
            w.varint(*id);
            w.bytes(bytes);
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
        // what the other side has seen of ours
        let mut acked: Vec<u16> = Vec::new();
        if self.local_seq != 0 || h.ack != 0 || h.ack_bits != 0 {
            acked.push(h.ack);
            for i in 0..32u16 {
                if h.ack_bits & (1 << i) != 0 {
                    acked.push(h.ack.wrapping_sub(i + 1));
                }
            }
        }
        self.pending
            .retain(|_, (_, carried)| !carried.iter().any(|s| acked.contains(s)));
        let mut out = Vec::new();
        while r.remaining() > 0 {
            let flags = r.u8()?;
            if flags & 1 != 0 {
                let id = r.varint()?;
                let bytes = r.bytes()?;
                if self.seen_reliable.insert(id) {
                    out.push(bytes.to_vec());
                }
            } else {
                out.push(r.bytes()?.to_vec());
            }
        }
        Ok(out)
    }

    pub fn pending_reliable(&self) -> usize {
        self.pending.len()
    }
}
