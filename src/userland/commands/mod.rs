mod builtin;
mod fs;
mod net;
mod proc;
mod sys;

use crate::userland::Context;

pub type CommandFn = fn(&mut Context, &str);

pub struct Command {
    pub name: &'static str,
    pub help: &'static str,
    pub run: CommandFn,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "help",
        help: "show command list",
        run: builtin::help,
    },
    Command {
        name: "echo",
        help: "echo <text>",
        run: builtin::echo,
    },
    Command {
        name: "clear",
        help: "clear screen",
        run: builtin::clear,
    },
    Command {
        name: "uname",
        help: "print system version",
        run: sys::uname,
    },
    Command {
        name: "lsdev",
        help: "list kernel devices",
        run: sys::lsdev,
    },
    Command {
        name: "cfg",
        help: "show key config files",
        run: sys::cfg,
    },
    Command {
        name: "halt",
        help: "halt cpu",
        run: sys::halt,
    },
    Command {
        name: "reboot",
        help: "reboot machine",
        run: sys::reboot,
    },
    Command {
        name: "ps",
        help: "list processes",
        run: proc::ps,
    },
    Command {
        name: "threads",
        help: "list threads",
        run: proc::threads,
    },
    Command {
        name: "fork",
        help: "fork <proc_name> [exec_path [args]]",
        run: proc::fork,
    },
    Command {
        name: "pthread",
        help: "pthread <pid> <thread_name>",
        run: proc::pthread,
    },
    Command {
        name: "kill",
        help: "kill <pid>",
        run: proc::kill,
    },
    Command {
        name: "exec",
        help: "exec <path> [args]",
        run: proc::exec_cmd,
    },
    Command {
        name: "execve",
        help: "execve <path> [args]",
        run: proc::execve,
    },
    Command {
        name: "exit",
        help: "exit [code]",
        run: proc::exit_cmd,
    },
    Command {
        name: "errno",
        help: "show last errno",
        run: proc::errno_cmd,
    },
    Command {
        name: "signal",
        help: "signal <pid> <sig>",
        run: proc::signal,
    },
    Command {
        name: "wait",
        help: "wait [pid]",
        run: proc::wait,
    },
    Command {
        name: "select",
        help: "select [timeout_ticks]",
        run: proc::select,
    },
    Command {
        name: "schedtick",
        help: "schedtick [n]",
        run: proc::schedtick,
    },
    Command {
        name: "schedtest",
        help: "run scheduler self-test",
        run: proc::schedtest,
    },
    Command {
        name: "ls",
        help: "ls [path]",
        run: fs::ls,
    },
    Command {
        name: "cd",
        help: "cd <dir>",
        run: fs::cd,
    },
    Command {
        name: "pwd",
        help: "print current directory",
        run: fs::pwd,
    },
    Command {
        name: "cat",
        help: "cat <path>",
        run: fs::cat,
    },
    Command {
        name: "touch",
        help: "touch <path>",
        run: fs::touch,
    },
    Command {
        name: "write",
        help: "write <path> <text>",
        run: fs::write,
    },
    Command {
        name: "rm",
        help: "rm <path>",
        run: fs::rm,
    },
    Command {
        name: "cp",
        help: "cp <src> <dst>",
        run: fs::cp,
    },
    Command {
        name: "mv",
        help: "mv <src> <dst>",
        run: fs::mv,
    },
    Command {
        name: "mount",
        help: "mount <source> [target]",
        run: fs::mount,
    },
    Command {
        name: "umount",
        help: "umount [target]",
        run: fs::umount,
    },
    Command {
        name: "mounts",
        help: "show mounted filesystems from /etc/mtab",
        run: fs::mounts,
    },
    Command {
        name: "sh",
        help: "start sh subshell (exit to return)",
        run: fs::sh,
    },
    Command {
        name: "ifconfig",
        help: "ifconfig [show|up|down|set <ip> <mask> <gw>]",
        run: net::ifconfig,
    },
    Command {
        name: "route",
        help: "route [show|set-gw <ip>]",
        run: net::route,
    },
    Command {
        name: "ping",
        help: "ping <host|ip> [count]",
        run: net::ping,
    },
    Command {
        name: "telnet",
        help: "telnet <host> [port] | telnet close <id>",
        run: net::telnet,
    },
    Command {
        name: "netstat",
        help: "show network stats and sessions",
        run: net::netstat,
    },
];

pub fn dispatch(ctx: &mut Context, line: &str) {
    if line.is_empty() {
        return;
    }

    let (name, args) = split_cmd(line);

    if let Some(cmd) = COMMANDS.iter().find(|c| c.name == name) {
        (cmd.run)(ctx, args);
    } else {
        crate::userland::shell::print("unknown command: ");
        crate::userland::shell::println(name);
    }
}

pub fn list_commands() -> &'static [Command] {
    COMMANDS
}

pub fn exists(name: &str) -> bool {
    COMMANDS.iter().any(|c| c.name == name)
}

fn split_cmd(line: &str) -> (&str, &str) {
    match line.find(' ') {
        Some(i) => (&line[..i], line[i + 1..].trim_start()),
        None => (line, ""),
    }
}
