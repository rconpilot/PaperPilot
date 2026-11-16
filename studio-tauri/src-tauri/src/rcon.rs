use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

// Source RCON packet types.
const SERVERDATA_RESPONSE_VALUE: i32 = 0;
const SERVERDATA_EXECCOMMAND: i32 = 2;
const SERVERDATA_AUTH: i32 = 3;
const SERVERDATA_AUTH_RESPONSE: i32 = 2;

#[derive(Debug)]
struct Packet {
    id: i32,
    #[allow(dead_code)]
    pkt_type: i32,
    body: String,
}

/// Encode one RCON packet:
/// [int32 LE size][int32 LE id][int32 LE type][body bytes][0x00][0x00]
/// where size counts everything after the size field itself.
fn encode_packet(id: i32, pkt_type: i32, body: &str) -> Vec<u8> {
    let body_bytes = body.as_bytes();
    let size = 4 + 4 + body_bytes.len() as i32 + 2;
    let mut buf = Vec::with_capacity((4 + size) as usize);
    buf.extend_from_slice(&size.to_le_bytes());
    buf.extend_from_slice(&id.to_le_bytes());
    buf.extend_from_slice(&pkt_type.to_le_bytes());
    buf.extend_from_slice(body_bytes);
    buf.push(0);
    buf.push(0);
    buf
}

/// A Source RCON client for a Minecraft (Paper) server — the Rust port of
/// the TypeScript core's RconClient, with identical wire behaviour.
///
/// Connects lazily on the first send() and reconnects if the socket drops.
/// Command responses that span several packets are reassembled using the
/// standard sentinel-packet trick: an empty RESPONSE_VALUE packet with a
/// different id marks the end of the response — everything received before
/// the sentinel's response belongs to the command. The sentinel is sent only
/// after the first response packet arrives (see send_inner for why). Sends
/// are serialized by the owning service's mutex, exactly as the TS
/// send-chain serialized them.
pub struct RconClient {
    host: String,
    port: u16,
    password: String,
    timeout: Duration,
    stream: Option<TcpStream>,
    buffer: Vec<u8>,
    next_id: i32,
}

impl RconClient {
    pub fn new(host: &str, port: u16, password: &str) -> Self {
        RconClient {
            host: host.to_string(),
            port,
            password: password.to_string(),
            timeout: Duration::from_millis(10_000),
            stream: None,
            buffer: Vec::new(),
            next_id: 2,
        }
    }

    pub fn send(&mut self, command: &str) -> Result<String, String> {
        let result = self.send_inner(command);
        if result.is_err() {
            // A failed op leaves the socket state unknown; drop it so the
            // next send reconnects, like the TS client does on close/error.
            self.stream = None;
            self.buffer.clear();
        }
        result
    }

    fn send_inner(&mut self, command: &str) -> Result<String, String> {
        self.ensure_connected()?;
        let command_id = self.next_id;
        self.next_id += 1;
        let sentinel_id = self.next_id;
        self.next_id += 1;
        self.write_packet(command_id, SERVERDATA_EXECCOMMAND, command)?;

        let deadline = Instant::now() + self.timeout;
        let mut response = String::new();
        // The sentinel packet is deliberately NOT written yet. Vanilla's
        // RCON reader mishandles two client packets that arrive in the same
        // TCP segment — if the sentinel bytes are already in its read buffer
        // when it processes the command, the connection dies (this port's
        // tight write loop hit that reliably; the TS client's looser timing
        // usually escaped it). The server always answers an EXECCOMMAND,
        // even with an empty body, so: read the first response packet, and
        // only then send the sentinel. Everything the server sends for the
        // command precedes the sentinel's own response on the wire, so
        // reassembly stays exact.
        let mut sentinel_sent = false;
        loop {
            if Instant::now() >= deadline {
                return Err(format!(
                    "RCON command timed out after {} ms: {command}",
                    self.timeout.as_millis()
                ));
            }
            let packet = self.read_packet(deadline)?;
            if packet.id == sentinel_id {
                break;
            }
            if packet.id == command_id {
                response.push_str(&packet.body);
            }
            // Packets with any other id are ignored (should not happen).
            if !sentinel_sent {
                self.write_packet(sentinel_id, SERVERDATA_RESPONSE_VALUE, "")?;
                sentinel_sent = true;
            }
        }
        Ok(response)
    }

