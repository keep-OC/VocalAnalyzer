use std::net::UdpSocket;
use std::time::SystemTime;

use rosc::{encoder, OscBundle, OscMessage, OscPacket, OscType};

const PARAM_FT_L: &str = "/avatar/parameters/FT_L";
const PARAM_FT_H: &str = "/avatar/parameters/FT_H";
const PARAM_G_PREFIX: &str = "/avatar/parameters/G";

pub struct OscSender {
    sock: UdpSocket,
}

fn new_float_message(addr: &str, v: f32) -> OscMessage {
    let mut message = OscMessage::from(addr);
    message.args.push(OscType::from(v));
    message
}

impl OscSender {
    pub fn new() -> Self {
        let host_addr = "127.0.0.1:0";
        let to_addr = "127.0.0.1:9000";
        let sock = UdpSocket::bind(host_addr).unwrap();
        sock.connect(to_addr).unwrap();
        Self { sock }
    }
    fn send_bundle(&self, vs: Vec<OscMessage>) {
        let bundle = OscBundle {
            timetag: SystemTime::now().try_into().unwrap(),
            content: vs.into_iter().map(|v| OscPacket::Message(v)).collect(),
        };
        let packet = OscPacket::Bundle(bundle);
        let bytes = encoder::encode(&packet).unwrap();
        self.sock.send(&bytes).unwrap();
    }
    pub fn send_frequency(&self, freq: f32, gains: Vec<f32>) {
        let v = (freq * 0x3FFF as f32) as u32;
        let ft_l = (v & 0x7F) as f32 / 127.0;
        let ft_h = ((v >> 7) & 0x7F) as f32 / 127.0;
        let ft_l = new_float_message(PARAM_FT_L, ft_l);
        let ft_h = new_float_message(PARAM_FT_H, ft_h);
        let mut vs: Vec<OscMessage> = gains
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                let addr = format!("{}{}", PARAM_G_PREFIX, i + 1);
                new_float_message(&addr, g)
            })
            .collect();
        vs.push(ft_l);
        vs.push(ft_h);
        self.send_bundle(vs);
    }
}
