use crate::userland::sched::{ProcState, ThreadState};
use crate::userland::{shell, Context};

const PROC_ROOT_ENTRIES: [&str; 9] = [
    "version",
    "uptime",
    "hostname",
    "mounts",
    "meminfo",
    "sched",
    "devices",
    "net",
    "config",
];

const PROC_NET_ENTRIES: [&str; 2] = ["dev", "route"];
const PROC_CONFIG_ENTRIES: [&str; 4] = ["system", "init", "network", "scheduler"];

pub fn is_proc_path(path: &str) -> bool {
    path == "/proc" || path.starts_with("/proc/")
}

pub fn is_proc_dir(path: &str) -> bool {
    matches!(path, "/proc" | "/proc/net" | "/proc/config") || parse_pid_dir(path).is_some()
}

fn is_proc_file(path: &str) -> bool {
    if matches!(
        path,
        "/proc/version"
            | "/proc/uptime"
            | "/proc/hostname"
            | "/proc/mounts"
            | "/proc/meminfo"
            | "/proc/sched"
            | "/proc/devices"
            | "/proc/net/dev"
            | "/proc/net/route"
            | "/proc/config/system"
            | "/proc/config/init"
            | "/proc/config/network"
            | "/proc/config/scheduler"
    ) {
        return true;
    }
    parse_pid_status(path).is_some() || parse_pid_threads(path).is_some()
}

pub fn list_dir<F: FnMut(&str)>(ctx: &Context, path: &str, mut cb: F) -> Result<(), &'static str> {
    if path == "/proc" {
        for e in PROC_ROOT_ENTRIES {
            cb(e);
        }
        let mut pids = [0u16; 64];
        let mut count = 0usize;
        for p in ctx.sched.proc_iter().map(|p| p.pid) {
            if count < pids.len() {
                pids[count] = p;
                count += 1;
            }
        }
        for pid in &pids[..count] {
            let mut buf = [0u8; 8];
            let n = u16_to_ascii(*pid, &mut buf);
            if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                cb(s);
            }
        }
        return Ok(());
    }
    if path == "/proc/net" {
        for e in PROC_NET_ENTRIES {
            cb(e);
        }
        return Ok(());
    }
    if path == "/proc/config" {
        for e in PROC_CONFIG_ENTRIES {
            cb(e);
        }
        return Ok(());
    }
    if parse_pid_dir(path).is_some() {
        cb("status");
        cb("threads");
        return Ok(());
    }
    if is_proc_file(path) {
        cb(path.rsplit('/').next().unwrap_or(path));
        return Ok(());
    }
    Err("no such file or directory")
}

pub fn cat(ctx: &Context, path: &str) -> Result<(), &'static str> {
    match path {
        "/proc/version" => {
            shell::println("Smallix 0.2.0 i686");
            Ok(())
        }
        "/proc/uptime" => {
            shell::println_fmt(format_args!("{} 0", ctx.sched.ticks()));
            Ok(())
        }
        "/proc/hostname" => {
            shell::println(ctx.hostname);
            Ok(())
        }
        "/proc/mounts" => {
            shell::println(ctx.fs.mtab_line());
            Ok(())
        }
        "/proc/meminfo" => {
            let mut files = 0usize;
            let mut used = 0usize;
            ctx.fs.list_paths(|p| {
                files = files.saturating_add(1);
                if let Some(data) = ctx.fs.read_file(p) {
                    used = used.saturating_add(data.len());
                }
            });
            shell::println("MemTotal: 262144 kB");
            shell::println_fmt(format_args!("MemFree:  {} kB", 262144usize.saturating_sub(used / 1024)));
            shell::println_fmt(format_args!("FsFiles:  {}", files));
            shell::println_fmt(format_args!("FsBytes:  {}", used));
            Ok(())
        }
        "/proc/sched" => {
            let procs = ctx.sched.proc_iter().count();
            let threads = ctx.sched.thread_iter().count();
            shell::println_fmt(format_args!("ticks={}", ctx.sched.ticks()));
            shell::println_fmt(format_args!("processes={}", procs));
            shell::println_fmt(format_args!("threads={}", threads));
            Ok(())
        }
        "/proc/devices" => {
            ctx.dev.list_devices(|d| shell::println(d));
            Ok(())
        }
        "/proc/net/dev" => {
            let iface = ctx.dev.net_interface();
            let st = ctx.dev.net_stats();
            shell::println("Iface    RX-packets  TX-packets  Dropped");
            shell::println_fmt(format_args!(
                "{:<8} {:<11} {:<11} {}",
                iface.name, st.rx_packets, st.tx_packets, st.dropped_packets
            ));
            Ok(())
        }
        "/proc/net/route" => {
            let iface = ctx.dev.net_interface();
            shell::println("Iface    Destination  Gateway      Mask");
            shell::println_fmt(format_args!(
                "{:<8} {:<12} {:<12} {}",
                iface.name, "0.0.0.0", iface.gateway, iface.mask
            ));
            Ok(())
        }
        "/proc/config/system" => {
            print_fs_text(ctx, "/etc/system.cfg", "config missing");
            Ok(())
        }
        "/proc/config/init" => {
            print_fs_text(ctx, "/etc/init.rc", "config missing");
            Ok(())
        }
        "/proc/config/network" => {
            print_fs_text(ctx, "/etc/net.cfg", "config missing");
            Ok(())
        }
        "/proc/config/scheduler" => {
            print_fs_text(ctx, "/etc/sched.cfg", "config missing");
            Ok(())
        }
        _ => {
            if let Some(pid) = parse_pid_status(path) {
                return cat_pid_status(ctx, pid);
            }
            if let Some(pid) = parse_pid_threads(path) {
                return cat_pid_threads(ctx, pid);
            }
            if parse_pid_dir(path).is_some() {
                return Err("is a directory");
            }
            if matches!(path, "/proc" | "/proc/net" | "/proc/config") {
                return Err("is a directory");
            }
            Err("no such file or directory")
        }
    }
}

