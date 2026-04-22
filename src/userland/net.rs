use core::fmt;

use crate::drivers::net::rtl8139;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Addr {
    octets: [u8; 4],
}

impl Ipv4Addr {
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self { octets: [a, b, c, d] }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let mut out = [0u8; 4];
        let mut idx = 0usize;
        let mut cur = 0u16;
        let mut has_digit = false;

        for b in bytes {
            if *b == b'.' {
                if !has_digit || idx >= 3 || cur > 255 {
                    return None;
                }
                out[idx] = cur as u8;
                idx += 1;
                cur = 0;
                has_digit = false;
                continue;
            }
            if !(*b >= b'0' && *b <= b'9') {
                return None;
            }
            cur = cur.saturating_mul(10).saturating_add((b - b'0') as u16);
            has_digit = true;
        }

        if idx != 3 || !has_digit || cur > 255 {
            return None;
        }
        out[3] = cur as u8;
        Some(Self { octets: out })
    }

    pub const fn octets(self) -> [u8; 4] {
        self.octets
    }

    pub fn to_u32(self) -> u32 {
        let [a, b, c, d] = self.octets;
        ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | d as u32
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.octets;
        write!(f, "{}.{}.{}.{}", a, b, c, d)
    }
}

#[derive(Clone, Copy)]
pub struct NetInterface {
    pub name: &'static str,
    pub up: bool,
    pub mac: [u8; 6],
    pub addr: Ipv4Addr,
    pub mask: Ipv4Addr,
    pub gateway: Ipv4Addr,
}

#[derive(Clone, Copy)]
pub struct NetStats {
    pub tx_packets: u32,
    pub rx_packets: u32,
    pub dropped_packets: u32,
    pub icmp_tx: u32,
    pub icmp_rx: u32,
}

#[derive(Clone, Copy)]
pub struct TelnetSession {
    pub id: u8,
    pub active: bool,
    pub remote: Ipv4Addr,
    pub port: u16,
    pub rx_bytes: u32,
    pub tx_bytes: u32,
    local_port: u16,
    next_seq: u32,
    next_ack: u32,
    peer_mac: [u8; 6],
    pending_len: u16,
    pending: [u8; 512],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TcpProbeStatus {
    SynAck,
    Rst,
    Timeout,
    ArpFail,
    LinkDown,
    TxFail,
}

#[derive(Clone, Copy)]
struct ArpEntry {
    valid: bool,
    ip: Ipv4Addr,
    mac: [u8; 6],
}

const MAX_SESSIONS: usize = 8;

pub struct NetStack {
    iface: NetInterface,
    stats: NetStats,
    sessions: [TelnetSession; MAX_SESSIONS],
    arp: ArpEntry,
    ip_ident: u16,
}

impl NetStack {
    pub const fn new() -> Self {
        Self {
            iface: NetInterface {
                name: "eth0",
                up: true,
                mac: [0; 6],
                addr: Ipv4Addr::new(10, 0, 2, 15),
                mask: Ipv4Addr::new(255, 255, 255, 0),
                gateway: Ipv4Addr::new(10, 0, 2, 2),
            },
            stats: NetStats {
                tx_packets: 0,
                rx_packets: 0,
                dropped_packets: 0,
                icmp_tx: 0,
                icmp_rx: 0,
            },
            sessions: [TelnetSession {
                id: 0,
                active: false,
                remote: Ipv4Addr::new(0, 0, 0, 0),
                port: 0,
                rx_bytes: 0,
                tx_bytes: 0,
                local_port: 0,
                next_seq: 0,
                next_ack: 0,
                peer_mac: [0; 6],
                pending_len: 0,
                pending: [0; 512],
            }; MAX_SESSIONS],
            arp: ArpEntry {
                valid: false,
                ip: Ipv4Addr::new(0, 0, 0, 0),
                mac: [0; 6],
            },
            ip_ident: 1,
        }
    }

    pub fn interface(&self) -> NetInterface {
        let mut i = self.iface;
        if rtl8139::is_ready() {
            i.mac = rtl8139::mac_addr();
        }
        i
    }

    pub fn set_interface(&mut self, addr: Ipv4Addr, mask: Ipv4Addr, gateway: Ipv4Addr) {
        self.iface.addr = addr;
        self.iface.mask = mask;
        self.iface.gateway = gateway;
        self.arp.valid = false;
    }

    pub fn set_gateway(&mut self, gateway: Ipv4Addr) {
        self.iface.gateway = gateway;
        self.arp.valid = false;
    }

