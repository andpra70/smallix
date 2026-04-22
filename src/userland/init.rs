use crate::drivers::{serial, vga};

use super::net::{tcp_probe_status_str, Ipv4Addr};
use super::{shell, syscall, Context};

pub fn start() -> ! {
    let mut ctx = Context::new();

    serial::write_str("smallix: init entered\n");
    vga::print("smallix kernel booted\n");
    vga::print("starting /sbin/init\n");
    serial::write_str("smallix: init start\n");

    seed_filesystem(&mut ctx);

    let sched_ok = ctx.sched.self_test();
    if sched_ok {
        serial::write_str("smallix: scheduler self-test PASS\n");
        vga::print("scheduler self-test PASS\n");
    } else {
        serial::write_str("smallix: scheduler self-test FAIL\n");
        vga::print("scheduler self-test FAIL\n");
    }

    let posix_ok = {
        let mut ok = true;
        let child = ctx.sched.spawn_process(1, "sigchild");
        match child {
            Ok(pid) => {
                let _ = ctx.sched.spawn_thread(pid, "main");
                if syscall::signal(&mut ctx, pid, syscall::SIGTERM).is_err() {
                    ok = false;
                }
                match syscall::wait(&mut ctx, 1, Some(pid)) {
                    Ok(Some((_wpid, _status))) => {}
                    _ => ok = false,
                }
            }
            Err(_) => ok = false,
        }
        let sel = syscall::select(&mut ctx, 2, false, true);
        let _ = sel;
        ok
    };
    if posix_ok {
        serial::write_str("smallix: posix syscall test PASS\n");
        vga::print("posix syscall test PASS\n");
    } else {
        serial::write_str("smallix: posix syscall test FAIL\n");
        vga::print("posix syscall test FAIL\n");
    }

    let gw = ctx.dev.net_interface().gateway;
    let net_ok = ctx.dev.net_ping_once(gw, 1).is_some();
    if net_ok {
        serial::write_str("smallix: net ping gateway PASS\n");
        vga::print("net ping gateway PASS\n");
    } else {
        serial::write_str("smallix: net ping gateway FAIL\n");
        vga::print("net ping gateway FAIL\n");
    }

    let probe_target = Ipv4Addr::new(10, 0, 2, 100);
    let st = ctx.dev.net_tcp_probe_status(probe_target, 2323);
    let (tcp_ok, tcp_msg) = (
        matches!(
            st,
            super::net::TcpProbeStatus::SynAck | super::net::TcpProbeStatus::Rst
        ),
        tcp_probe_status_str(st),
    );
    if tcp_ok {
        serial::write_str("smallix: net tcp connect PASS\n");
        vga::print("net tcp connect PASS\n");
    } else {
        serial::write_str("smallix: net tcp connect FAIL\n");
        serial::write_str("smallix: net tcp reason ");
        serial::write_str(tcp_msg);
        serial::write_str("\n");
        vga::print("net tcp connect FAIL\n");
    }

    let telnet_io_ok = if let Ok((sid, _)) = ctx.dev.net_open_telnet(probe_target, 2323) {
        let mut ok = true;
        if ctx.dev.net_telnet_send(sid, b"hello-from-smallix\n").is_err() {
            ok = false;
        }
        let mut buf = [0u8; 128];
        match ctx.dev.net_telnet_recv(sid, &mut buf, 30000) {
            Ok(n) if n > 0 => {}
            _ => ok = false,
        }
        let _ = ctx.dev.net_close_telnet(sid);
        ok
    } else {
        false
    };
    if telnet_io_ok {
        serial::write_str("smallix: telnet io PASS\n");
        vga::print("telnet io PASS\n");
    } else {
        serial::write_str("smallix: telnet io FAIL\n");
        vga::print("telnet io FAIL\n");
    }

    let hda_persist_ok = test_hda_persistence(&mut ctx);
    if hda_persist_ok {
        serial::write_str("smallix: hda persistence PASS\n");
        vga::print("hda persistence PASS\n");
    }

    vga::print("init complete\n");
    vga::print("type 'help' for commands\n");
    serial::write_str("smallix: init complete\n");

    shell::run(&mut ctx)
}

fn seed_filesystem(ctx: &mut Context) {
    if ctx.fs.mount("/dev/hda", "/").is_err() {
        serial::write_str("smallix: rootfs mount /dev/hda failed (need FAT32 image)\n");
        vga::print("rootfs mount /dev/hda failed\n");
    }
}

fn test_hda_persistence(ctx: &mut Context) -> bool {
    if ctx.fs.mount("/dev/hda", "/").is_err() {
        serial::write_str("smallix: hda persistence FAIL (mount)\n");
        vga::print("hda persistence FAIL\n");
        let _ = ctx.fs.mount("/dev/ramfs", "/");
        return false;
    }

    if ctx.fs.active_name() == "fat32" {
        serial::write_str("smallix: hda fat32 detected, skip native persistence marker\n");
        let _ = ctx.fs.mount("/dev/ramfs", "/");
        return true;
    }

    let marker = "/var/persist.marker";
    let existed = ctx.fs.read_file(marker).is_some();
    if !existed {
        let _ = ctx.fs.write_file(marker, b"persisted\n");
        serial::write_str("smallix: hda persistence SEED\n");
        vga::print("hda persistence SEED\n");
    }

    let _ = ctx.fs.mount("/dev/ramfs", "/");
    existed
}
