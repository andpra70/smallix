use crate::userland::sched::{name_str, proc_state_str, thread_state_str};
use crate::userland::syscall;
use crate::userland::exec;
use crate::userland::{shell, Context};

pub fn ps(ctx: &mut Context, _args: &str) {
    shell::println("PID  PPID STATE NAME");
    for p in ctx.sched.proc_iter() {
        shell::println_fmt(format_args!(
            "{:<4} {:<4} {:<5} {}",
            p.pid,
            p.ppid,
            proc_state_str(p.state),
            name_str(&p.name, p.name_len)
        ));
    }
}

pub fn threads(ctx: &mut Context, _args: &str) {
    shell::println("TID  PID  STATE TICKS NAME");
    for t in ctx.sched.thread_iter() {
        shell::println_fmt(format_args!(
            "{:<4} {:<4} {:<5} {:<5} {}",
            t.tid,
            t.pid,
            thread_state_str(t.state),
            t.runtime_ticks,
            name_str(&t.name, t.name_len)
        ));
    }
}

pub fn fork(ctx: &mut Context, args: &str) {
    if args.is_empty() {
        shell::println("usage: fork <proc_name> [exec_path [args]]");
        return;
    }

    let mut parts = args.split_whitespace();
    let name = parts.next().unwrap_or("");
    let exec_path = parts.next();
    let rest = if let Some(p) = exec_path {
        args.split_once(p).map(|(_, r)| r.trim_start()).unwrap_or("")
    } else {
        ""
    };

    match ctx.sched.spawn_process(1, name) {
        Ok(pid) => {
            let _ = ctx.sched.spawn_thread(pid, "main");
            if let Some(path) = exec_path {
                match exec::exec_from_path(ctx, path, rest) {
                    Ok(code) => {
                        let _ = ctx.sched.exit_process(pid, code);
                        shell::println_fmt(format_args!("fork+exec pid={} exit={}", pid, code));
                    }
                    Err(e) => {
                        ctx.set_errno(e);
                        let _ = ctx.sched.exit_process(pid, 256 + e);
                        shell::println_fmt(format_args!("fork+exec pid={} failed errno={}", pid, e));
                    }
                }
            } else {
                shell::println_fmt(format_args!("forked pid={}", pid));
            }
        }
        Err(e) => {
            ctx.set_errno(exec::ENOMEM);
            shell::println(e)
        }
    }
}

pub fn pthread(ctx: &mut Context, args: &str) {
    let Some((pid_s, tname)) = args.split_once(' ') else {
        shell::println("usage: pthread <pid> <thread_name>");
        return;
    };

    let Ok(pid) = pid_s.parse::<u16>() else {
        shell::println("invalid pid");
        return;
    };

    match ctx.sched.spawn_thread(pid, tname) {
        Ok(tid) => shell::println_fmt(format_args!("spawned tid={} pid={}", tid, pid)),
        Err(e) => shell::println(e),
    }
}

pub fn kill(ctx: &mut Context, args: &str) {
    let Ok(pid) = args.parse::<u16>() else {
        shell::println("usage: kill <pid>");
        return;
    };

    match syscall::signal(ctx, pid, syscall::SIGTERM) {
        Ok(()) => shell::println("killed"),
        Err(e) => {
            ctx.set_errno(exec::EINVAL);
            shell::println(e)
        }
    }
}

pub fn exec_cmd(ctx: &mut Context, args: &str) {
    let Some((path, tail)) = split_path_tail(args) else {
        shell::println("usage: exec <path> [args]");
        ctx.set_errno(exec::EINVAL);
        return;
    };
    match exec::exec_from_path(ctx, path, tail) {
        Ok(code) => {
            ctx.last_exit_code = code;
            ctx.set_errno(0);
        }
        Err(e) => {
            ctx.set_errno(e);
            shell::println_fmt(format_args!("exec failed errno={}", e));
        }
    }
}

pub fn execve(ctx: &mut Context, args: &str) {
    let Some((path, tail)) = split_path_tail(args) else {
        shell::println("usage: execve <path> [args]");
        ctx.set_errno(exec::EINVAL);
        return;
    };
    match exec::exec_from_path(ctx, path, tail) {
        Ok(code) => {
            ctx.last_exit_code = code;
            ctx.set_errno(0);
            shell::println_fmt(format_args!("execve rc={}", code));
        }
        Err(e) => {
            ctx.set_errno(e);
            shell::println_fmt(format_args!("execve errno={}", e));
        }
    }
}

pub fn exit_cmd(ctx: &mut Context, args: &str) {
    let code = if args.trim().is_empty() {
        0
    } else {
        match args.trim().parse::<i32>() {
            Ok(v) => v,
            Err(_) => {
                ctx.set_errno(exec::EINVAL);
                shell::println("usage: exit [code]");
                return;
            }
        }
    };
    ctx.last_exit_code = code;
    ctx.set_errno(0);
    shell::println_fmt(format_args!("exit code={}", code));
}

pub fn errno_cmd(ctx: &mut Context, _args: &str) {
    shell::println_fmt(format_args!("errno={}", ctx.errno));
}

pub fn schedtick(ctx: &mut Context, args: &str) {
    let n = if args.is_empty() {
        1
    } else {
        args.parse::<u32>().ok().filter(|v| *v > 0 && *v <= 10000).unwrap_or(1)
    };

    ctx.sched.run_ticks(n);
    shell::println_fmt(format_args!("scheduler ticks={}", ctx.sched.ticks()));
}

pub fn schedtest(ctx: &mut Context, _args: &str) {
    let ok = ctx.sched.self_test();
    if ok {
        shell::println("SCHED TEST PASS");
    } else {
        shell::println("SCHED TEST FAIL");
    }
}

pub fn signal(ctx: &mut Context, args: &str) {
    let Some((pid_s, sig_s)) = args.split_once(' ') else {
        shell::println("usage: signal <pid> <sig>");
        return;
    };
    let Ok(pid) = pid_s.parse::<u16>() else {
        shell::println("invalid pid");
        return;
    };
    let Ok(sig) = sig_s.parse::<u8>() else {
        shell::println("invalid signal");
        return;
    };
    match syscall::signal(ctx, pid, sig) {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

pub fn wait(ctx: &mut Context, args: &str) {
    let wanted = if args.is_empty() {
        None
    } else {
        match args.parse::<u16>() {
            Ok(v) => Some(v),
            Err(_) => {
                shell::println("usage: wait [pid]");
                return;
            }
        }
    };

    match syscall::wait(ctx, 1, wanted) {
        Ok(Some((pid, status))) => {
            shell::println_fmt(format_args!("reaped pid={} status={}", pid, status));
        }
        Ok(None) => shell::println("no child exited"),
        Err(e) => shell::println(e),
    }
}

pub fn select(ctx: &mut Context, args: &str) {
    let timeout = if args.is_empty() {
        100u32
    } else {
        args.parse::<u32>().ok().unwrap_or(100)
    };

    let ready = syscall::select(ctx, timeout, true, true);
    shell::println_fmt(format_args!("select mask=0x{:02x}", ready));
}

fn split_path_tail(input: &str) -> Option<(&str, &str)> {
    let t = input.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(i) = t.find(' ') {
        Some((&t[..i], t[i + 1..].trim_start()))
    } else {
        Some((t, ""))
    }
}
