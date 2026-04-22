use super::commands;
use super::vfs::Vfs;
use super::Context;

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ENOEXEC: i32 = 8;
pub const EIO: i32 = 5;
pub const ENOMEM: i32 = 12;
pub const EINVAL: i32 = 22;
pub const ENOSYS: i32 = 38;

pub fn exec_from_path(ctx: &mut Context, path: &str, argv_tail: &str) -> Result<i32, i32> {
    let mut abs = [0u8; 64];
    let p = Vfs::resolve_path(ctx.cwd(), path, &mut abs).map_err(|_| EINVAL)?;
    let data = ctx.fs.read_file(p).ok_or(ENOENT)?;
    let mut file_buf = [0u8; 4096];
    if data.len() > file_buf.len() {
        return Err(ENOMEM);
    }
    file_buf[..data.len()].copy_from_slice(data);
    let data = &file_buf[..data.len()];

    if is_elf32(data) {
        return exec_elf32_from_path(ctx, data, argv_tail);
    }

    let text = core::str::from_utf8(data).map_err(|_| EIO)?;
    run_command_stream(ctx, text, argv_tail)
}

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const EI_VERSION: usize = 6;
const ELFCLASS32: u8 = 1;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_386: u16 = 3;
const PT_LOAD: u32 = 1;

const ELF32_EHDR_SIZE: usize = 52;
const ELF32_PHDR_SIZE: usize = 32;
const MAX_ELF_IMAGE: usize = 8192;

static mut ELF_IMAGE: [u8; MAX_ELF_IMAGE] = [0; MAX_ELF_IMAGE];

fn is_elf32(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == 0x7f && data[1] == b'E' && data[2] == b'L' && data[3] == b'F'
}

