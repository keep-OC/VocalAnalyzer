use std::net::UdpSocket;

const PARAM_FT_L: &str = "/avatar/parameters/FT_L";
const PARAM_FT_H: &str = "/avatar/parameters/FT_H";
const PARAM_G1: &str = "/avatar/parameters/G1";

pub struct OscSender {
    sock: UdpSocket,
}

fn log_scale(x: f32, min: f32, max: f32) -> f32 {
    (x.ln() - min.ln()) / (max.ln() - min.ln())
}

impl OscSender {
    pub fn new() -> Self {
        let host_addr = "127.0.0.1:0";
        let to_addr = "127.0.0.1:9000";
        let sock = UdpSocket::bind(host_addr).unwrap();
        sock.connect(to_addr).unwrap();
        Self { sock }
    }
    fn send_float(&self, addr: &str, v: f32) {
        let addr = addr.into();
        let args = vec![rosc::OscType::Float(v)];
        let message = rosc::OscMessage { addr, args };
        let packet = rosc::OscPacket::Message(message);
        let bytes = rosc::encoder::encode(&packet).unwrap();
        self.sock.send(&bytes).unwrap();
    }
    pub fn send_frequency(&self, freq: f32) {
        let e2_freq = 82.407;
        let g5_freq = 783.991;
        let log_freq = log_scale(freq, e2_freq, g5_freq);
        let v = (log_freq * (1u32 << 14) as f32) as u32;
        let ft_l = (v & 0x7F) as f32 / 128.0;
        let ft_h = ((v >> 7) & 0x7F) as f32 / 128.0;
        self.send_float(PARAM_FT_L, ft_l);
        self.send_float(PARAM_FT_H, ft_h);
        self.send_float(PARAM_G1, 1.0);
    }
}
