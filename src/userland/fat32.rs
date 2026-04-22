use core::cmp::min;
use core::str;

use super::blockdev::{self, DeviceId, BLOCK_SIZE};

const FAT_EOC: u32 = 0x0FFF_FFF8;
const MAX_FILE_READ: usize = 4096;

static mut READ_BUF: [u8; MAX_FILE_READ] = [0; MAX_FILE_READ];

#[derive(Clone, Copy)]
pub struct Fat32 {
    pub dev: DeviceId,
    sectors_per_cluster: u8,
    fat_start_lba: u32,
    data_start_lba: u32,
    root_cluster: u32,
    cluster_count: u32,
}

#[derive(Clone, Copy)]
struct DirEnt {
    name: [u8; 11],
    attr: u8,
    first_cluster: u32,
    size: u32,
}

#[derive(Clone, Copy)]
struct DirLoc {
    cluster: u32,
    sector_off: u8,
    slot_off: u16,
}

pub fn probe(dev: DeviceId) -> Result<Fat32, &'static str> {
    let mut bs = [0u8; BLOCK_SIZE];
    blockdev::read_sector(dev, 0, &mut bs)?;
    if bs[510] != 0x55 || bs[511] != 0xAA {
        return Err("not fat boot sector");
    }

    let bps = le16(&bs, 11);
    if bps != 512 {
        return Err("unsupported sector size");
    }
    let spc = bs[13];
    if spc == 0 {
        return Err("invalid sectors/cluster");
    }
    let reserved = le16(&bs, 14);
    let fats = bs[16];
    let tot16 = le16(&bs, 19) as u32;
    let tot32 = le32(&bs, 32);
    let total_secs = if tot32 != 0 { tot32 } else { tot16 };
    let fat_sz = le32(&bs, 36);
    let root_cluster = le32(&bs, 44);
    if reserved == 0 || fats == 0 || fat_sz == 0 || root_cluster < 2 || total_secs == 0 {
        return Err("invalid fat32 geometry");
    }

    let fat_start = reserved as u32;
    let data_start = fat_start + (fats as u32) * fat_sz;
    if data_start >= total_secs {
        return Err("invalid fat32 layout");
    }
    let data_secs = total_secs - data_start;
    let clusters = data_secs / (spc as u32);

    Ok(Fat32 {
        dev,
        sectors_per_cluster: spc,
        fat_start_lba: fat_start,
        data_start_lba: data_start,
        root_cluster,
        cluster_count: clusters,
    })
}

pub fn is_dir(fs: Fat32, path: &str) -> bool {
    if path == "/" {
        return true;
    }
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    let parts = match parse_path(path, &mut seg1, &mut seg2) {
        Ok(v) => v,
        Err(_) => return false,
    };

    match parts {
        PathParts::One => match find_entry(fs, fs.root_cluster, &seg1) {
            Ok(Some((e, _))) => (e.attr & 0x10) != 0,
            _ => false,
        },
        PathParts::Two => {
            let Some((d, _)) = find_entry(fs, fs.root_cluster, &seg1).ok().flatten() else {
                return false;
            };
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return false;
            }
            match find_entry(fs, d.first_cluster, &seg2) {
                Ok(Some((e, _))) => (e.attr & 0x10) != 0,
                _ => false,
            }
        }
    }
}

pub fn list_dir<F: FnMut(&str)>(fs: Fat32, path: &str, mut cb: F) -> Result<(), &'static str> {
    if path == "/" {
        return list_dir_cluster(fs, fs.root_cluster, &mut cb);
    }
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    match parse_path(path, &mut seg1, &mut seg2)? {
        PathParts::One => {
            let Some((e, _)) = find_entry(fs, fs.root_cluster, &seg1)? else {
                return Err("no such file or directory");
            };
            if (e.attr & 0x10) == 0 {
                let name = short_name_to_utf8(&e.name);
                cb(name);
                return Ok(());
            }
            if e.first_cluster < 2 {
                return Ok(());
            }
            list_dir_cluster(fs, e.first_cluster, &mut cb)
        }
        PathParts::Two => {
            let Some((d, _)) = find_entry(fs, fs.root_cluster, &seg1)? else {
                return Err("no such file or directory");
            };
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return Err("no such file or directory");
            }
            let Some((e, _)) = find_entry(fs, d.first_cluster, &seg2)? else {
                return Err("no such file or directory");
            };
            if (e.attr & 0x10) == 0 {
                let name = short_name_to_utf8(&e.name);
                cb(name);
                return Ok(());
            }
            if e.first_cluster < 2 {
                return Ok(());
            }
            list_dir_cluster(fs, e.first_cluster, &mut cb)
        }
    }
}