fn cat_pid_status(ctx: &Context, pid: u16) -> Result<(), &'static str> {
    let Some(proc) = ctx.sched.proc_iter().find(|p| p.pid == pid) else {
        return Err("no such process");
    };
    let name = core::str::from_utf8(&proc.name[..proc.name_len]).unwrap_or("?");
    let threads = ctx.sched.thread_iter().filter(|t| t.pid == pid).count();
    shell::println_fmt(format_args!("Name:\t{}", name));
    shell::println_fmt(format_args!("Pid:\t{}", proc.pid));
    shell::println_fmt(format_args!("PPid:\t{}", proc.ppid));
    shell::println_fmt(format_args!("State:\t{}", proc_state_name(proc.state)));
    shell::println_fmt(format_args!("Threads:\t{}", threads));
    shell::println_fmt(format_args!("Signals:\t0x{:08x}", proc.pending_signals));
    shell::println_fmt(format_args!("ExitCode:\t{}", proc.exit_code));
    Ok(())
}

fn cat_pid_threads(ctx: &Context, pid: u16) -> Result<(), &'static str> {
    if ctx.sched.proc_iter().find(|p| p.pid == pid).is_none() {
        return Err("no such process");
    }
    shell::println("TID STATE    TICKS NAME");
    let mut found = false;
    for t in ctx.sched.thread_iter().filter(|t| t.pid == pid) {
        found = true;
        let name = core::str::from_utf8(&t.name[..t.name_len]).unwrap_or("?");
        shell::println_fmt(format_args!(
            "{:<3} {:<8} {:<5} {}",
            t.tid,
            thread_state_name(t.state),
            t.runtime_ticks,
            name
        ));
    }
    if !found {
        shell::println("(none)");
    }
    Ok(())
}

fn proc_state_name(s: ProcState) -> &'static str {
    match s {
        ProcState::Running => "running",
        ProcState::Ready => "ready",
        ProcState::Zombie => "zombie",
    }
}

fn thread_state_name(s: ThreadState) -> &'static str {
    match s {
        ThreadState::Running => "running",
        ThreadState::Ready => "ready",
        ThreadState::Blocked => "blocked",
        ThreadState::Zombie => "zombie",
    }
}

fn parse_pid_dir(path: &str) -> Option<u16> {
    let rest = path.strip_prefix("/proc/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    parse_u16(rest)
}

fn parse_pid_status(path: &str) -> Option<u16> {
    let rest = path.strip_prefix("/proc/")?;
    let (pid_s, leaf) = rest.split_once('/')?;
    if leaf != "status" {
        return None;
    }
    parse_u16(pid_s)
}

fn parse_pid_threads(path: &str) -> Option<u16> {
    let rest = path.strip_prefix("/proc/")?;
    let (pid_s, leaf) = rest.split_once('/')?;
    if leaf != "threads" {
        return None;
    }
    parse_u16(pid_s)
}

fn parse_u16(s: &str) -> Option<u16> {
    if s.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        v = v.saturating_mul(10).saturating_add((b - b'0') as u32);
        if v > u16::MAX as u32 {
            return None;
        }
    }
    Some(v as u16)
}

fn u16_to_ascii(mut v: u16, out: &mut [u8; 8]) -> usize {
    if v == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 8];
    let mut n = 0usize;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    let mut i = 0usize;
    while i < n {
        out[i] = tmp[n - 1 - i];
        i += 1;
    }
    n
}

fn print_fs_text(ctx: &Context, path: &str, fallback: &str) {
    let Some(data) = ctx.fs.read_file(path) else {
        shell::println(fallback);
        return;
    };
    let Ok(text) = core::str::from_utf8(data) else {
        shell::println("binary content");
        return;
    };
    shell::println(text);
}
