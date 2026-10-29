//! Own network protocol over the `gm-server` tick: packets with sequence/ack bitfields,
//! reliable messages resent until acked, 66 Hz usercmd batches (last 8 per packet), delta
//! snapshots (33 Hz default) against the newest snapshot the client acked. No compatibility with
//! the original game's protocol. Transports are pluggable; the loopback is deterministic.

pub mod channel;
pub mod client;
pub mod msg;
pub mod server;
pub mod transport;
pub mod wire;

pub use client::{ClientNet, ClientWorld, Interpolator, Prediction};
pub use msg::EntityState;
pub use msg::{ClientMsg, Command, NetCmd, ServerMsg};
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
            }],
            entities: vec![EntityState {
                id: 2,
                model: "models/props_junk/wood_crate001a.mdl".into(),
                pos: Vec3::new(900.0, -500.0, -123.7),
                angles: Vec3::new(0.0, 180.0, 0.0),
                vel: Vec3::new(0.0, 0.0, -9.0),
                angvel: Vec3::ZERO,
                frozen: false,
                owner: 1,
            }],
            removed: vec![1],
            own: OwnSandbox {
                count: 1,
                constraints: 0,
                undos: vec![UndoEntry {
                    id: 1,
                    name: "Prop".into(),
                }],
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
}