pub fn read_file(fs: Fat32, path: &str) -> Option<&'static [u8]> {
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    let parts = parse_path(path, &mut seg1, &mut seg2).ok()?;
    let (e, _) = match parts {
        PathParts::One => find_entry(fs, fs.root_cluster, &seg1).ok().flatten()?,
        PathParts::Two => {
            let (d, _) = find_entry(fs, fs.root_cluster, &seg1).ok().flatten()?;
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return None;
            }
            find_entry(fs, d.first_cluster, &seg2).ok().flatten()?
        }
    };
    if (e.attr & 0x10) != 0 {
        return None;
    }
    let size = min(e.size as usize, MAX_FILE_READ);
    if size == 0 {
        return Some(&[]);
    }
    if e.first_cluster < 2 {
        return Some(&[]);
    }

    let mut left = size;
    let mut cur = e.first_cluster;
    let mut out_off = 0usize;
    let csz = cluster_size(fs);
    let mut cbuf = [0u8; 4096];
    if csz > cbuf.len() {
        return None;
    }
    loop {
        if cur < 2 {
            break;
        }
        if read_cluster(fs, cur, &mut cbuf[..csz]).is_err() {
            return None;
        }
        let n = min(left, csz);
        unsafe {
            READ_BUF[out_off..out_off + n].copy_from_slice(&cbuf[..n]);
        }
        out_off += n;
        left -= n;
        if left == 0 {
            break;
        }
        cur = fat_next(fs, cur).ok()?;
        if cur >= FAT_EOC {
            break;
        }
    }
    unsafe { Some(&READ_BUF[..out_off]) }
}

pub fn write_file(fs: Fat32, path: &str, data: &[u8]) -> Result<(), &'static str> {
    if data.len() > 4096 {
        return Err("content too large");
    }
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    let (parent_cluster, name) = match parse_path(path, &mut seg1, &mut seg2)? {
        PathParts::One => (fs.root_cluster, seg1),
        PathParts::Two => {
            let Some((d, _)) = find_entry(fs, fs.root_cluster, &seg1)? else {
                return Err("parent not found");
            };
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return Err("parent not directory");
            }
            (d.first_cluster, seg2)
        }
    };

    let found = find_entry(fs, parent_cluster, &name)?;
    let mut ent = DirEnt {
        name,
        attr: 0x20,
        first_cluster: 0,
        size: 0,
    };
    let mut loc = if let Some((e, l)) = found {
        if (e.attr & 0x10) != 0 {
            return Err("is a directory");
        }
        ent = e;
        l
    } else {
        find_free_slot(fs, parent_cluster)?.ok_or("directory full")?
    };

    let old_cluster = ent.first_cluster;
    if data.is_empty() {
        ent.first_cluster = 0;
        ent.size = 0;
        write_dirent(fs, loc, ent)?;
        if old_cluster >= 2 {
            free_chain(fs, old_cluster)?;
        }
        return Ok(());
    }

    let csz = cluster_size(fs);
    if data.len() > csz {
        return Err("content too large");
    }

    let new_cluster = if old_cluster >= 2 {
        old_cluster
    } else {
        alloc_cluster(fs)?
    };
    fat_set(fs, new_cluster, 0x0FFF_FFFF)?;

    let mut cbuf = [0u8; 4096];
    if csz > cbuf.len() {
        return Err("cluster too large");
    }
    cbuf[..csz].fill(0);
    cbuf[..data.len()].copy_from_slice(data);
    write_cluster(fs, new_cluster, &cbuf[..csz])?;

    ent.first_cluster = new_cluster;
    ent.size = data.len() as u32;
    write_dirent(fs, loc, ent)?;
    if old_cluster >= 2 && old_cluster != new_cluster {
        free_chain(fs, old_cluster)?;
    }
    Ok(())
}

pub fn create_file(fs: Fat32, path: &str, data: &[u8]) -> Result<(), &'static str> {
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    let (parent_cluster, name) = match parse_path(path, &mut seg1, &mut seg2)? {
        PathParts::One => (fs.root_cluster, seg1),
        PathParts::Two => {
            let Some((d, _)) = find_entry(fs, fs.root_cluster, &seg1)? else {
                return Err("parent not found");
            };
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return Err("parent not directory");
            }
            (d.first_cluster, seg2)
        }
    };
    if find_entry(fs, parent_cluster, &name)?.is_some() {
        return Err("already exists");
    }
    write_file(fs, path, data)
}

