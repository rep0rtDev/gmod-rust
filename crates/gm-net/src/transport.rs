//! Transports move packets; the protocol never sees sockets. The loopback is deterministic
//! (seeded loss, fixed delay in ticks) for harnesses; a UDP transport comes with the launcher.

use std::collections::VecDeque;

pub trait Transport {
    fn send(&mut self, to: usize, packet: Vec<u8>);
    /// Packets that arrived for `at`, in order.
    fn recv(&mut self, at: usize) -> Vec<Vec<u8>>;
}

/// Two endpoints (0 = server, 1.. = clients) with a fixed one-way delay and random loss.
pub struct Loopback {
    delay_ticks: u32,
    loss: f32,
    rng: u64,
    now: u32,
    queue: VecDeque<(u32, usize, Vec<u8>)>,
    pub dropped: u64,
}

impl Loopback {
    pub fn new(delay_ticks: u32, loss: f32, seed: u64) -> Self {
        Loopback {
            delay_ticks,
            loss,
            rng: seed.max(1),
            now: 0,
            queue: VecDeque::new(),
            dropped: 0,
        }
    }

    fn next_f32(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let v = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Advances time by one tick: packets sent `delay_ticks` ago become receivable.
    pub fn tick(&mut self) {
        self.now += 1;
    }
}

impl Transport for Loopback {
    fn send(&mut self, to: usize, packet: Vec<u8>) {
        if self.loss > 0.0 && self.next_f32() < self.loss {
            self.dropped += 1;
            return;
        }
        self.queue
            .push_back((self.now + self.delay_ticks, to, packet));
    }

    fn recv(&mut self, at: usize) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut keep = VecDeque::new();
        for (due, to, p) in self.queue.drain(..) {
            if to == at && due <= self.now {
                out.push(p);
            } else {
                keep.push_back((due, to, p));
            }
        }
        self.queue = keep;
        out
    }
}