    pub fn set_link_state(&mut self, up: bool) {
        self.iface.up = up;
    }

    pub fn frame_ready(&self) -> bool {
        rtl8139::has_rx()
    }

    pub fn resolve_host(&mut self, host: &str) -> Option<Ipv4Addr> {
        if let Some(ip) = Ipv4Addr::parse(host) {
            return Some(ip);
        }
        match host {
            "localhost" => Some(Ipv4Addr::new(127, 0, 0, 1)),
            "gateway" | "gw" | "router" => Some(self.iface.gateway),
            "dns" => Some(Ipv4Addr::new(10, 0, 2, 3)),
            "google" => Some(Ipv4Addr::new(8, 8, 8, 8)),
            "cloudflare" => Some(Ipv4Addr::new(1, 1, 1, 1)),
            "example" => Some(Ipv4Addr::new(93, 184, 216, 34)),
            _ => self.dns_lookup_a(host),
        }
    }

    pub fn ping_once(&mut self, target: Ipv4Addr, seq: u8) -> Option<u16> {
        self.stats.tx_packets = self.stats.tx_packets.saturating_add(1);
        self.stats.icmp_tx = self.stats.icmp_tx.saturating_add(1);

        if !self.iface.up || !rtl8139::is_ready() {
            self.stats.dropped_packets = self.stats.dropped_packets.saturating_add(1);
            return None;
        }

        let next_hop = self.next_hop(target);
        let dst_mac = match self.arp_resolve(next_hop) {
            Some(m) => m,
            None => {
                self.stats.dropped_packets = self.stats.dropped_packets.saturating_add(1);
                return None;
            }
        };

        let mut pkt = [0u8; 98];
        let src_mac = rtl8139::mac_addr();
        build_eth_header(&mut pkt[0..14], dst_mac, src_mac, 0x0800);

        let icmp_len = 8usize;
        let ip_len = 20usize + icmp_len;
        build_ipv4_header(
            &mut pkt[14..34],
            self.iface.addr,
            target,
            1,
            ip_len as u16,
            self.next_ident(),
        );

        pkt[34] = 8;
        pkt[35] = 0;
        pkt[36] = 0;
        pkt[37] = 0;
        pkt[38] = 0x12;
        pkt[39] = 0x34;
        pkt[40] = 0x00;
        pkt[41] = seq;
        let csum = checksum16(&pkt[34..42]);
        pkt[36] = (csum >> 8) as u8;
        pkt[37] = csum as u8;

        if rtl8139::send_frame(&pkt[..42]).is_err() {
            self.stats.dropped_packets = self.stats.dropped_packets.saturating_add(1);
            return None;
        }

        let mut rx = [0u8; 1600];
        for t in 1..=200_000u32 {
            let Some(n) = Self::recv_frame_with_wait(&mut rx, 1) else {
                continue;
            };
            self.stats.rx_packets = self.stats.rx_packets.saturating_add(1);
            if n < 42 {
                continue;
            }

            let ether_type = u16::from_be_bytes([rx[12], rx[13]]);
            if ether_type == 0x0806 {
                self.learn_arp(&rx[..n]);
                continue;
            }
            if ether_type != 0x0800 {
                continue;
            }
            if rx[23] != 1 {
                continue;
            }
            if rx[26..30] != target.octets() {
                continue;
            }
            if rx[30..34] != self.iface.addr.octets() {
                continue;
            }
            if rx[34] != 0 || rx[35] != 0 {
                continue;
            }
            if rx[38] != 0x12 || rx[39] != 0x34 || rx[41] != seq {
                continue;
            }

            self.stats.icmp_rx = self.stats.icmp_rx.saturating_add(1);
            return Some(1 + ((t as u16) % 120));
        }

        self.stats.dropped_packets = self.stats.dropped_packets.saturating_add(1);
        None
    }

