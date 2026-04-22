use crate::drivers::{keyboard, serial, vga};
use core::fmt::{self, Write};

use super::{commands, Context};

const MAX_LINE: usize = 192;

pub fn run(ctx: &mut Context) -> ! {
    let mut line = [0u8; MAX_LINE];

    loop {
        prompt(ctx.hostname, ctx.cwd(), false);
        let len = read_line(&mut line);
        if len == 0 {
            continue;
        }

        let raw = &line[..len];
        let cmd = match core::str::from_utf8(raw) {
            Ok(s) => s.trim(),
            Err(_) => {
                println("invalid utf8");
                continue;
            }
        };

        commands::dispatch(ctx, cmd);
    }
}

pub fn run_sh(ctx: &mut Context) {
    let mut line = [0u8; MAX_LINE];
    loop {
        prompt(ctx.hostname, ctx.cwd(), true);
        let len = read_line(&mut line);
        if len == 0 {
            continue;
        }
        let raw = &line[..len];
        let cmd = match core::str::from_utf8(raw) {
            Ok(s) => s.trim(),
            Err(_) => {
                println("invalid utf8");
                continue;
            }
        };
        if cmd == "exit" {
            break;
        }
        commands::dispatch(ctx, cmd);
    }
}

fn prompt(hostname: &str, cwd: &str, sh_mode: bool) {
    print(hostname);
    if sh_mode {
        print(":sh");
    }
    print(":");
    print(cwd);
    print("$ ");
}

fn read_line(buf: &mut [u8]) -> usize {
    let mut idx = 0usize;

    loop {
        let key = keyboard::read_key_blocking();
        match key {
            b'\n' => {
                println("");
                return idx;
            }
            0x08 => {
                if idx > 0 {
                    idx -= 1;
                    print("\x08");
                }
            }
            b => {
                if idx < buf.len() - 1 {
                    buf[idx] = b;
                    idx += 1;
                    let one = [b];
                    if let Ok(s) = core::str::from_utf8(&one) {
                        print(s);
                    }
                }
            }
        }
    }
}

pub fn print(s: &str) {
    vga::print(s);
    serial::write_str(s);
}

pub fn println(s: &str) {
    vga::println(s);
    serial::write_str(s);
    serial::write_str("\n");
}

struct ScratchBuf {
    data: [u8; 256],
    len: usize,
}

impl ScratchBuf {
    fn new() -> Self {
        Self {
            data: [0; 256],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
}

impl Write for ScratchBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let space = self.data.len().saturating_sub(self.len);
        let n = core::cmp::min(space, bytes.len());
        self.data[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        Ok(())
    }
}

pub fn println_fmt(args: fmt::Arguments<'_>) {
    let mut buf = ScratchBuf::new();
    let _ = buf.write_fmt(args);
    println(buf.as_str());
}