fn rd16(data: &[u8], off: usize) -> Option<u16> {
    let b = data.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn rd32(data: &[u8], off: usize) -> Option<u32> {
    let b = data.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn exec_elf32_from_path(ctx: &mut Context, data: &[u8], argv_tail: &str) -> Result<i32, i32> {
    if data.len() < ELF32_EHDR_SIZE {
        return Err(EINVAL);
    }
    if !is_elf32(data) {
        return Err(ENOEXEC);
    }
    if data[EI_CLASS] != ELFCLASS32 || data[EI_DATA] != ELFDATA2LSB || data[EI_VERSION] != EV_CURRENT {
        return Err(ENOEXEC);
    }

    let e_type = rd16(data, 16).ok_or(EINVAL)?;
    if e_type != ET_EXEC && e_type != ET_DYN {
        return Err(ENOEXEC);
    }
    let e_machine = rd16(data, 18).ok_or(EINVAL)?;
    if e_machine != EM_386 {
        return Err(ENOEXEC);
    }

    let e_entry = rd32(data, 24).ok_or(EINVAL)?;
    let e_phoff = rd32(data, 28).ok_or(EINVAL)? as usize;
    let e_ehsize = rd16(data, 40).ok_or(EINVAL)? as usize;
    let e_phentsize = rd16(data, 42).ok_or(EINVAL)? as usize;
    let e_phnum = rd16(data, 44).ok_or(EINVAL)? as usize;

    if e_ehsize < ELF32_EHDR_SIZE || e_phentsize != ELF32_PHDR_SIZE || e_phnum == 0 {
        return Err(EINVAL);
    }

    let ph_table_size = e_phentsize.checked_mul(e_phnum).ok_or(EINVAL)?;
    let _ = data.get(e_phoff..e_phoff + ph_table_size).ok_or(EINVAL)?;

    let mut load_count = 0usize;
    let mut min_vaddr = u32::MAX;
    let mut max_vaddr_end = 0u32;

    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        let p_type = rd32(data, ph).ok_or(EINVAL)?;
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = rd32(data, ph + 4).ok_or(EINVAL)? as usize;
        let p_vaddr = rd32(data, ph + 8).ok_or(EINVAL)?;
        let p_filesz = rd32(data, ph + 16).ok_or(EINVAL)? as usize;
        let p_memsz = rd32(data, ph + 20).ok_or(EINVAL)? as usize;

        if p_filesz > p_memsz {
            return Err(EINVAL);
        }
        let _ = data.get(p_offset..p_offset + p_filesz).ok_or(EINVAL)?;

        let end = p_vaddr.checked_add(p_memsz as u32).ok_or(EINVAL)?;
        if p_vaddr < min_vaddr {
            min_vaddr = p_vaddr;
        }
        if end > max_vaddr_end {
            max_vaddr_end = end;
        }
        load_count += 1;
    }

    if load_count == 0 || min_vaddr >= max_vaddr_end {
        return Err(ENOEXEC);
    }

    let image_len = (max_vaddr_end - min_vaddr) as usize;
    if image_len == 0 || image_len > MAX_ELF_IMAGE {
        return Err(ENOMEM);
    }

    // SAFETY: cooperative single-core userland path; buffer is used only during this call.
    let img = unsafe { &mut ELF_IMAGE[..image_len] };
    for b in img.iter_mut() {
        *b = 0;
    }

    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        let p_type = rd32(data, ph).ok_or(EINVAL)?;
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = rd32(data, ph + 4).ok_or(EINVAL)? as usize;
        let p_vaddr = rd32(data, ph + 8).ok_or(EINVAL)?;
        let p_filesz = rd32(data, ph + 16).ok_or(EINVAL)? as usize;

        if p_filesz == 0 {
            continue;
        }
        let dst_start = (p_vaddr - min_vaddr) as usize;
        let dst_end = dst_start.checked_add(p_filesz).ok_or(EINVAL)?;
        if dst_end > image_len {
            return Err(EINVAL);
        }
        let src = data.get(p_offset..p_offset + p_filesz).ok_or(EINVAL)?;
        img[dst_start..dst_end].copy_from_slice(src);
    }

    if e_entry < min_vaddr || e_entry >= max_vaddr_end {
        return Err(ENOEXEC);
    }
    let entry_off = (e_entry - min_vaddr) as usize;
    let entry = &img[entry_off..];
    let end = entry.iter().position(|&b| b == 0).unwrap_or(entry.len());
    if end == 0 {
        return Err(ENOEXEC);
    }
    let text = core::str::from_utf8(&entry[..end]).map_err(|_| ENOEXEC)?;
    run_command_stream(ctx, text, argv_tail)
}

fn run_command_stream(ctx: &mut Context, text: &str, argv_tail: &str) -> Result<i32, i32> {
    let mut ran = false;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }

        let base = if let Some(rest) = t.strip_prefix("builtin:") { rest.trim() } else { t };
        if base.is_empty() {
            continue;
        }

        if !ran && !argv_tail.is_empty() {
            let mut line_buf = [0u8; 192];
            if base.len() + 1 + argv_tail.len() >= line_buf.len() {
                return Err(ENOMEM);
            }
            let mut n = 0usize;
            line_buf[..base.len()].copy_from_slice(base.as_bytes());
            n += base.len();
            line_buf[n] = b' ';
            n += 1;
            line_buf[n..n + argv_tail.len()].copy_from_slice(argv_tail.as_bytes());
            n += argv_tail.len();
            let run = core::str::from_utf8(&line_buf[..n]).map_err(|_| EIO)?;
            let cmd = run.split_whitespace().next().unwrap_or("");
            if !commands::exists(cmd) {
                return Err(ENOSYS);
            }
            commands::dispatch(ctx, run);
            ran = true;
        } else {
            let cmd = base.split_whitespace().next().unwrap_or("");
            if !commands::exists(cmd) {
                return Err(ENOSYS);
            }
            commands::dispatch(ctx, base);
            ran = true;
        }
    }

    if ran {
        Ok(0)
    } else {
        Err(EINVAL)
    }
}