    pub fn open_telnet(&mut self, remote: Ipv4Addr, port: u16) -> Result<(u8, &'static str), &'static str> {
        if !self.iface.up || !rtl8139::is_ready() {
            return Err("network device down");
        }

        let idx = self.sessions.iter().position(|s| !s.active).ok_or("too many open sessions")?;

        let hop = self.next_hop(remote);
        let dst_mac = self.arp_resolve(hop).ok_or("arp failed")?;

        let src_port = 40000u16 + idx as u16;
        let seq = 0x1020_3040u32.wrapping_add(idx as u32);

        let mut syn = [0u8; 74];
        let src_mac = rtl8139::mac_addr();
        build_eth_header(&mut syn[0..14], dst_mac, src_mac, 0x0800);
        build_ipv4_header(&mut syn[14..34], self.iface.addr, remote, 6, 40, self.next_ident());
        build_tcp_header(
            &mut syn[34..54],
            self.iface.addr,
            remote,
            src_port,
            port,
            seq,
            0,
            0x02,
            1024,
            &[],
        );

        rtl8139::send_frame(&syn[..54]).map_err(|_| "tx error")?;

        let mut rx = [0u8; 1600];
        let mut remote_seq = 0u32;
        let mut got_synack = false;

        for _ in 0..300_000u32 {
            let Some(n) = Self::recv_frame_with_wait(&mut rx, 1) else {
                continue;
            };
            if n < 54 {
                continue;
            }
            if u16::from_be_bytes([rx[12], rx[13]]) == 0x0806 {
                self.learn_arp(&rx[..n]);
                continue;
            }
            if u16::from_be_bytes([rx[12], rx[13]]) != 0x0800 || rx[23] != 6 {
                continue;
            }
            if rx[26..30] != remote.octets() || rx[30..34] != self.iface.addr.octets() {
                continue;
            }

            let ip_hlen = ((rx[14] & 0x0F) as usize) * 4;
            let off = 14 + ip_hlen;
            if n < off + 20 {
                continue;
            }

            let dst = u16::from_be_bytes([rx[off + 2], rx[off + 3]]);
            if dst != src_port {
                continue;
            }
            let srcp = u16::from_be_bytes([rx[off], rx[off + 1]]);
            if srcp != port {
                continue;
            }
            let flags = rx[off + 13];
            if (flags & 0x04) != 0 {
                return Err("connection refused");
            }
            if (flags & 0x12) == 0x12 {
                remote_seq = u32::from_be_bytes([rx[off + 4], rx[off + 5], rx[off + 6], rx[off + 7]]);
                got_synack = true;
                break;
            }
        }

        if !got_synack {
            return Err("tcp connect timeout");
        }

        let mut ack = [0u8; 74];
        build_eth_header(&mut ack[0..14], dst_mac, src_mac, 0x0800);
        build_ipv4_header(&mut ack[14..34], self.iface.addr, remote, 6, 40, self.next_ident());
        build_tcp_header(
            &mut ack[34..54],
            self.iface.addr,
            remote,
            src_port,
            port,
            seq.wrapping_add(1),
            remote_seq.wrapping_add(1),
            0x10,
            1024,
            &[],
        );
        rtl8139::send_frame(&ack[..54]).map_err(|_| "tx error")?;

        self.sessions[idx] = TelnetSession {
            id: idx as u8,
            active: true,
            remote,
            port,
            rx_bytes: 0,
            tx_bytes: 0,
            local_port: src_port,
            next_seq: seq.wrapping_add(1),
            next_ack: remote_seq.wrapping_add(1),
            peer_mac: dst_mac,
            pending_len: 0,
            pending: [0; 512],
        };

        Ok((idx as u8, "connected"))
    }

    pub fn send_telnet_data(&mut self, id: u8, data: &[u8]) -> Result<usize, &'static str> {
        let idx = id as usize;
        if idx >= MAX_SESSIONS {
            return Err("invalid session id");
        }
        if !self.sessions[idx].active {
            return Err("session not active");
        }
        if data.is_empty() {
            return Ok(0);
        }
        if data.len() > 512 {
            return Err("payload too large");
        }

        let s = self.sessions[idx];
        self.send_tcp_segment(
            s.peer_mac,
            s.remote,
            s.local_port,
            s.port,
            s.next_seq,
            s.next_ack,
            0x18,
            data,
        )?;

        let target_ack = s.next_seq.wrapping_add(data.len() as u32);
        let _ = self.wait_for_ack(idx, target_ack, 15000);
        self.sessions[idx].next_seq = target_ack;
        self.sessions[idx].tx_bytes = self.sessions[idx].tx_bytes.saturating_add(data.len() as u32);
        Ok(data.len())
    }

