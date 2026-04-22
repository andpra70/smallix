use crate::userland::{shell, Context};
use crate::userland::procfs;
use crate::userland::vfs::Vfs;

pub fn ls(ctx: &mut Context, args: &str) {
    let target = if args.trim().is_empty() { ctx.cwd() } else { args.trim() };
    let mut abs = [0u8; 64];
    let path = match Vfs::resolve_path(ctx.cwd(), target, &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };

    if procfs::is_proc_path(path) {
        match procfs::list_dir(ctx, path, |name| shell::println(name)) {
            Ok(()) => {}
            Err(e) => shell::println(e),
        }
        return;
    }

    match ctx.fs.list_dir(ctx.cwd(), args.trim(), |name| shell::println(name)) {
        Ok(()) => {
            if path == "/" {
                shell::println("proc");
            }
        }
        Err(e) => shell::println(e),
    }
}

pub fn cat(ctx: &mut Context, args: &str) {
    if args.is_empty() {
        shell::println("usage: cat <path>");
        return;
    }

    let p = args.trim();
    if p.starts_with("/dev/") {
        match ctx.dev.read_text(p) {
            Ok(s) => shell::println(s),
            Err(e) => shell::println(e),
        }
        return;
    }

    let mut abs = [0u8; 64];
    let path = match Vfs::resolve_path(ctx.cwd(), p, &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };

    if procfs::is_proc_path(path) {
        if let Err(e) = procfs::cat(ctx, path) {
            shell::println(e);
        }
        return;
    }

    match ctx.fs.read_file(path) {
        Some(data) => match core::str::from_utf8(data) {
            Ok(s) => shell::println(s),
            Err(_) => shell::println("binary content"),
        },
        None => shell::println("file not found"),
    }
}

