use crate::userland::net::Ipv4Addr;
use crate::userland::{shell, Context};
const DEV_NET: &str = "/dev/net0";

pub fn ifconfig(ctx: &mut Context, args: &str) {
    if !crate::userland::dev::DevFs::exists(DEV_NET) {
        shell::println("network device not present");
        return;
    }

    if args.is_empty() || args == "show" {
        let iface = ctx.dev.net_interface();
        shell::println_fmt(format_args!(
            "{}: {}",
            iface.name,
            if iface.up { "UP" } else { "DOWN" }
        ));
        shell::println_fmt(format_args!("  inet {} netmask {}", iface.addr, iface.mask));
        shell::println_fmt(format_args!("  gateway {}", iface.gateway));
        shell::println_fmt(format_args!(
            "  ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            iface.mac[0], iface.mac[1], iface.mac[2], iface.mac[3], iface.mac[4], iface.mac[5]
        ));
        return;
    }

    if args == "up" {
        ctx.dev.net_set_link_state(true);
        shell::println("eth0 set UP");
        return;
    }

    if args == "down" {
        ctx.dev.net_set_link_state(false);
        shell::println("eth0 set DOWN");
        return;
    }

    let mut parts = args.split_whitespace();
    let cmd = parts.next();
    let ip = parts.next();
    let mask = parts.next();
    let gw = parts.next();

    if cmd == Some("set") && ip.is_some() && mask.is_some() && gw.is_some() && parts.next().is_none() {
        let addr = match Ipv4Addr::parse(ip.unwrap_or("")) {
            Some(v) => v,
            None => {
                shell::println("invalid ipv4 address");
                return;
            }
        };
        let netmask = match Ipv4Addr::parse(mask.unwrap_or("")) {
            Some(v) => v,
            None => {
                shell::println("invalid netmask");
                return;
            }
        };
        let gateway = match Ipv4Addr::parse(gw.unwrap_or("")) {
            Some(v) => v,
            None => {
                shell::println("invalid gateway");
                return;
            }
        };

        ctx.dev.net_set_interface(addr, netmask, gateway);
        shell::println("eth0 updated");
        return;
    }

    shell::println("usage: ifconfig [show|up|down|set <ip> <mask> <gw>]");
}

pub fn route(ctx: &mut Context, args: &str) {
    if args.is_empty() || args == "show" {
        let iface = ctx.dev.net_interface();
        shell::println("Destination     Gateway         Netmask         Iface");
        shell::println_fmt(format_args!("0.0.0.0         {}      0.0.0.0         {}", iface.gateway, iface.name));
        shell::println_fmt(format_args!("{}         0.0.0.0         {}      {}", iface.addr, iface.mask, iface.name));
        return;
    }

    let mut parts = args.split_whitespace();
    if parts.next() == Some("set-gw") {
        let gw = match parts.next() {
            Some(v) => v,
            None => {
                shell::println("usage: route set-gw <ip>");
                return;
            }
        };
        if parts.next().is_some() {
            shell::println("usage: route set-gw <ip>");
            return;
        }

        let gateway = match Ipv4Addr::parse(gw) {
            Some(v) => v,
            None => {
                shell::println("invalid gateway");
                return;
            }
        };

        ctx.dev.net_set_gateway(gateway);
        shell::println("default gateway updated");
        return;
    }

    shell::println("usage: route [show|set-gw <ip>]");
}

pub fn ping(ctx: &mut Context, args: &str) {
    let mut parts = args.split_whitespace();
    let host = match parts.next() {
        Some(h) => h,
        None => {
            shell::println("usage: ping <host|ip> [count]");
            return;
        }
    };

    let count = match parts.next() {
        Some(n) => n.parse::<u8>().ok().filter(|v| *v > 0 && *v <= 10).unwrap_or(4),
        None => 4,
    };

    let target = match ctx.dev.net_resolve_host(host) {
        Some(ip) => ip,
        None => {
            shell::println("unknown host");
            return;
        }
    };

    shell::println_fmt(format_args!("PING {} ({})", host, target));

    let mut received = 0u8;
    for seq in 1..=count {
        match ctx.dev.net_ping_once(target, seq) {
            Some(ms) => {
                received = received.saturating_add(1);
                shell::println_fmt(format_args!(
                    "64 bytes from {}: icmp_seq={} ttl=64 time={} ms",
                    target, seq, ms
                ));
            }
            None => {
                shell::println_fmt(format_args!("request timeout for icmp_seq={}", seq));
            }
        }
    }

    shell::println_fmt(format_args!(
        "--- {} ping statistics ---",
        host
    ));
    shell::println_fmt(format_args!(
        "{} packets transmitted, {} received",
        count, received
    ));
}