pub fn remove_file(fs: Fat32, path: &str) -> Result<(), &'static str> {
    let mut seg1 = [0u8; 11];
    let mut seg2 = [0u8; 11];
    let (parent_cluster, name) = match parse_path(path, &mut seg1, &mut seg2)? {
        PathParts::One => (fs.root_cluster, seg1),
        PathParts::Two => {
            let Some((d, _)) = find_entry(fs, fs.root_cluster, &seg1)? else {
                return Err("parent not found");
            };
            if (d.attr & 0x10) == 0 || d.first_cluster < 2 {
                return Err("parent not directory");
            }
            (d.first_cluster, seg2)
        }
    };

    let Some((e, loc)) = find_entry(fs, parent_cluster, &name)? else {
        return Err("file not found");
    };
    if (e.attr & 0x10) != 0 {
        return Err("is a directory");
    }
    delete_slot(fs, loc)?;
    if e.first_cluster >= 2 {
        free_chain(fs, e.first_cluster)?;
    }
    Ok(())
}

fn list_dir_cluster<F: FnMut(&str)>(fs: Fat32, start_cluster: u32, cb: &mut F) -> Result<(), &'static str> {
    let mut sec = [0u8; BLOCK_SIZE];
    let mut cur = start_cluster;
    let mut guard = 0u32;
    while cur >= 2 && cur < FAT_EOC && guard <= fs.cluster_count.saturating_add(2) {
        guard = guard.saturating_add(1);
        for s in 0..fs.sectors_per_cluster {
            let lba = cluster_to_lba(fs, cur) + s as u32;
            blockdev::read_sector(fs.dev, lba, &mut sec)?;
            let mut off = 0usize;
            while off + 32 <= BLOCK_SIZE {
                let b0 = sec[off];
                if b0 == 0x00 {
                    return Ok(());
                }
                if b0 == 0xE5 {
                    off += 32;
                    continue;
                }
                let attr = sec[off + 11];
                if attr == 0x0F {
                    off += 32;
                    continue;
                }
                let mut name = [0u8; 11];
                name.copy_from_slice(&sec[off..off + 11]);
                let sname = short_name_to_utf8(&name);
                if sname != "." && sname != ".." {
                    cb(sname);
                }
                off += 32;
            }
        }
        cur = fat_next(fs, cur)?;
        if cur >= FAT_EOC {
            break;
        }
    }
    Ok(())
}

fn find_entry(fs: Fat32, dir_cluster: u32, name: &[u8; 11]) -> Result<Option<(DirEnt, DirLoc)>, &'static str> {
    let mut sec = [0u8; BLOCK_SIZE];
    let mut cur = dir_cluster;
    let mut guard = 0u32;
    while cur >= 2 && cur < FAT_EOC && guard <= fs.cluster_count.saturating_add(2) {
        guard = guard.saturating_add(1);
        for s in 0..fs.sectors_per_cluster {
            let lba = cluster_to_lba(fs, cur) + s as u32;
            blockdev::read_sector(fs.dev, lba, &mut sec)?;
            let mut off = 0usize;
            while off + 32 <= BLOCK_SIZE {
                let b0 = sec[off];
                if b0 == 0x00 {
                    return Ok(None);
                }
                if b0 == 0xE5 || sec[off + 11] == 0x0F {
                    off += 32;
                    continue;
                }
                let mut n = [0u8; 11];
                n.copy_from_slice(&sec[off..off + 11]);
                if &n == name {
                    let hi = le16_slice(&sec[off + 20..off + 22]) as u32;
                    let lo = le16_slice(&sec[off + 26..off + 28]) as u32;
                    let ent = DirEnt {
                        name: n,
                        attr: sec[off + 11],
                        first_cluster: (hi << 16) | lo,
                        size: le32_slice(&sec[off + 28..off + 32]),
                    };
                    let loc = DirLoc {
                        cluster: cur,
                        sector_off: s,
                        slot_off: off as u16,
                    };
                    return Ok(Some((ent, loc)));
                }
                off += 32;
            }
        }
        cur = fat_next(fs, cur)?;
        if cur >= FAT_EOC {
            break;
        }
    }
    Ok(None)
}

fn find_free_slot(fs: Fat32, dir_cluster: u32) -> Result<Option<DirLoc>, &'static str> {
    let mut sec = [0u8; BLOCK_SIZE];
    let mut cur = dir_cluster;
    let mut guard = 0u32;
    while cur >= 2 && cur < FAT_EOC && guard <= fs.cluster_count.saturating_add(2) {
        guard = guard.saturating_add(1);
        for s in 0..fs.sectors_per_cluster {
            let lba = cluster_to_lba(fs, cur) + s as u32;
            blockdev::read_sector(fs.dev, lba, &mut sec)?;
            let mut off = 0usize;
            while off + 32 <= BLOCK_SIZE {
                let b0 = sec[off];
                if b0 == 0x00 || b0 == 0xE5 {
                    return Ok(Some(DirLoc {
                        cluster: cur,
                        sector_off: s,
                        slot_off: off as u16,
                    }));
                }
                off += 32;
            }
        }
        cur = fat_next(fs, cur)?;
        if cur >= FAT_EOC {
            break;
        }
    }
    Ok(None)
}

