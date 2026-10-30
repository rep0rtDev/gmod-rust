//! Own network protocol over the `gm-server` tick: packets with sequence/ack bitfields,
//! reliable messages resent until acked, 66 Hz usercmd batches (last 8 per packet), delta
//! snapshots (33 Hz default) against the newest snapshot the client acked. No compatibility with
//! the original game's protocol. Transports are pluggable; the loopback is deterministic.

pub mod channel;
pub mod client;
pub mod master;
pub mod msg;
pub mod server;
pub mod transport;
pub mod wire;

pub mod crypto;
pub use client::{ClientNet, ClientWorld, Interpolator, Prediction};
pub use master::{MasterMsg, MasterNet, ServerEntry, ServerInfo};
pub use msg::EntityState;
pub use msg::{auth_message, AddonInfo, ClientMsg, Command, NetCmd, ServerMsg, AUTH_PREFIX};
pub use server::ServerNet;
pub use transport::{Loopback, LoopbackEndpoint, Transport, Udp};

#[cfg(test)]
mod tests {
    use super::channel::Channel;
    use super::msg::*;
    use super::wire::{Reader, Writer};
    use glam::Vec3;

    #[test]
    fn messages_round_trip() {
        let msgs = vec![
            ClientMsg::Hello {
                name: "bot".into(),
                protocol: PROTOCOL,
                pubkey: [7; 32],
            },
            ClientMsg::Cmds {
                cmds: vec![NetCmd {
                    tick: 7,
                    buttons: 1 | 32,
                    view_angles: Vec3::new(10.0, -90.0, 0.0),
                    forward_move: 400.0,
                    side_move: 0.0,
                    up_move: 0.0,
                    mouse_x: 3.0,
                    mouse_y: -2.0,
                    weapon: 1,
                    seen_tick: 0,
                }],
            },
            ClientMsg::Command {
                seq: 3,
                cmd: Command::ToolClick {
                    tool: "weld".into(),
                    right: true,
                    reload: false,
                },
            },
            ClientMsg::AckSnapshot { tick: 99 },
        ];
        for m in msgs {
            let mut w = Writer::default();
            m.write(&mut w);
            assert_eq!(ClientMsg::read(&mut Reader::new(&w.buf)).unwrap(), m);
        }
        let snap = ServerMsg::Snapshot {
            tick: 10,
            baseline: 8,
            last_cmd_tick: 9,
            players: vec![PlayerSnap {
                id: 1,
                origin: Vec3::new(1.0, 2.0, 3.0),
                velocity: Vec3::ZERO,
                view_angles: Vec3::Y,
                ducked: true,
                held: Some(2),
                alive: true,
                health: 100,
                armor: 0,
                weapons: 0x1fff,
                active: 5,
            }],
            entities: vec![EntityState {
                id: 2,
                model: String::new(),
                model_id: 3,
                pos: Vec3::new(900.0, -500.0, -123.75),
                angles: Vec3::new(0.0, 179.0, 0.0),
                vel: Vec3::new(0.0, 0.0, -9.0),
                angvel: Vec3::ZERO,
                frozen: false,
                owner: 1,
            }],
            removed: vec![1],
            own: OwnSandbox {
                count: 1,
                constraints: 0,
                undos: Vec::new(),
                max_props: 200,
            },
            local: Some(LocalPlayer {
                state: gm_move::PlayerState {
                    origin: Vec3::new(1.0, 2.0, 3.0),
                    ducked: true,
                    duck_timer: 0.4,
                    ..Default::default()
                },
            }),
        };
        let mut w = Writer::default();
        snap.write(&mut w);
        assert_eq!(ServerMsg::read(&mut Reader::new(&w.buf)).unwrap(), snap);
        let undos = ServerMsg::Undos {
            seq: 3,
            entries: vec![UndoEntry {
                id: 1,
                name: "Prop".into(),
            }],
        };
        let mut w = Writer::default();
        undos.write(&mut w);
        assert_eq!(ServerMsg::read(&mut Reader::new(&w.buf)).unwrap(), undos);
        let models = ServerMsg::Models {
            from: 1,
            names: vec!["models/props_junk/wood_crate001a.mdl".into()],
        };
        let mut w = Writer::default();
        models.write(&mut w);
        assert_eq!(ServerMsg::read(&mut Reader::new(&w.buf)).unwrap(), models);
    }

    #[test]
    fn usercmd_batch_is_delta_coded() {
        let mut cmds = Vec::new();
        for i in 0..8u32 {
            cmds.push(NetCmd {
                tick: 100 + i,
                buttons: 8,
                view_angles: Vec3::new(5.0, 90.0 + i as f32 * 0.5, 0.0),
                forward_move: 400.0,
                ..Default::default()
            });
        }
        let m = ClientMsg::Cmds { cmds: cmds.clone() };
        let mut w = Writer::default();
        m.write(&mut w);
        // 1 type + 1 count + 4 first tick + first cmd (1 + 1 + 12 + 4) + 7 * (1 + 12)
        assert_eq!(w.buf.len(), 6 + 18 + 7 * 13, "{} bytes", w.buf.len());
        assert_eq!(ClientMsg::read(&mut Reader::new(&w.buf)).unwrap(), m);
    }