pub fn telnet(ctx: &mut Context, args: &str) {
    if args.is_empty() {
        shell::println("usage: telnet <host> [port] | telnet send <id> <text> | telnet recv <id> | telnet close <id>");
        return;
    }

    if let Some(rest) = args.strip_prefix("send ") {
        let Some((id_s, text)) = rest.split_once(' ') else {
            shell::println("usage: telnet send <id> <text>");
            return;
        };
        let Ok(id) = id_s.parse::<u8>() else {
            shell::println("invalid id");
            return;
        };
        if text.is_empty() {
            shell::println("usage: telnet send <id> <text>");
            return;
        }
        match ctx.dev.net_telnet_send(id, text.as_bytes()) {
            Ok(n) => shell::println_fmt(format_args!("sent {} bytes", n)),
            Err(e) => shell::println(e),
        }
        return;
    }

    if let Some(rest) = args.strip_prefix("recv ") {
        let id_s = rest.trim();
        if id_s.is_empty() || id_s.contains(' ') {
            shell::println("usage: telnet recv <id>");
            return;
        }
        let Ok(id) = id_s.parse::<u8>() else {
            shell::println("invalid id");
            return;
        };
        let mut buf = [0u8; 256];
        match ctx.dev.net_telnet_recv(id, &mut buf, 20000) {
            Ok(0) => shell::println("no data"),
            Ok(n) => match core::str::from_utf8(&buf[..n]) {
                Ok(s) => shell::println_fmt(format_args!("{}", s)),
                Err(_) => shell::println("binary data"),
            },
            Err(e) => shell::println(e),
        }
        return;
    }

    if let Some(rest) = args.strip_prefix("close ") {
        let id = match rest.trim().parse::<u8>().ok() {
            Some(v) => v,
            None => {
                shell::println("usage: telnet close <id>");
                return;
            }
        };
        match ctx.dev.net_close_telnet(id) {
            Ok(()) => shell::println("session closed"),
            Err(e) => shell::println(e),
        }
        return;
    }

    let mut parts = args.split_whitespace();
    let host = match parts.next() {
        Some(v) => v,
        None => {
            shell::println("usage: telnet <host> [port]");
            return;
        }
    };

    let port = parts
        .next()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(23);

    let remote = match ctx.dev.net_resolve_host(host) {
        Some(ip) => ip,
        None => {
            shell::println("unknown host");
            return;
        }
    };

    match ctx.dev.net_open_telnet(remote, port) {
        Ok((id, banner)) => {
            shell::println_fmt(format_args!("Connected to {}:{} (session #{})", remote, port, id));
            shell::println_fmt(format_args!("{}", banner));
            shell::println("Use 'telnet send <id> <text>' / 'telnet recv <id>' / 'telnet close <id>'");
        }
        Err(e) => shell::println(e),
    }
}

pub fn netstat(ctx: &mut Context, _args: &str) {
    let stats = ctx.dev.net_stats();
    shell::println("netstat -s");
    shell::println_fmt(format_args!(
        "  tx={} rx={} drop={} icmp_tx={} icmp_rx={}",
        stats.tx_packets, stats.rx_packets, stats.dropped_packets, stats.icmp_tx, stats.icmp_rx
    ));

    shell::println("Active telnet sessions:");
    let sessions = ctx.dev.net_sessions();
    let mut any = false;
    for session in sessions {
        if !session.active {
            continue;
        }
        any = true;
        shell::println_fmt(format_args!(
            "  #{} {}:{} tx={}B rx={}B ESTABLISHED",
            session.id, session.remote, session.port, session.tx_bytes, session.rx_bytes
        ));
    }

    if !any {
        shell::println("  (none)");
    }
}