fn write_dirent(fs: Fat32, loc: DirLoc, ent: DirEnt) -> Result<(), &'static str> {
    let lba = cluster_to_lba(fs, loc.cluster) + loc.sector_off as u32;
    let mut sec = [0u8; BLOCK_SIZE];
    blockdev::read_sector(fs.dev, lba, &mut sec)?;
    let off = loc.slot_off as usize;
    sec[off..off + 11].copy_from_slice(&ent.name);
    sec[off + 11] = ent.attr;
    sec[off + 12] = 0;
    sec[off + 13] = 0;
    sec[off + 14] = 0;
    sec[off + 15] = 0;
    sec[off + 16] = 0;
    sec[off + 17] = 0;
    sec[off + 18] = 0;
    sec[off + 19] = 0;
    let hi = ((ent.first_cluster >> 16) as u16).to_le_bytes();
    sec[off + 20] = hi[0];
    sec[off + 21] = hi[1];
    sec[off + 22] = 0;
    sec[off + 23] = 0;
    let lo = (ent.first_cluster as u16).to_le_bytes();
    sec[off + 26] = lo[0];
    sec[off + 27] = lo[1];
    let sz = ent.size.to_le_bytes();
    sec[off + 28..off + 32].copy_from_slice(&sz);
    blockdev::write_sector(fs.dev, lba, &sec)
}

fn delete_slot(fs: Fat32, loc: DirLoc) -> Result<(), &'static str> {
    let lba = cluster_to_lba(fs, loc.cluster) + loc.sector_off as u32;
    let mut sec = [0u8; BLOCK_SIZE];
    blockdev::read_sector(fs.dev, lba, &mut sec)?;
    sec[loc.slot_off as usize] = 0xE5;
    blockdev::write_sector(fs.dev, lba, &sec)
}

fn alloc_cluster(fs: Fat32) -> Result<u32, &'static str> {
    let max = fs.cluster_count.saturating_add(2);
    let mut c = 2u32;
    while c < max {
        if fat_next(fs, c)? == 0 {
            fat_set(fs, c, 0x0FFF_FFFF)?;
            clear_cluster(fs, c)?;
            return Ok(c);
        }
        c += 1;
    }
    Err("no space left")
}

fn free_chain(fs: Fat32, start: u32) -> Result<(), &'static str> {
    let mut cur = start;
    let mut guard = 0u32;
    while cur >= 2 && cur < FAT_EOC && guard <= fs.cluster_count.saturating_add(2) {
        guard += 1;
        let next = fat_next(fs, cur)?;
        fat_set(fs, cur, 0)?;
        if next >= FAT_EOC || next == 0 {
            break;
        }
        cur = next;
    }
    Ok(())
}

fn clear_cluster(fs: Fat32, cluster: u32) -> Result<(), &'static str> {
    let sec = [0u8; BLOCK_SIZE];
    for s in 0..fs.sectors_per_cluster {
        blockdev::write_sector(fs.dev, cluster_to_lba(fs, cluster) + s as u32, &sec)?;
    }
    Ok(())
}

fn read_cluster(fs: Fat32, cluster: u32, out: &mut [u8]) -> Result<(), &'static str> {
    let csz = cluster_size(fs);
    if out.len() < csz {
        return Err("buffer too small");
    }
    let mut sec = [0u8; BLOCK_SIZE];
    for s in 0..fs.sectors_per_cluster {
        blockdev::read_sector(fs.dev, cluster_to_lba(fs, cluster) + s as u32, &mut sec)?;
        let off = (s as usize) * BLOCK_SIZE;
        out[off..off + BLOCK_SIZE].copy_from_slice(&sec);
    }
    Ok(())
}

fn write_cluster(fs: Fat32, cluster: u32, data: &[u8]) -> Result<(), &'static str> {
    let csz = cluster_size(fs);
    if data.len() < csz {
        return Err("buffer too small");
    }
    for s in 0..fs.sectors_per_cluster {
        let off = (s as usize) * BLOCK_SIZE;
        let mut sec = [0u8; BLOCK_SIZE];
        sec.copy_from_slice(&data[off..off + BLOCK_SIZE]);
        blockdev::write_sector(fs.dev, cluster_to_lba(fs, cluster) + s as u32, &sec)?;
    }
    Ok(())
}

