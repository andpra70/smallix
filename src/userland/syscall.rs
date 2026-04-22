use crate::arch::inb;

use super::Context;

pub const SIGTERM: u8 = 15;
pub const SIGKILL: u8 = 9;
pub const SIGUSR1: u8 = 10;
pub const SIGCHLD: u8 = 17;

pub const FD_STDIN: u8 = 0;
pub const FD_NET0: u8 = 1;

pub fn signal(ctx: &mut Context, pid: u16, sig: u8) -> Result<(), &'static str> {
    ctx.sched.send_signal(pid, sig)
}

pub fn wait(ctx: &mut Context, parent_pid: u16, wanted_pid: Option<u16>) -> Result<Option<(u16, i32)>, &'static str> {
    ctx.sched.wait_child(parent_pid, wanted_pid)
}

pub fn select(ctx: &mut Context, timeout_ticks: u32, watch_stdin: bool, watch_net0: bool) -> u8 {
    let mut elapsed = 0u32;

    loop {
        let mut ready = 0u8;

        if watch_stdin && (inb(0x64) & 0x01) != 0 {
            ready |= 1 << FD_STDIN;
        }

        if watch_net0 && ctx.dev.net_frame_ready() {
            ready |= 1 << FD_NET0;
        }

        if ready != 0 {
            return ready;
        }

        if elapsed >= timeout_ticks {
            return 0;
        }

        ctx.sched.tick();
        elapsed = elapsed.saturating_add(1);
    }
}