    pub fn recv_telnet_data(&mut self, id: u8, out: &mut [u8], spins: u32) -> Result<usize, &'static str> {
        let idx = id as usize;
        if idx >= MAX_SESSIONS {
            return Err("invalid session id");
        }
        if !self.sessions[idx].active {
            return Err("session not active");
        }
        let pending_len = self.sessions[idx].pending_len as usize;
        if pending_len > 0 {
            let copy_len = core::cmp::min(out.len(), pending_len);
            out[..copy_len].copy_from_slice(&self.sessions[idx].pending[..copy_len]);
            if copy_len < pending_len {
                let mut i = 0usize;
                while i + copy_len < pending_len {
                    self.sessions[idx].pending[i] = self.sessions[idx].pending[i + copy_len];
                    i += 1;
                }
            }
            self.sessions[idx].pending_len = (pending_len - copy_len) as u16;
            self.sessions[idx].rx_bytes = self.sessions[idx].rx_bytes.saturating_add(copy_len as u32);
            return Ok(copy_len);
        }

        let mut frame = [0u8; 1600];
        for _ in 0..spins {
            let Some(n) = Self::recv_frame_with_wait(&mut frame, 1) else {
                continue;
            };
            let evt = self.parse_tcp_event(idx, &frame[..n]);
            if evt.fin {
                let _ = self.send_tcp_segment(
                    self.sessions[idx].peer_mac,
                    self.sessions[idx].remote,
                    self.sessions[idx].local_port,
                    self.sessions[idx].port,
                    self.sessions[idx].next_seq,
                    evt.seq.wrapping_add(1),
                    0x10,
                    &[],
                );
                self.sessions[idx].active = false;
                return Ok(0);
            }
            if evt.payload_len == 0 {
                continue;
            }

            let copy_len = core::cmp::min(out.len(), evt.payload_len);
            out[..copy_len].copy_from_slice(&evt.payload[..copy_len]);

            self.sessions[idx].next_ack = evt.seq.wrapping_add(evt.payload_len as u32);
            self.sessions[idx].rx_bytes = self.sessions[idx].rx_bytes.saturating_add(copy_len as u32);

            let _ = self.send_tcp_segment(
                self.sessions[idx].peer_mac,
                self.sessions[idx].remote,
                self.sessions[idx].local_port,
                self.sessions[idx].port,
                self.sessions[idx].next_seq,
                self.sessions[idx].next_ack,
                0x10,
                &[],
            );
            return Ok(copy_len);
        }

        Ok(0)
    }

    pub fn tcp_probe(&mut self, remote: Ipv4Addr, port: u16) -> bool {
        matches!(self.tcp_probe_status(remote, port), TcpProbeStatus::SynAck | TcpProbeStatus::Rst)
    }

    pub fn tcp_probe_status(&mut self, remote: Ipv4Addr, port: u16) -> TcpProbeStatus {
        if !self.iface.up || !rtl8139::is_ready() {
            return TcpProbeStatus::LinkDown;
        }
        let hop = self.next_hop(remote);
        let Some(dst_mac) = self.arp_resolve(hop) else {
            return TcpProbeStatus::ArpFail;
        };
        let src_port = 45000u16;
        let seq = 0x5566_7788u32;

        let mut syn = [0u8; 74];
        let src_mac = rtl8139::mac_addr();
        build_eth_header(&mut syn[0..14], dst_mac, src_mac, 0x0800);
        build_ipv4_header(&mut syn[14..34], self.iface.addr, remote, 6, 40, self.next_ident());
        build_tcp_header(
            &mut syn[34..54],
            self.iface.addr,
            remote,
            src_port,
            port,
            seq,
            0,
            0x02,
            1024,
            &[],
        );
        if rtl8139::send_frame(&syn[..54]).is_err() {
            return TcpProbeStatus::TxFail;
        }

        let mut rx = [0u8; 1600];
        for _ in 0..300_000u32 {
            let Some(n) = Self::recv_frame_with_wait(&mut rx, 1) else {
                continue;
            };
            if n < 54 || u16::from_be_bytes([rx[12], rx[13]]) != 0x0800 || rx[23] != 6 {
                continue;
            }
            if rx[26..30] != remote.octets() || rx[30..34] != self.iface.addr.octets() {
                continue;
            }

            let ip_hlen = ((rx[14] & 0x0F) as usize) * 4;
            let off = 14 + ip_hlen;
            if n < off + 20 {
                continue;
            }
            let dst = u16::from_be_bytes([rx[off + 2], rx[off + 3]]);
            if dst != src_port {
                continue;
            }

            let flags = rx[off + 13];
            if (flags & 0x12) == 0x12 {
                return TcpProbeStatus::SynAck;
            }
            if (flags & 0x04) != 0 {
                return TcpProbeStatus::Rst;
            }
        }
        TcpProbeStatus::Timeout
    }