fn fat_next(fs: Fat32, cluster: u32) -> Result<u32, &'static str> {
    let fat_off = cluster.saturating_mul(4);
    let sec_idx = fat_off / (BLOCK_SIZE as u32);
    let off = (fat_off % (BLOCK_SIZE as u32)) as usize;
    let mut sec = [0u8; BLOCK_SIZE];
    blockdev::read_sector(fs.dev, fs.fat_start_lba + sec_idx, &mut sec)?;
    let v = le32_slice(&sec[off..off + 4]) & 0x0FFF_FFFF;
    Ok(v)
}

fn fat_set(fs: Fat32, cluster: u32, value: u32) -> Result<(), &'static str> {
    let fat_off = cluster.saturating_mul(4);
    let sec_idx = fat_off / (BLOCK_SIZE as u32);
    let off = (fat_off % (BLOCK_SIZE as u32)) as usize;
    let lba = fs.fat_start_lba + sec_idx;
    let mut sec = [0u8; BLOCK_SIZE];
    blockdev::read_sector(fs.dev, lba, &mut sec)?;
    let cur = le32_slice(&sec[off..off + 4]);
    let nv = (cur & 0xF000_0000) | (value & 0x0FFF_FFFF);
    sec[off..off + 4].copy_from_slice(&nv.to_le_bytes());
    blockdev::write_sector(fs.dev, lba, &sec)
}

fn cluster_size(fs: Fat32) -> usize {
    (fs.sectors_per_cluster as usize) * BLOCK_SIZE
}

fn cluster_to_lba(fs: Fat32, cluster: u32) -> u32 {
    fs.data_start_lba + (cluster - 2) * (fs.sectors_per_cluster as u32)
}

fn short_name_to_utf8(name: &[u8; 11]) -> &str {
    static mut NAME_BUF: [u8; 13] = [0; 13];
    let mut len = 0usize;
    let mut i = 0usize;
    while i < 8 && name[i] != b' ' {
        unsafe {
            NAME_BUF[len] = name[i];
        }
        len += 1;
        i += 1;
    }
    let has_ext = name[8] != b' ' || name[9] != b' ' || name[10] != b' ';
    if has_ext {
        unsafe { NAME_BUF[len] = b'.' };
        len += 1;
        let mut j = 8usize;
        while j < 11 && name[j] != b' ' {
            unsafe {
                NAME_BUF[len] = name[j];
            }
            len += 1;
            j += 1;
        }
    }
    unsafe { str::from_utf8(&NAME_BUF[..len]).unwrap_or("?") }
}

enum PathParts {
    One,
    Two,
}

fn parse_path(path: &str, out1: &mut [u8; 11], out2: &mut [u8; 11]) -> Result<PathParts, &'static str> {
    if !path.starts_with('/') || path == "/" {
        return Err("invalid fat32 path");
    }
    let rest = &path[1..];
    let mut it = rest.split('/');
    let p1 = it.next().unwrap_or("");
    if p1.is_empty() {
        return Err("invalid fat32 path");
    }
    name_to_short(p1, out1)?;
    if let Some(p2) = it.next() {
        if p2.is_empty() || it.next().is_some() {
            return Err("fat32 path depth unsupported");
        }
        name_to_short(p2, out2)?;
        Ok(PathParts::Two)
    } else {
        Ok(PathParts::One)
    }
}

fn name_to_short(name: &str, out: &mut [u8; 11]) -> Result<(), &'static str> {
    out.fill(b' ');
    if name == "." || name == ".." {
        return Err("invalid name");
    }
    let (base, ext) = match name.split_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return Err("unsupported 8.3 name");
    }
    for (i, b) in base.bytes().enumerate() {
        out[i] = upcase_83(b).ok_or("unsupported name char")?;
    }
    for (i, b) in ext.bytes().enumerate() {
        out[8 + i] = upcase_83(b).ok_or("unsupported name char")?;
    }
    Ok(())
}

fn upcase_83(b: u8) -> Option<u8> {
    match b {
        b'a'..=b'z' => Some(b - 32),
        b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' => Some(b),
        _ => None,
    }
}

fn le16(buf: &[u8; BLOCK_SIZE], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn le32(buf: &[u8; BLOCK_SIZE], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn le16_slice(buf: &[u8]) -> u16 {
    u16::from_le_bytes([buf[0], buf[1]])
}

fn le32_slice(buf: &[u8]) -> u32 {
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}
