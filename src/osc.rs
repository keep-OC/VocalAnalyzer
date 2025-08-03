use std::net::UdpSocket;
use std::time::SystemTime;

use rosc::{encoder, OscBundle, OscMessage, OscPacket, OscType};

const PARAM_FT: &str = "/avatar/parameters/FT";
const PARAM_G_PREFIX: &str = "/avatar/parameters/G";
const PARAM_FORMANT_PREFIX: &str = "/avatar/parameters/F";

pub struct OscSender {
    sock: UdpSocket,
}

fn new_float_message(addr: &str, v: f32) -> OscMessage {
    let mut message = OscMessage::from(addr);
    message.args.push(OscType::from(v));
    message
}

fn split_float(v: f32) -> (f32, f32) {
    let i = (v * 0x3FFF as f32) as u32;
    let l = (i & 0x7F) as f32 / 127.0;
    let h = ((i >> 7) & 0x7F) as f32 / 127.0;
    (l, h)
}

fn new_split_float_message(addr_base: &str, v: f32) -> (OscMessage, OscMessage) {
    let (l, h) = split_float(v);
    let addr_l = format!("{addr_base}_L");
    let addr_h = format!("{addr_base}_H");
    let l = new_float_message(&addr_l, l);
    let h = new_float_message(&addr_h, h);
    (l, h)
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
            content: vs.into_iter().map(OscPacket::Message).collect(),
        };
        let packet = OscPacket::Bundle(bundle);
        let bytes = encoder::encode(&packet).unwrap();
        self.sock.send(&bytes).unwrap();
    }
    pub fn send_param(&self, freq: f32, gains: Vec<f32>, formants: Vec<f32>) {
        let mut vs: Vec<OscMessage> = gains
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                let addr = format!("{}{}", PARAM_G_PREFIX, i + 1);
                new_float_message(&addr, g)
            })
            .collect();
        let (ft_l, ft_h) = new_split_float_message(PARAM_FT, freq);
        vs.push(ft_l);
        vs.push(ft_h);
        formants.into_iter().enumerate().for_each(|(i, formant)| {
            let addr = format!("{}{}", PARAM_FORMANT_PREFIX, i + 1);
            let (l, h) = new_split_float_message(&addr, formant);
            vs.push(l);
            vs.push(h)
        });
        self.send_bundle(vs);
    }
}