pub fn touch(ctx: &mut Context, args: &str) {
    if args.is_empty() {
        shell::println("usage: touch <path>");
        return;
    }

    let mut abs = [0u8; 64];
    let path = match Vfs::resolve_path(ctx.cwd(), args.trim(), &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(path) {
        shell::println("read-only fs");
        return;
    }

    match ctx.fs.create_file(path, b"") {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

pub fn write(ctx: &mut Context, args: &str) {
    let Some((path, text)) = args.split_once(' ') else {
        shell::println("usage: write <path> <text>");
        return;
    };

    if path.is_empty() {
        shell::println("usage: write <path> <text>");
        return;
    }

    if path.starts_with("/dev/") {
        match ctx.dev.write(path, text) {
            Ok(_) => shell::println("ok"),
            Err(e) => shell::println(e),
        }
        return;
    }

    let mut abs = [0u8; 64];
    let ap = match Vfs::resolve_path(ctx.cwd(), path, &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(ap) {
        shell::println("read-only fs");
        return;
    }

    match ctx.fs.write_file(ap, text.as_bytes()) {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

pub fn rm(ctx: &mut Context, args: &str) {
    if args.is_empty() {
        shell::println("usage: rm <path>");
        return;
    }
    let mut abs = [0u8; 64];
    let p = match Vfs::resolve_path(ctx.cwd(), args.trim(), &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(p) {
        shell::println("read-only fs");
        return;
    }
    match ctx.fs.remove_file(p) {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

fn parse_two_paths<'a>(args: &'a str, usage: &'static str) -> Option<(&'a str, &'a str)> {
    let Some((a, b)) = args.trim().split_once(' ') else {
        shell::println(usage);
        return None;
    };
    let src = a.trim();
    let dst = b.trim();
    if src.is_empty() || dst.is_empty() {
        shell::println(usage);
        return None;
    }
    Some((src, dst))
}

fn resolve_abs<'a>(ctx: &Context, input: &str, out: &'a mut [u8; 64]) -> Result<&'a str, &'static str> {
    Vfs::resolve_path(ctx.cwd(), input, out)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn resolve_target<'a>(ctx: &Context, src_abs: &str, dst_in: &str, out: &'a mut [u8; 64]) -> Result<&'a str, &'static str> {
    let mut dst_abs = [0u8; 64];
    let resolved = resolve_abs(ctx, dst_in, &mut dst_abs)?;
    if !ctx.fs.is_dir(resolved) {
        out[..resolved.len()].copy_from_slice(resolved.as_bytes());
        return core::str::from_utf8(&out[..resolved.len()]).map_err(|_| "invalid path");
    }

    let name = basename(src_abs);
    if name.is_empty() {
        return Err("invalid source");
    }
    let join_len = if resolved == "/" {
        1 + name.len()
    } else {
        resolved.len() + 1 + name.len()
    };
    if join_len >= out.len() {
        return Err("path too long");
    }

    let mut pos = 0usize;
    if resolved == "/" {
        out[pos] = b'/';
        pos += 1;
    } else {
        out[..resolved.len()].copy_from_slice(resolved.as_bytes());
        pos += resolved.len();
        out[pos] = b'/';
        pos += 1;
    }
    out[pos..pos + name.len()].copy_from_slice(name.as_bytes());
    pos += name.len();
    core::str::from_utf8(&out[..pos]).map_err(|_| "invalid path")
}

pub fn cp(ctx: &mut Context, args: &str) {
    let Some((src_in, dst_in)) = parse_two_paths(args, "usage: cp <src> <dst>") else {
        return;
    };
    if src_in.starts_with("/dev/") || dst_in.starts_with("/dev/") {
        shell::println("cp on /dev/* not supported");
        return;
    }

    let mut src_buf = [0u8; 64];
    let src = match resolve_abs(ctx, src_in, &mut src_buf) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(src) {
        shell::println("cp from /proc not supported");
        return;
    }

    if ctx.fs.is_dir(src) {
        shell::println("source is a directory");
        return;
    }
    let Some(data) = ctx.fs.read_file(src) else {
        shell::println("file not found");
        return;
    };

    let mut copy = [0u8; 512];
    if data.len() > copy.len() {
        shell::println("content too large");
        return;
    }
    copy[..data.len()].copy_from_slice(data);

    let mut dst_buf = [0u8; 64];
    let dst = match resolve_target(ctx, src, dst_in, &mut dst_buf) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(dst) {
        shell::println("read-only fs");
        return;
    }

    match ctx.fs.write_file(dst, &copy[..data.len()]) {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

pub fn mv(ctx: &mut Context, args: &str) {
    let Some((src_in, dst_in)) = parse_two_paths(args, "usage: mv <src> <dst>") else {
        return;
    };
    if src_in.starts_with("/dev/") || dst_in.starts_with("/dev/") {
        shell::println("mv on /dev/* not supported");
        return;
    }

    let mut src_buf = [0u8; 64];
    let src = match resolve_abs(ctx, src_in, &mut src_buf) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(src) {
        shell::println("mv from /proc not supported");
        return;
    }

    if ctx.fs.is_dir(src) {
        shell::println("source is a directory");
        return;
    }
    let Some(data) = ctx.fs.read_file(src) else {
        shell::println("file not found");
        return;
    };

    let mut copy = [0u8; 512];
    if data.len() > copy.len() {
        shell::println("content too large");
        return;
    }
    copy[..data.len()].copy_from_slice(data);

    let mut dst_buf = [0u8; 64];
    let dst = match resolve_target(ctx, src, dst_in, &mut dst_buf) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if procfs::is_proc_path(dst) {
        shell::println("read-only fs");
        return;
    }

    if src == dst {
        shell::println("ok");
        return;
    }

    match ctx.fs.write_file(dst, &copy[..data.len()]) {
        Ok(()) => {}
        Err(e) => {
            shell::println(e);
            return;
        }
    }
    match ctx.fs.remove_file(src) {
        Ok(()) => shell::println("ok"),
        Err(e) => shell::println(e),
    }
}

pub fn cd(ctx: &mut Context, args: &str) {
    let target = if args.trim().is_empty() { "/" } else { args.trim() };
    let mut abs = [0u8; 64];
    let p = match Vfs::resolve_path(ctx.cwd(), target, &mut abs) {
        Ok(v) => v,
        Err(e) => {
            shell::println(e);
            return;
        }
    };
    if !ctx.fs.is_dir(p) && !procfs::is_proc_dir(p) {
        shell::println("not a directory");
        return;
    }
    let _ = ctx.set_cwd(p);
}

pub fn pwd(ctx: &mut Context, _args: &str) {
    shell::println(ctx.cwd());
}

pub fn mount(ctx: &mut Context, args: &str) {
    let raw = args.trim();
    if raw.is_empty() {
        shell::println_fmt(format_args!("mounted fs: {}", ctx.fs.active_name()));
        shell::println("usage: mount <source> [target]");
        shell::println("source: /dev/ramfs|/dev/hda|/dev/loop0|/dev/usb0|<img_path>");
        shell::println("target: / or /mnt/<name> (default /)");
        return;
    }
    let mut it = raw.split_whitespace();
    let source = it.next().unwrap_or("");
    let target = it.next().unwrap_or("/");
    if it.next().is_some() {
        shell::println("usage: mount <source> [target]");
        return;
    }

    let mut cwd_buf = [0u8; 64];
    let cwd = ctx.cwd();
    let cwd_len = cwd.len();
    cwd_buf[..cwd_len].copy_from_slice(cwd.as_bytes());
    let cwd_copy = core::str::from_utf8(&cwd_buf[..cwd_len]).unwrap_or("/");
    match ctx.fs.mount_at(source, target, cwd_copy) {
        Ok(msg) => shell::println(msg),
        Err(e) => shell::println(e),
    }
}

pub fn umount(ctx: &mut Context, args: &str) {
    let mut target_buf = [0u8; 64];
    let target = if args.trim().is_empty() {
        let mp = ctx.fs.mount_point();
        let n = mp.len().min(target_buf.len());
        target_buf[..n].copy_from_slice(&mp.as_bytes()[..n]);
        core::str::from_utf8(&target_buf[..n]).unwrap_or("/")
    } else {
        args.trim()
    };
    let mut cwd_buf = [0u8; 64];
    let cwd = ctx.cwd();
    let cwd_len = cwd.len();
    cwd_buf[..cwd_len].copy_from_slice(cwd.as_bytes());
    let cwd_copy = core::str::from_utf8(&cwd_buf[..cwd_len]).unwrap_or("/");

    match ctx.fs.umount_at(target, cwd_copy) {
        Ok(msg) => {
            if ctx.cwd().starts_with("/mnt/") {
                let _ = ctx.set_cwd("/");
            }
            shell::println(msg);
        }
        Err(e) => shell::println(e),
    }
}

pub fn sh(ctx: &mut Context, _args: &str) {
    crate::userland::shell::run_sh(ctx);
}

pub fn mounts(ctx: &mut Context, _args: &str) {
    let text = ctx.fs.mtab_line();
    shell::println("SOURCE       TARGET  FSTYPE");
    let mut any = false;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let src = it.next().unwrap_or("");
        let tgt = it.next().unwrap_or("");
        let fst = it.next().unwrap_or("");
        if src.is_empty() || tgt.is_empty() || fst.is_empty() {
            continue;
        }
        any = true;
        shell::println_fmt(format_args!("{:<12} {:<7} {}", src, tgt, fst));
    }
    if !any {
        shell::println("(empty)");
    }
}
