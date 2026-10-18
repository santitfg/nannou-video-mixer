use crate::modules::param::Param;
use crate::ModuleChrome;
use std::net::UdpSocket;

pub const DEFAULT_PORT: f32 = 9000.0;
pub const OSC_LABELS: [&str; 3] = ["Port", "Min", "Max"];

/// A typed-in number box (dragging moves 1.0 per box width).
fn field(value: f32) -> Param {
    let mut p = Param::unit(value);
    p.set_clamp(false);
    p
}

/// OSC: listens for Open Sound Control messages over UDP (TouchOSC, Max,
/// Ableton, a phone...) and outputs the first numeric argument of the
/// first address it hears, scaled from Min..Max onto 0..1. Changing the
/// port rebinds and forgets the address, so the next sender is picked up.
pub struct OscState {
    pub chrome: ModuleChrome,
    /// Port, Min, Max.
    pub fields: [Param; 3],
    /// The address this module follows (the first one heard on this port).
    pub address: Option<String>,
    pub error: Option<String>,
    last: Option<f32>,
    socket: Option<(u16, UdpSocket)>,
}

impl OscState {
    pub fn new() -> Self {
        OscState {
            chrome: ModuleChrome::new(),
            fields: [field(DEFAULT_PORT), field(0.0), field(1.0)],
            address: None,
            error: None,
            last: None,
            socket: None,
        }
    }

    pub fn port(&self) -> u16 {
        self.fields[0].value.round().clamp(1.0, 65535.0) as u16
    }

    /// The last raw value received.
    pub fn raw(&self) -> Option<f32> {
        self.last
    }

    /// The last value mapped from Min..Max onto 0..1.
    pub fn output(&self) -> Option<f32> {
        let (lo, hi) = (self.fields[1].value, self.fields[2].value);
        let raw = self.last?;
        if (hi - lo).abs() < f32::EPSILON {
            return Some(0.0);
        }
        Some(((raw - lo) / (hi - lo)).clamp(0.0, 1.0))
    }

    /// Reads whatever has arrived since the last tick, (re)binding the
    /// socket when the port changed.
    pub fn poll(&mut self) {
        let port = self.port();
        if self.socket.as_ref().map(|(p, _)| *p) != Some(port) {
            self.address = None;
            self.last = None;
            self.socket = None;
            match UdpSocket::bind(("0.0.0.0", port)).and_then(|s| s.set_nonblocking(true).map(|_| s)) {
                Ok(s) => {
                    self.error = None;
                    self.socket = Some((port, s));
                }
                Err(e) => {
                    self.error = Some(format!("port {port}: {e}"));
                    return;
                }
            }
        }
        let Some((_, socket)) = &self.socket else { return };
        let mut buf = [0u8; 2048];
        let mut messages = Vec::new();
        for _ in 0..256 {
            match socket.recv(&mut buf) {
                Ok(n) => parse_packet(&buf[..n], &mut messages),
                Err(_) => break,
            }
        }
        for (address, value) in messages {
            match &self.address {
                None => self.address = Some(address),
                Some(a) if *a != address => continue,
                _ => {}
            }
            self.last = Some(value);
        }
    }
}

/// Collects `(address, first numeric argument)` from a message or a bundle.
pub fn parse_packet(data: &[u8], out: &mut Vec<(String, f32)>) {
    if data.starts_with(b"#bundle\0") {
        let mut rest = data.get(16..).unwrap_or(&[]);
        while rest.len() >= 4 {
            let size = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            let Some(element) = rest.get(4..4 + size) else { break };
            parse_packet(element, out);
            rest = &rest[4 + size..];
        }
        return;
    }
    if let Some(message) = parse_message(data) {
        out.push(message);
    }
}

/// An OSC string (null-terminated, padded to 4 bytes) and the offset after it.
fn read_string(data: &[u8], at: usize) -> Option<(&str, usize)> {
    let rest = data.get(at..)?;
    let len = rest.iter().position(|&b| b == 0)?;
    let text = std::str::from_utf8(&rest[..len]).ok()?;
    Some((text, at + (len + 4) / 4 * 4))
}

fn parse_message(data: &[u8]) -> Option<(String, f32)> {
    let (address, at) = read_string(data, 0)?;
    if !address.starts_with('/') {
        return None;
    }
    let (tags, mut at) = read_string(data, at)?;
    for tag in tags.strip_prefix(',')?.chars() {
        let value = match tag {
            'f' => f32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?),
            'i' => i32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as f32,
            'd' => f64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?) as f32,
            'h' => i64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?) as f32,
            'T' => return Some((address.to_string(), 1.0)),
            'F' => return Some((address.to_string(), 0.0)),
            // Strings/blobs before the first number: skip what we can't size.
            _ => return None,
        };
        if value.is_finite() {
            return Some((address.to_string(), value));
        }
        at += if tag == 'd' || tag == 'h' { 8 } else { 4 };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(address: &str, tags: &str, args: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        for s in [address, tags] {
            v.extend(s.as_bytes());
            v.push(0);
            while v.len() % 4 != 0 {
                v.push(0);
            }
        }
        v.extend(args);
        v
    }

    #[test]
    fn parses_float_and_int() {
        let mut out = Vec::new();
        parse_packet(&message("/fader/1", ",f", &0.25f32.to_be_bytes()), &mut out);
        parse_packet(&message("/n", ",i", &7i32.to_be_bytes()), &mut out);
        assert_eq!(out, [("/fader/1".to_string(), 0.25), ("/n".to_string(), 7.0)]);
    }

    #[test]
    fn parses_bundles() {
        let m = message("/x", ",f", &1.0f32.to_be_bytes());
        let mut bundle = b"#bundle\0".to_vec();
        bundle.extend([0u8; 8]);
        bundle.extend((m.len() as u32).to_be_bytes());
        bundle.extend(&m);
        let mut out = Vec::new();
        parse_packet(&bundle, &mut out);
        assert_eq!(out, [("/x".to_string(), 1.0)]);
    }

    #[test]
    fn ignores_garbage() {
        let mut out = Vec::new();
        parse_packet(b"hello", &mut out);
        parse_packet(&message("/s", ",s", b"hi\0\0"), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn scales_into_unit_range() {
        let mut osc = OscState::new();
        osc.fields[1].set(0.0);
        osc.fields[2].set(127.0);
        osc.last = Some(63.5);
        assert_eq!(osc.output(), Some(0.5));
        osc.last = Some(500.0);
        assert_eq!(osc.output(), Some(1.0));
    }
}