    pub fn close_telnet(&mut self, id: u8) -> Result<(), &'static str> {
        let idx = id as usize;
        if idx >= MAX_SESSIONS {
            return Err("invalid session id");
        }
        if !self.sessions[idx].active {
            return Err("session not active");
        }

        let s = self.sessions[idx];
        let _ = self.send_tcp_segment(
            s.peer_mac,
            s.remote,
            s.local_port,
            s.port,
            s.next_seq,
            s.next_ack,
            0x11,
            &[],
        );
        let _ = self.wait_for_ack(idx, s.next_seq.wrapping_add(1), 8000);

        self.sessions[idx].active = false;
        Ok(())
    }

    pub fn stats(&self) -> NetStats {
        self.stats
    }

    pub fn sessions(&self) -> [TelnetSession; MAX_SESSIONS] {
        self.sessions
    }

    pub fn nic_info(&self) -> Option<([u8; 6], u8)> {
        if !rtl8139::is_ready() {
            return None;
        }
        Some((rtl8139::mac_addr(), rtl8139::irq_line().unwrap_or(0)))
    }

    fn next_ident(&mut self) -> u16 {
        let id = self.ip_ident;
        self.ip_ident = self.ip_ident.wrapping_add(1);
        id
    }

    fn next_hop(&self, target: Ipv4Addr) -> Ipv4Addr {
        let l = self.iface.addr.to_u32();
        let m = self.iface.mask.to_u32();
        let t = target.to_u32();
        if (l & m) == (t & m) {
            target
        } else {
            self.iface.gateway
        }
    }

    fn arp_resolve(&mut self, ip: Ipv4Addr) -> Option<[u8; 6]> {
        if self.arp.valid && self.arp.ip == ip {
            return Some(self.arp.mac);
        }

        let src_mac = rtl8139::mac_addr();
        let mut frame = [0u8; 42];
        build_eth_header(&mut frame[0..14], [0xFF; 6], src_mac, 0x0806);

        frame[14..16].copy_from_slice(&[0x00, 0x01]);
        frame[16..18].copy_from_slice(&[0x08, 0x00]);
        frame[18] = 6;
        frame[19] = 4;
        frame[20..22].copy_from_slice(&[0x00, 0x01]);
        frame[22..28].copy_from_slice(&src_mac);
        frame[28..32].copy_from_slice(&self.iface.addr.octets());
        frame[32..38].copy_from_slice(&[0; 6]);
        frame[38..42].copy_from_slice(&ip.octets());

        if rtl8139::send_frame(&frame).is_err() {
            return None;
        }

        let mut rx = [0u8; 1600];
        for _ in 0..200_000u32 {
            let Some(n) = Self::recv_frame_with_wait(&mut rx, 1) else {
                continue;
            };
            if n < 42 || u16::from_be_bytes([rx[12], rx[13]]) != 0x0806 {
                continue;
            }
            if rx[20] != 0x00 || rx[21] != 0x02 {
                continue;
            }
            if rx[28..32] != ip.octets() {
                continue;
            }
            if rx[38..42] != self.iface.addr.octets() {
                continue;
            }

            let mut mac = [0u8; 6];
            mac.copy_from_slice(&rx[22..28]);
            self.arp.valid = true;
            self.arp.ip = ip;
            self.arp.mac = mac;
            return Some(mac);
        }

        None
    }

    fn learn_arp(&mut self, frame: &[u8]) {
        if frame.len() < 42 {
            return;
        }
        if u16::from_be_bytes([frame[12], frame[13]]) != 0x0806 {
            return;
        }
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&frame[22..28]);
        let ip = Ipv4Addr::new(frame[28], frame[29], frame[30], frame[31]);
        self.arp.valid = true;
        self.arp.ip = ip;
        self.arp.mac = mac;
    }

    fn send_tcp_segment(
        &mut self,
        dst_mac: [u8; 6],
        remote: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        seq: u32,
        ack: u32,
        flags: u8,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        let mut pkt = [0u8; 1600];
        let src_mac = rtl8139::mac_addr();
        build_eth_header(&mut pkt[0..14], dst_mac, src_mac, 0x0800);
        let total_len = 20 + 20 + payload.len();
        build_ipv4_header(
            &mut pkt[14..34],
            self.iface.addr,
            remote,
            6,
            total_len as u16,
            self.next_ident(),
        );
        build_tcp_header(
            &mut pkt[34..54],
            self.iface.addr,
            remote,
            src_port,
            dst_port,
            seq,
            ack,
            flags,
            4096,
            payload,
        );
        if !payload.is_empty() {
            pkt[54..54 + payload.len()].copy_from_slice(payload);
        }
        rtl8139::send_frame(&pkt[..14 + total_len]).map_err(|_| "tx error")
    }

    fn wait_for_ack(&mut self, sid: usize, ack_value: u32, spins: u32) -> bool {
        let mut frame = [0u8; 1600];
        for _ in 0..spins {
            let Some(n) = Self::recv_frame_with_wait(&mut frame, 1) else {
                continue;
            };
            let evt = self.parse_tcp_event(sid, &frame[..n]);
            if evt.valid && evt.ack && evt.ack_num >= ack_value {
                return true;
            }
            if evt.payload_len > 0 {
                self.sessions[sid].next_ack = evt.seq.wrapping_add(evt.payload_len as u32);
                let cur = self.sessions[sid].pending_len as usize;
                let avail = self.sessions[sid].pending.len().saturating_sub(cur);
                if avail > 0 {
                    let take = core::cmp::min(avail, evt.payload_len);
                    self.sessions[sid].pending[cur..cur + take].copy_from_slice(&evt.payload[..take]);
                    self.sessions[sid].pending_len = (cur + take) as u16;
                }
                let _ = self.send_tcp_segment(
                    self.sessions[sid].peer_mac,
                    self.sessions[sid].remote,
                    self.sessions[sid].local_port,
                    self.sessions[sid].port,
                    self.sessions[sid].next_seq,
                    self.sessions[sid].next_ack,
                    0x10,
                    &[],
                );
            }
            if evt.fin {
                return false;
            }
        }
        false
    }

    fn parse_tcp_event<'a>(&self, sid: usize, frame: &'a [u8]) -> TcpEvent<'a> {
        let s = self.sessions[sid];
        if !s.active && s.local_port == 0 {
            return TcpEvent::invalid();
        }
        if frame.len() < 54 {
            return TcpEvent::invalid();
        }
        if u16::from_be_bytes([frame[12], frame[13]]) != 0x0800 || frame[23] != 6 {
            return TcpEvent::invalid();
        }
        if frame[26..30] != s.remote.octets() || frame[30..34] != self.iface.addr.octets() {
            return TcpEvent::invalid();
        }
        let ip_hlen = ((frame[14] & 0x0F) as usize) * 4;
        let off = 14 + ip_hlen;
        if frame.len() < off + 20 {
            return TcpEvent::invalid();
        }
        let srcp = u16::from_be_bytes([frame[off], frame[off + 1]]);
        let dstp = u16::from_be_bytes([frame[off + 2], frame[off + 3]]);
        if srcp != s.port || dstp != s.local_port {
            return TcpEvent::invalid();
        }
        let seq = u32::from_be_bytes([frame[off + 4], frame[off + 5], frame[off + 6], frame[off + 7]]);
        let ack_num = u32::from_be_bytes([frame[off + 8], frame[off + 9], frame[off + 10], frame[off + 11]]);
        let data_off = ((frame[off + 12] >> 4) as usize) * 4;
        let flags = frame[off + 13];
        let payload_off = off + data_off;
        if payload_off > frame.len() {
            return TcpEvent::invalid();
        }
        let payload = &frame[payload_off..];
        TcpEvent {
            valid: true,
            ack: (flags & 0x10) != 0,
            fin: (flags & 0x01) != 0,
            seq,
            ack_num,
            payload_len: payload.len(),
            payload,
        }
    }

    fn dns_lookup_a(&mut self, host: &str) -> Option<Ipv4Addr> {
        if host.is_empty() {
            return None;
        }
        if !self.iface.up || !rtl8139::is_ready() {
            return None;
        }

        let dns_ip = Ipv4Addr::new(10, 0, 2, 3);
        let hop = self.next_hop(dns_ip);
        let dst_mac = self.arp_resolve(hop)?;

        let src_port = 53000u16;
        let txid = self.next_ident();

        let mut dns = [0u8; 512];
        dns[0..2].copy_from_slice(&txid.to_be_bytes());
        dns[2..4].copy_from_slice(&0x0100u16.to_be_bytes());
        dns[4..6].copy_from_slice(&1u16.to_be_bytes());
        dns[6..8].copy_from_slice(&0u16.to_be_bytes());
        dns[8..10].copy_from_slice(&0u16.to_be_bytes());
        dns[10..12].copy_from_slice(&0u16.to_be_bytes());

        let mut dlen = 12usize;
        for label in host.split('.') {
            if label.is_empty() || label.len() > 63 {
                return None;
            }
            if dlen + 1 + label.len() + 4 > dns.len() {
                return None;
            }
            dns[dlen] = label.len() as u8;
            dlen += 1;
            dns[dlen..dlen + label.len()].copy_from_slice(label.as_bytes());
            dlen += label.len();
        }
        if dlen + 5 > dns.len() {
            return None;
        }
        dns[dlen] = 0;
        dlen += 1;
        dns[dlen..dlen + 2].copy_from_slice(&1u16.to_be_bytes());
        dlen += 2;
        dns[dlen..dlen + 2].copy_from_slice(&1u16.to_be_bytes());
        dlen += 2;

        let udp_len = (8 + dlen) as u16;
        let ip_len = 20u16 + udp_len;
        let mut pkt = [0u8; 1600];
        let src_mac = rtl8139::mac_addr();
        build_eth_header(&mut pkt[0..14], dst_mac, src_mac, 0x0800);
        build_ipv4_header(&mut pkt[14..34], self.iface.addr, dns_ip, 17, ip_len, self.next_ident());
        build_udp_header(&mut pkt[34..42], src_port, 53, udp_len);
        pkt[42..42 + dlen].copy_from_slice(&dns[..dlen]);

        if rtl8139::send_frame(&pkt[..42 + dlen]).is_err() {
            return None;
        }

        let mut rx = [0u8; 1600];
        for _ in 0..400_000u32 {
            let Some(n) = Self::recv_frame_with_wait(&mut rx, 1) else {
                continue;
            };
            if n < 42 {
                continue;
            }
            let eth = u16::from_be_bytes([rx[12], rx[13]]);
            if eth == 0x0806 {
                self.learn_arp(&rx[..n]);
                continue;
            }
            if eth != 0x0800 || rx[23] != 17 {
                continue;
            }
            if rx[26..30] != dns_ip.octets() || rx[30..34] != self.iface.addr.octets() {
                continue;
            }
            let ip_hlen = ((rx[14] & 0x0F) as usize) * 4;
            let uoff = 14 + ip_hlen;
            if n < uoff + 8 {
                continue;
            }
            let srcp = u16::from_be_bytes([rx[uoff], rx[uoff + 1]]);
            let dstp = u16::from_be_bytes([rx[uoff + 2], rx[uoff + 3]]);
            if srcp != 53 || dstp != src_port {
                continue;
            }
            let udp_len = u16::from_be_bytes([rx[uoff + 4], rx[uoff + 5]]) as usize;
            if udp_len < 8 || n < uoff + udp_len {
                continue;
            }
            let msg = &rx[uoff + 8..uoff + udp_len];
            if msg.len() < 12 {
                continue;
            }
            let rid = u16::from_be_bytes([msg[0], msg[1]]);
            if rid != txid {
                continue;
            }
            let flags = u16::from_be_bytes([msg[2], msg[3]]);
            if (flags & 0x8000) == 0 {
                continue;
            }
            let qdcount = u16::from_be_bytes([msg[4], msg[5]]) as usize;
            let ancount = u16::from_be_bytes([msg[6], msg[7]]) as usize;
            let mut off = 12usize;

            for _ in 0..qdcount {
                off = skip_dns_name(msg, off)?;
                if off + 4 > msg.len() {
                    return None;
                }
                off += 4;
            }
            for _ in 0..ancount {
                off = skip_dns_name(msg, off)?;
                if off + 10 > msg.len() {
                    return None;
                }
                let rr_type = u16::from_be_bytes([msg[off], msg[off + 1]]);
                let rr_class = u16::from_be_bytes([msg[off + 2], msg[off + 3]]);
                let rdlen = u16::from_be_bytes([msg[off + 8], msg[off + 9]]) as usize;
                off += 10;
                if off + rdlen > msg.len() {
                    return None;
                }
                if rr_type == 1 && rr_class == 1 && rdlen == 4 {
                    return Some(Ipv4Addr::new(msg[off], msg[off + 1], msg[off + 2], msg[off + 3]));
                }
                off += rdlen;
            }
        }
        None
    }

    fn recv_frame_with_wait(buf: &mut [u8], tries: u32) -> Option<usize> {
        for _ in 0..tries {
            if let Some(n) = rtl8139::recv_frame(buf) {
                return Some(n);
            }
            for _ in 0..256 {
                core::hint::spin_loop();
            }
        }
        None
    }
}