    #[test]
    fn reliable_messages_survive_loss_and_are_not_duplicated() {
        let (mut a, mut b) = (Channel::default(), Channel::default());
        a.send_reliable(b"hello".to_vec());
        let lost = a.build(&[]);
        drop(lost);
        let p2 = a.build(&[b"u1".to_vec()]);
        let got = b.receive(&p2).unwrap();
        assert_eq!(got, vec![b"hello".to_vec(), b"u1".to_vec()]);
        // b answers, acking p2: a drops the reliable from its queue
        let back = b.build(&[]);
        a.receive(&back).unwrap();
        assert_eq!(a.pending_reliable(), 0);
        // a duplicate (resent before the ack arrived) is dropped by id
        let got = b.receive(&p2).unwrap();
        assert_eq!(got, vec![b"u1".to_vec()]);
    }

    #[test]
    fn master_lists_heartbeating_servers_and_expires_silent_ones() {
        use crate::master::*;
        use crate::{Loopback, Transport};
        let net = Loopback::new(0, 0.0, 1);
        let (mut m_ep, mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
        let mut master = MasterNet::default();
        let info = ServerInfo {
            name: "box".into(),
            map: "gm_construct".into(),
            players: 3,
            max_players: 16,
            port: 27015,
            version: "0.1".into(),
            addons: vec!["hud".into()],
        };
        s_ep.send(0, MasterMsg::Heartbeat(info.clone()).encode());
        net.tick();
        master.receive(&mut m_ep, 100);
        assert_eq!(master.servers.len(), 1);
        c_ep.send(0, MasterMsg::List { offset: 0 }.encode());
        net.tick();
        master.receive(&mut m_ep, 105);
        net.tick();
        let reply = c_ep.recv();
        assert_eq!(reply.len(), 1);
        let MasterMsg::Servers {
            total,
            offset,
            entries,
        } = MasterMsg::read(&reply[0].1).unwrap()
        else {
            panic!("not a list");
        };
        assert_eq!((total, offset), (1, 0));
        assert_eq!(
            entries,
            vec![ServerEntry {
                addr: "loop1:27015".into(),
                info: info.clone(),
                age_s: 5
            }]
        );
        // many servers: pages stay within the budget and cover everything
        for i in 0..200u16 {
            let mut inf = info.clone();
            inf.port = 30000 + i;
            inf.name = format!("server number {i} with a long name");
            master
                .servers
                .insert(format!("10.0.0.{}:{}", i % 7, inf.port), (inf, 100));
        }
        let (mut seen, mut off) = (0usize, 0u32);
        loop {
            let page = master.page(off, 100);
            let bytes = page.encode();
            assert!(bytes.len() <= LIST_BUDGET, "{} bytes", bytes.len());
            let MasterMsg::Servers { total, entries, .. } = page else {
                unreachable!()
            };
            assert_eq!(total, 201);
            assert!(!entries.is_empty());
            seen += entries.len();
            off += entries.len() as u32;
            if seen as u32 >= total {
                break;
            }
        }
        assert_eq!(seen, 201);
        master.servers.get_mut("loop1:27015").unwrap().1 = 150;
        assert_eq!(
            master.expire(100 + TTL_S).len(),
            0,
            "exactly TTL old still counts"
        );
        assert_eq!(
            master.expire(101 + TTL_S).len(),
            200,
            "the silent entries go"
        );
        assert_eq!(master.servers.len(), 1, "the one heard at 150 stays");
    }

    #[test]
    fn a_reliable_message_in_the_lost_first_packet_is_resent() {
        let (mut a, mut b) = (Channel::default(), Channel::default());
        a.send_reliable(b"welcome".to_vec());
        let _lost = a.build(&[]);
        let empty = b.build(&[]);
        a.receive(&empty).unwrap();
        assert_eq!(
            a.pending_reliable(),
            1,
            "an ack of nothing is not an ack of packet 0"
        );
        let again = a.build(&[]);
        assert_eq!(b.receive(&again).unwrap(), vec![b"welcome".to_vec()]);
        a.receive(&b.build(&[])).unwrap();
        assert_eq!(a.pending_reliable(), 0);
    }

    #[test]
    fn long_reliable_messages_arrive_whole_and_in_order_over_loss_within_the_packet_budget() {
        use crate::channel::{MAX_BURST, MAX_PACKET};
        let (mut a, mut b) = (Channel::default(), Channel::default());
        let big: Vec<u8> = (0..50_000u32).map(|i| (i * 7 % 251) as u8).collect();
        a.send_reliable(b"first".to_vec());
        a.send_reliable(big.clone());
        a.send_reliable(b"last".to_vec());
        let mut got = Vec::new();
        let mut rng = 11u64;
        let mut ticks = 0;
        while got.len() < 3 {
            ticks += 1;
            assert!(ticks < 200, "transfer did not finish");
            let burst = a.build_burst(&[b"snap".to_vec()]);
            assert!(burst.len() <= MAX_BURST);
            for p in burst {
                assert!(p.len() <= MAX_PACKET, "{} bytes", p.len());
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                if rng.is_multiple_of(5) {
                    continue; // lost
                }
                got.extend(b.receive(&p).unwrap().into_iter().filter(|m| m != b"snap"));
            }
            a.receive(&b.build(&[])).unwrap();
        }
        assert_eq!(got, vec![b"first".to_vec(), big, b"last".to_vec()]);
        while a.pending_reliable() > 0 {
            for p in a.build_burst(&[]) {
                b.receive(&p).unwrap();
            }
            a.receive(&b.build(&[])).unwrap();
        }
    }
}
