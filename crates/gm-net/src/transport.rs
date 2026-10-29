//! Transports move packets; the protocol never sees sockets. One `Transport` per endpoint:
//! the server's receives from every client (with the sender's endpoint id), a client's talks to
//! endpoint 0 only. The loopback hub is deterministic (seeded loss, fixed delay in ticks) for
//! harnesses; `Udp` is the real one.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::rc::Rc;

pub trait Transport {
    fn send(&mut self, to: usize, packet: Vec<u8>);
    /// Packets that arrived for this endpoint, with the sender's endpoint id, in order.
    fn recv(&mut self) -> Vec<(usize, Vec<u8>)>;
}

struct Hub {
    delay_ticks: u32,
    loss: f32,
    rng: u64,
    now: u32,
    queue: VecDeque<(u32, usize, usize, Vec<u8>)>,
    dropped: u64,
}

impl Hub {
    fn next_f32(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let v = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The shared in-memory network: endpoints 0 (server) and 1.. (clients) with a fixed one-way
/// delay and random loss. `tick()` advances time.
#[derive(Clone)]
pub struct Loopback {
    hub: Rc<RefCell<Hub>>,
}

pub struct LoopbackEndpoint {
    hub: Rc<RefCell<Hub>>,
    id: usize,
}

impl Loopback {
    pub fn new(delay_ticks: u32, loss: f32, seed: u64) -> Self {
        Loopback {
            hub: Rc::new(RefCell::new(Hub {
                delay_ticks,
                loss,
                rng: seed.max(1),
                now: 0,
                queue: VecDeque::new(),
                dropped: 0,
            })),
        }
    }

    pub fn endpoint(&self, id: usize) -> LoopbackEndpoint {
        LoopbackEndpoint {
            hub: self.hub.clone(),
            id,
        }
    }

    /// Advances time by one tick: packets sent `delay_ticks` ago become receivable.
    pub fn tick(&self) {
        self.hub.borrow_mut().now += 1;
    }

    pub fn dropped(&self) -> u64 {
        self.hub.borrow().dropped
    }
}

impl Transport for LoopbackEndpoint {
    fn send(&mut self, to: usize, packet: Vec<u8>) {
        let mut h = self.hub.borrow_mut();
        if h.loss > 0.0 && h.next_f32() < h.loss {
            h.dropped += 1;
            return;
        }
        let due = h.now + h.delay_ticks;
        h.queue.push_back((due, self.id, to, packet));
    }

    fn recv(&mut self) -> Vec<(usize, Vec<u8>)> {
        let mut h = self.hub.borrow_mut();
        let now = h.now;
        let mut out = Vec::new();
        let mut keep = VecDeque::new();
        for (due, from, to, p) in h.queue.drain(..) {
            if to == self.id && due <= now {
                out.push((from, p));
            } else {
                keep.push_back((due, from, to, p));
            }
        }
        h.queue = keep;
        out
    }
}

/// UDP: endpoint ids are indices into the peer table; the server learns peers from incoming
/// packets (endpoint 1.. in order of first contact), a client has the server as endpoint 0.
pub struct Udp {
    socket: UdpSocket,
    peers: Vec<SocketAddr>,
    by_addr: BTreeMap<SocketAddr, usize>,
    pub max_packet: usize,
}

impl Udp {
    /// A server socket bound to `bind` (peers are assigned ids 1.. as they appear).
    pub fn server(bind: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        Ok(Udp {
            socket,
            peers: vec![bind],
            by_addr: BTreeMap::new(),
            max_packet: 1400,
        })
    }

    /// A client socket talking to `server` as endpoint 0.
    pub fn client(server: SocketAddr) -> std::io::Result<Self> {
        let bind: SocketAddr = if server.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        let mut by_addr = BTreeMap::new();
        by_addr.insert(server, 0);
        Ok(Udp {
            socket,
            peers: vec![server],
            by_addr,
            max_packet: 1400,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn peer(&self, id: usize) -> Option<SocketAddr> {
        self.peers.get(id).copied()
    }

    fn endpoint_of(&mut self, addr: SocketAddr) -> usize {
        if let Some(&id) = self.by_addr.get(&addr) {
            return id;
        }
        let id = self.peers.len();
        self.peers.push(addr);
        self.by_addr.insert(addr, id);
        id
    }
}

impl Transport for Udp {
    fn send(&mut self, to: usize, packet: Vec<u8>) {
        if let Some(addr) = self.peers.get(to) {
            let _ = self.socket.send_to(&packet, addr);
        }
    }

    fn recv(&mut self) -> Vec<(usize, Vec<u8>)> {
        let mut out = Vec::new();
        let mut buf = vec![0u8; self.max_packet.max(64)];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let from = self.endpoint_of(addr);
                    out.push((from, buf[..n].to_vec()));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        out
    }
}