    fn write_packet(&mut self, id: i32, pkt_type: i32, body: &str) -> Result<(), String> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| "RCON socket is not connected.".to_string())?;
        stream
            .write_all(&encode_packet(id, pkt_type, body))
            .map_err(|e| format!("RCON write failed: {e}"))
    }

    fn ensure_connected(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Ok(());
        }
        self.connect()
    }

    fn connect(&mut self) -> Result<(), String> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| {
                format!(
                    "RCON connection to {}:{} failed: {e}",
                    self.host, self.port
                )
            })?
            .next()
            .ok_or_else(|| {
                format!(
                    "RCON connection to {}:{} failed: could not resolve host.",
                    self.host, self.port
                )
            })?;
        let stream = TcpStream::connect_timeout(&addr, self.timeout).map_err(|e| {
            format!(
                "RCON connection to {}:{} failed: {e}",
                self.host, self.port
            )
        })?;
        let _ = stream.set_nodelay(true);
        self.stream = Some(stream);
        self.buffer.clear();

        // Authenticate: id 1, type SERVERDATA_AUTH. The server replies with
        // an (optional) empty RESPONSE_VALUE followed by an AUTH_RESPONSE
        // whose id mirrors ours on success, or -1 on failure.
        let auth_id = 1;
        self.write_packet(auth_id, SERVERDATA_AUTH, &self.password.clone())?;
        let deadline = Instant::now() + self.timeout;
        loop {
            if Instant::now() >= deadline {
                self.stream = None;
                return Err("RCON authentication timed out.".to_string());
            }
            let packet = self.read_packet(deadline)?;
            if packet.pkt_type == SERVERDATA_AUTH_RESPONSE {
                if packet.id == -1 {
                    self.stream = None;
                    return Err("RCON authentication failed: wrong password.".to_string());
                }
                if packet.id == auth_id {
                    return Ok(());
                }
            }
        }
    }

    /// Decode one packet from the front of the buffer, when complete.
    fn try_decode(&mut self) -> Option<Packet> {
        if self.buffer.len() < 4 {
            return None;
        }
        let size = i32::from_le_bytes(self.buffer[0..4].try_into().unwrap());
        if size < 10 {
            // Malformed — the TS client throws here; surface it by dropping
            // the buffer so the read loop errors out instead of spinning.
            self.buffer.clear();
            return None;
        }
        let total = 4 + size as usize;
        if self.buffer.len() < total {
            return None;
        }
        let id = i32::from_le_bytes(self.buffer[4..8].try_into().unwrap());
        let pkt_type = i32::from_le_bytes(self.buffer[8..12].try_into().unwrap());
        let body = String::from_utf8_lossy(&self.buffer[12..total - 2]).into_owned();
        self.buffer.drain(..total);
        Some(Packet { id, pkt_type, body })
    }

    fn read_packet(&mut self, deadline: Instant) -> Result<Packet, String> {
        loop {
            if let Some(packet) = self.try_decode() {
                return Ok(packet);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Timed out waiting for an RCON response.".to_string());
            }
            let mut chunk = [0u8; 8192];
            let n = {
                let stream = self
                    .stream
                    .as_mut()
                    .ok_or_else(|| "RCON socket is not connected.".to_string())?;
                stream
                    .set_read_timeout(Some(remaining))
                    .map_err(|e| e.to_string())?;
                stream.read(&mut chunk)
            };
            match n {
                Ok(0) => {
                    self.stream = None;
                    return Err("RCON connection closed by the server.".to_string());
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(e) => {
                    return Err(format!("Timed out waiting for an RCON response ({e})."));
                }
            }
        }
    }
}
