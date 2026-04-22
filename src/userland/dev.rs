use crate::drivers::{serial, vga};

use super::net::{Ipv4Addr, NetInterface, NetStack, NetStats, TcpProbeStatus, TelnetSession};

const DEVICES: [&str; 9] = [
    "/dev/console",
    "/dev/null",
    "/dev/kmsg",
    "/dev/net0",
    "/dev/tty0",
    "/dev/ramfs",
    "/dev/hda",
    "/dev/loop0",
    "/dev/usb0",
];

pub struct DevFs {
    net: NetStack,
}

impl DevFs {
    pub const fn new() -> Self {
        Self { net: NetStack::new() }
    }

    pub fn list_devices<F: FnMut(&str)>(&self, mut f: F) {
        for dev in DEVICES {
            f(dev);
        }
    }

    pub fn exists(path: &str) -> bool {
        DEVICES.contains(&path)
    }

    pub fn write(&mut self, path: &str, data: &str) -> Result<usize, &'static str> {
        match path {
            "/dev/console" | "/dev/tty0" => {
                vga::print(data);
                serial::write_str(data);
                Ok(data.len())
            }
            "/dev/kmsg" => {
                serial::write_str("[kmsg] ");
                serial::write_str(data);
                serial::write_str("\n");
                Ok(data.len())
            }
            "/dev/null" => Ok(data.len()),
            "/dev/net0" => Err("use network commands for /dev/net0"),
            "/dev/ramfs" | "/dev/hda" | "/dev/loop0" | "/dev/usb0" => Err("block device: use mount"),
            _ => Err("no such device"),
        }
    }

    pub fn read_text(&self, path: &str) -> Result<&'static str, &'static str> {
        match path {
            "/dev/null" => Ok(""),
            "/dev/console" | "/dev/tty0" => Ok("console device\n"),
            "/dev/kmsg" => Ok("kernel message sink\n"),
            "/dev/net0" => Ok("smallix net device\n"),
            "/dev/ramfs" => Ok("ramfs block device\n"),
            "/dev/hda" => Ok("hda block device (persistent)\n"),
            "/dev/loop0" => Ok("loop block device\n"),
            "/dev/usb0" => Ok("usb block device\n"),
            _ => Err("no such device"),
        }
    }

    pub fn net_interface(&self) -> NetInterface {
        self.net.interface()
    }

    pub fn net_set_interface(&mut self, addr: Ipv4Addr, mask: Ipv4Addr, gateway: Ipv4Addr) {
        self.net.set_interface(addr, mask, gateway);
    }

    pub fn net_set_gateway(&mut self, gateway: Ipv4Addr) {
        self.net.set_gateway(gateway);
    }

    pub fn net_set_link_state(&mut self, up: bool) {
        self.net.set_link_state(up);
    }

    pub fn net_resolve_host(&mut self, host: &str) -> Option<Ipv4Addr> {
        self.net.resolve_host(host)
    }

    pub fn net_ping_once(&mut self, target: Ipv4Addr, seq: u8) -> Option<u16> {
        self.net.ping_once(target, seq)
    }

    pub fn net_open_telnet(
        &mut self,
        remote: Ipv4Addr,
        port: u16,
    ) -> Result<(u8, &'static str), &'static str> {
        self.net.open_telnet(remote, port)
    }

    pub fn net_close_telnet(&mut self, id: u8) -> Result<(), &'static str> {
        self.net.close_telnet(id)
    }

    pub fn net_telnet_send(&mut self, id: u8, data: &[u8]) -> Result<usize, &'static str> {
        self.net.send_telnet_data(id, data)
    }

    pub fn net_telnet_recv(&mut self, id: u8, out: &mut [u8], spins: u32) -> Result<usize, &'static str> {
        self.net.recv_telnet_data(id, out, spins)
    }

    pub fn net_tcp_probe(&mut self, remote: Ipv4Addr, port: u16) -> bool {
        self.net.tcp_probe(remote, port)
    }

    pub fn net_tcp_probe_status(&mut self, remote: Ipv4Addr, port: u16) -> TcpProbeStatus {
        self.net.tcp_probe_status(remote, port)
    }

    pub fn net_stats(&self) -> NetStats {
        self.net.stats()
    }

    pub fn net_sessions(&self) -> [TelnetSession; 8] {
        self.net.sessions()
    }

    pub fn net_frame_ready(&self) -> bool {
        self.net.frame_ready()
    }

    pub fn net_nic_info(&self) -> Option<([u8; 6], u8)> {
        self.net.nic_info()
    }
}