struct TcpEvent<'a> {
    valid: bool,
    ack: bool,
    fin: bool,
    seq: u32,
    ack_num: u32,
    payload_len: usize,
    payload: &'a [u8],
}

impl<'a> TcpEvent<'a> {
    fn invalid() -> Self {
        Self {
            valid: false,
            ack: false,
            fin: false,
            seq: 0,
            ack_num: 0,
            payload_len: 0,
            payload: &[],
        }
    }
}

pub fn tcp_probe_status_str(st: TcpProbeStatus) -> &'static str {
    match st {
        TcpProbeStatus::SynAck => "syn-ack",
        TcpProbeStatus::Rst => "rst",
        TcpProbeStatus::Timeout => "timeout",
        TcpProbeStatus::ArpFail => "arp-fail",
        TcpProbeStatus::LinkDown => "link-down",
        TcpProbeStatus::TxFail => "tx-fail",
    }
}

fn build_eth_header(buf: &mut [u8], dst: [u8; 6], src: [u8; 6], ether_type: u16) {
    buf[0..6].copy_from_slice(&dst);
    buf[6..12].copy_from_slice(&src);
    buf[12..14].copy_from_slice(&ether_type.to_be_bytes());
}

fn build_ipv4_header(
    buf: &mut [u8],
    src: Ipv4Addr,
    dst: Ipv4Addr,
    proto: u8,
    total_len: u16,
    ident: u16,
) {
    buf[0] = 0x45;
    buf[1] = 0;
    buf[2..4].copy_from_slice(&total_len.to_be_bytes());
    buf[4..6].copy_from_slice(&ident.to_be_bytes());
    buf[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    buf[8] = 64;
    buf[9] = proto;
    buf[10] = 0;
    buf[11] = 0;
    buf[12..16].copy_from_slice(&src.octets());
    buf[16..20].copy_from_slice(&dst.octets());
    let csum = checksum16(&buf[0..20]);
    buf[10..12].copy_from_slice(&csum.to_be_bytes());
}

fn build_tcp_header(
    buf: &mut [u8],
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) {
    buf[0..2].copy_from_slice(&src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&dst_port.to_be_bytes());
    buf[4..8].copy_from_slice(&seq.to_be_bytes());
    buf[8..12].copy_from_slice(&ack.to_be_bytes());
    buf[12] = 5 << 4;
    buf[13] = flags;
    buf[14..16].copy_from_slice(&window.to_be_bytes());
    buf[16] = 0;
    buf[17] = 0;
    buf[18] = 0;
    buf[19] = 0;

    let tcp_len = 20 + payload.len();
    let mut pseudo = [0u8; 64];
    pseudo[0..4].copy_from_slice(&src_ip.octets());
    pseudo[4..8].copy_from_slice(&dst_ip.octets());
    pseudo[8] = 0;
    pseudo[9] = 6;
    pseudo[10..12].copy_from_slice(&(tcp_len as u16).to_be_bytes());
    pseudo[12..32].copy_from_slice(&buf[0..20]);
    if !payload.is_empty() {
        pseudo[32..32 + payload.len()].copy_from_slice(payload);
    }
    let csum = checksum16(&pseudo[..12 + tcp_len]);
    buf[16..18].copy_from_slice(&csum.to_be_bytes());
}

fn build_udp_header(buf: &mut [u8], src_port: u16, dst_port: u16, len: u16) {
    buf[0..2].copy_from_slice(&src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&dst_port.to_be_bytes());
    buf[4..6].copy_from_slice(&len.to_be_bytes());
    buf[6..8].copy_from_slice(&0u16.to_be_bytes());
}

fn skip_dns_name(msg: &[u8], start: usize) -> Option<usize> {
    let mut off = start;
    let mut steps = 0usize;
    while off < msg.len() && steps < 128 {
        let len = msg[off];
        if len == 0 {
            return Some(off + 1);
        }
        if (len & 0xC0) == 0xC0 {
            if off + 1 >= msg.len() {
                return None;
            }
            return Some(off + 2);
        }
        let l = len as usize;
        off += 1;
        if off + l > msg.len() {
            return None;
        }
        off += l;
        steps += 1;
    }
    None
}

fn checksum16(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let word = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < bytes.len() {
        sum = sum.wrapping_add((bytes[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
