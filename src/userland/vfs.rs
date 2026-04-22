use core::cmp::min;
use core::str;

use super::blockdev::{self, DeviceId, BLOCK_SIZE};
use super::fat32;

const MAX_FILES: usize = 32;
const PATH_CAP: usize = 64;
const DATA_CAP: usize = 512;
const FS_SECTORS: u32 = 64;
const MAGIC: &[u8; 8] = b"SVFSBLK1";
const LEGACY_IMG_MAGIC: &[u8] = b"SMALLIXIMG1\n";

#[derive(Clone, Copy)]
struct FileEntry {
    used: bool,
    path_len: u8,
    data_len: u32,
    path: [u8; PATH_CAP],
    data: [u8; DATA_CAP],
}

impl FileEntry {
    const fn empty() -> Self {
        Self {
            used: false,
            path_len: 0,
            data_len: 0,
            path: [0; PATH_CAP],
            data: [0; DATA_CAP],
        }
    }
}

#[derive(Clone, Copy)]
enum ActiveFs {
    Ram,
    Hda,
    Loop0,
    Usb0,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FsKind {
    Smallix,
    Fat32,
}

pub struct Vfs {
    files: [FileEntry; MAX_FILES],
    active: ActiveFs,
    kind: FsKind,
    fat: Option<fat32::Fat32>,
    mount_point: [u8; PATH_CAP],
    mount_point_len: usize,
    mtab_line: [u8; 96],
    mtab_len: usize,
}

impl Vfs {
    pub fn new() -> Self {
        let mut mtab = [0u8; 96];
        let line = b"/dev/ramfs / ramfs rw 0 0\n";
        mtab[..line.len()].copy_from_slice(line);
        let mut mount_point = [0u8; PATH_CAP];
        mount_point[0] = b'/';
        Self {
            files: [FileEntry::empty(); MAX_FILES],
            active: ActiveFs::Ram,
            kind: FsKind::Smallix,
            fat: None,
            mount_point,
            mount_point_len: 1,
            mtab_line: mtab,
            mtab_len: line.len(),
        }
    }

    pub fn mount_point(&self) -> &str {
        str::from_utf8(&self.mount_point[..self.mount_point_len]).unwrap_or("/")
    }

    pub fn active_name(&self) -> &'static str {
        if self.kind == FsKind::Fat32 {
            return "fat32";
        }
        match self.active {
            ActiveFs::Ram => "ramfs",
            ActiveFs::Hda => "hdafs",
            ActiveFs::Loop0 => "loopfs",
            ActiveFs::Usb0 => "usbfs",
        }
    }

    pub fn mtab_line(&self) -> &str {
        str::from_utf8(&self.mtab_line[..self.mtab_len]).unwrap_or("")
    }

    pub fn create_file(&mut self, path: &str, content: &[u8]) -> Result<(), &'static str> {
        if self.kind == FsKind::Fat32 {
            let Some(fs) = self.fat else {
                return Err("fat32 not mounted");
            };
            let mut mapped = [0u8; PATH_CAP];
            let inner = self.map_path_to_mounted(path, &mut mapped).ok_or("outside mountpoint")?;
            return fat32::create_file(fs, inner, content);
        }
        if path.is_empty() || path.len() >= PATH_CAP {
            return Err("invalid path");
        }
        if content.len() > DATA_CAP {
            return Err("content too large");
        }
        if self.find_index(path).is_some() {
            return Err("already exists");
        }
        let Some(slot) = self.files.iter_mut().find(|f| !f.used) else {
            return Err("fs full");
        };
        slot.used = true;
        slot.path_len = path.len() as u8;
        slot.data_len = content.len() as u32;
        slot.path[..path.len()].copy_from_slice(path.as_bytes());
        slot.data[..content.len()].copy_from_slice(content);
        self.sync_to_device()
    }

    pub fn write_file(&mut self, path: &str, content: &[u8]) -> Result<(), &'static str> {
        if self.kind == FsKind::Fat32 {
            let Some(fs) = self.fat else {
                return Err("fat32 not mounted");
            };
            let mut mapped = [0u8; PATH_CAP];
            let inner = self.map_path_to_mounted(path, &mut mapped).ok_or("outside mountpoint")?;
            return fat32::write_file(fs, inner, content);
        }
        if content.len() > DATA_CAP {
            return Err("content too large");
        }

        if let Some(i) = self.find_index(path) {
            self.files[i].data_len = content.len() as u32;
            self.files[i].data[..content.len()].copy_from_slice(content);
            return self.sync_to_device();
        }

        self.create_file(path, content)
    }

    pub fn read_file<'a>(&'a self, path: &str) -> Option<&'a [u8]> {
        if self.kind == FsKind::Fat32 {
            let fs = self.fat?;
            let mut mapped = [0u8; PATH_CAP];
            let inner = self.map_path_to_mounted(path, &mut mapped)?;
            return fat32::read_file(fs, inner);
        }
        let i = self.find_index(path)?;
        let f = &self.files[i];
        Some(&f.data[..f.data_len as usize])
    }

    pub fn remove_file(&mut self, path: &str) -> Result<(), &'static str> {
        if self.kind == FsKind::Fat32 {
            let Some(fs) = self.fat else {
                return Err("fat32 not mounted");
            };
            let mut mapped = [0u8; PATH_CAP];
            let inner = self.map_path_to_mounted(path, &mut mapped).ok_or("outside mountpoint")?;
            return fat32::remove_file(fs, inner);
        }
        let Some(i) = self.find_index(path) else {
            return Err("file not found");
        };
        self.files[i] = FileEntry::empty();
        self.sync_to_device()
    }

    pub fn list_paths<F: FnMut(&str)>(&self, mut cb: F) {
        if self.kind == FsKind::Fat32 {
            if let Some(fs) = self.fat {
                let _ = fat32::list_dir(fs, "/", |n| {
                    let mut full = [0u8; PATH_CAP];
                    full[0] = b'/';
                    let nl = n.len();
                    if nl + 1 >= PATH_CAP {
                        return;
                    }
                    full[1..1 + nl].copy_from_slice(n.as_bytes());
                    if let Ok(s) = str::from_utf8(&full[..1 + nl]) {
                        cb(s);
                    }
                });
            }
            return;
        }
        for f in &self.files {
            if !f.used {
                continue;
            }
            if let Ok(p) = str::from_utf8(&f.path[..f.path_len as usize]) {
                cb(p);
            }
        }
    }

    pub fn mount(&mut self, source: &str, cwd: &str) -> Result<&'static str, &'static str> {
        self.mount_at(source, "/", cwd)
    }

    pub fn mount_at(&mut self, source: &str, target: &str, cwd: &str) -> Result<&'static str, &'static str> {
        self.set_mount_point(target, cwd)?;

        if source == "/dev/ramfs" {
            self.active = ActiveFs::Ram;
            let meta = fat32::probe(DeviceId::Ram).map_err(|_| "ramfs is not fat32")?;
            self.kind = FsKind::Fat32;
            self.fat = Some(meta);
            self.refresh_mtab();
            return Ok("mounted /dev/ramfs");
        }

        if source == "/dev/hda" {
            self.active = ActiveFs::Hda;
            let meta = fat32::probe(DeviceId::HdaRaw).map_err(|_| "/dev/hda is not fat32")?;
            self.kind = FsKind::Fat32;
            self.fat = Some(meta);
            self.refresh_mtab();
            return Ok("mounted /dev/hda");
        }

        if source == "/dev/loop0" {
            self.active = ActiveFs::Loop0;
            let meta = fat32::probe(DeviceId::Loop0).map_err(|_| "/dev/loop0 is not fat32")?;
            self.kind = FsKind::Fat32;
            self.fat = Some(meta);
            self.refresh_mtab();
            return Ok("mounted /dev/loop0");
        }

        if source == "/dev/usb0" {
            self.active = ActiveFs::Usb0;
            let meta = fat32::probe(DeviceId::Usb0).map_err(|_| "/dev/usb0 is not fat32")?;
            self.kind = FsKind::Fat32;
            self.fat = Some(meta);
            self.refresh_mtab();
            return Ok("mounted /dev/usb0");
        }

        let mut abs = [0u8; PATH_CAP];
        let p = Self::resolve_path(cwd, source, &mut abs)?;
        let img = self.read_file(p).ok_or("mount source not found")?;
        let mut tmp = [0u8; DATA_CAP];
        let n = min(img.len(), tmp.len());
        tmp[..n].copy_from_slice(&img[..n]);
        self.active = ActiveFs::Loop0;
        blockdev::load_loop_image(&tmp[..n])?;
        let meta = fat32::probe(DeviceId::Loop0).map_err(|_| "image is not fat32")?;
        self.kind = FsKind::Fat32;
        self.fat = Some(meta);
        self.refresh_mtab();
        Ok("mounted image file on /dev/loop0")
    }

    pub fn umount_at(&mut self, target: &str, cwd: &str) -> Result<&'static str, &'static str> {
        let mut abs = [0u8; PATH_CAP];
        let p = Self::resolve_path(cwd, target, &mut abs)?;
        let cur = self.mount_point();
        if cur == "/" {
            return Err("nothing mounted on /mnt");
        }
        if p != cur {
            return Err("not mounted");
        }
        self.mount_point[0] = b'/';
        self.mount_point_len = 1;
        self.refresh_mtab();
        Ok("unmounted")
    }

    pub fn refresh_mtab(&mut self) {
        let target = self.mount_point();
        let line = match (self.active, self.kind) {
            (ActiveFs::Ram, _) => {
                if target == "/" { "/dev/ramfs / ramfs rw 0 0\n" } else { "/dev/ramfs <mnt> ramfs rw 0 0\n" }
            }
            (ActiveFs::Hda, FsKind::Fat32) => {
                if target == "/" { "/dev/hda / fat32 rw 0 0\n" } else { "/dev/hda <mnt> fat32 rw 0 0\n" }
            }
            (ActiveFs::Hda, FsKind::Smallix) => {
                if target == "/" { "/dev/hda / hdafs rw 0 0\n" } else { "/dev/hda <mnt> hdafs rw 0 0\n" }
            }
            (ActiveFs::Loop0, FsKind::Fat32) => {
                if target == "/" { "/dev/loop0 / fat32 rw 0 0\n" } else { "/dev/loop0 <mnt> fat32 rw 0 0\n" }
            }
            (ActiveFs::Loop0, FsKind::Smallix) => {
                if target == "/" { "/dev/loop0 / loopfs rw 0 0\n" } else { "/dev/loop0 <mnt> loopfs rw 0 0\n" }
            }
            (ActiveFs::Usb0, FsKind::Fat32) => {
                if target == "/" { "/dev/usb0 / fat32 rw 0 0\n" } else { "/dev/usb0 <mnt> fat32 rw 0 0\n" }
            }
            (ActiveFs::Usb0, FsKind::Smallix) => {
                if target == "/" { "/dev/usb0 / usbfs rw 0 0\n" } else { "/dev/usb0 <mnt> usbfs rw 0 0\n" }
            }
        };
        if target == "/" {
            self.mtab_len = line.len();
            self.mtab_line[..self.mtab_len].copy_from_slice(line.as_bytes());
            let _ = self.write_file("/etc/mtab", line.as_bytes());
        } else {
            let src = match self.active {
                ActiveFs::Ram => "/dev/ramfs",
                ActiveFs::Hda => "/dev/hda",
                ActiveFs::Loop0 => "/dev/loop0",
                ActiveFs::Usb0 => "/dev/usb0",
            };
            let fstype = match (self.active, self.kind) {
                (_, FsKind::Fat32) => "fat32",
                (ActiveFs::Ram, FsKind::Smallix) => "ramfs",
                (ActiveFs::Hda, FsKind::Smallix) => "hdafs",
                (ActiveFs::Loop0, FsKind::Smallix) => "loopfs",
                (ActiveFs::Usb0, FsKind::Smallix) => "usbfs",
            };
            let mut tmp = [0u8; 96];
            let mut n = 0usize;
            for &b in src.as_bytes() {
                tmp[n] = b;
                n += 1;
            }
            tmp[n] = b' ';
            n += 1;
            for &b in target.as_bytes() {
                tmp[n] = b;
                n += 1;
            }
            tmp[n] = b' ';
            n += 1;
            for &b in fstype.as_bytes() {
                tmp[n] = b;
                n += 1;
            }
            for &b in b" rw 0 0\n" {
                tmp[n] = b;
                n += 1;
            }
            self.mtab_len = n;
            self.mtab_line[..n].copy_from_slice(&tmp[..n]);

            let mut mtab_path = [0u8; PATH_CAP];
            let mtab_vis = self.join_visible_mount_path("/etc/mtab", &mut mtab_path).unwrap_or("/etc/mtab");
            let _ = self.write_file(mtab_vis, &tmp[..n]);
        }
    }

    fn active_device(&self) -> DeviceId {
        match self.active {
            ActiveFs::Ram => DeviceId::Ram,
            ActiveFs::Hda => DeviceId::Hda,
            ActiveFs::Loop0 => DeviceId::Loop0,
            ActiveFs::Usb0 => DeviceId::Usb0,
        }
    }

    fn sync_to_device(&self) -> Result<(), &'static str> {
        let dev = self.active_device();
        let mut sec = [0u8; BLOCK_SIZE];
        let mut pos = 0usize;

        let mut put = |b: u8, sec: &mut [u8; BLOCK_SIZE], dev: DeviceId, pos: &mut usize| -> Result<(), &'static str> {
            if *pos >= (FS_SECTORS as usize) * BLOCK_SIZE {
                return Err("fs image overflow");
            }
            let sidx = *pos / BLOCK_SIZE;
            let off = *pos % BLOCK_SIZE;
            sec[off] = b;
            *pos += 1;
            if off + 1 == BLOCK_SIZE {
                blockdev::write_sector(dev, sidx as u32, sec)?;
                sec.fill(0);
            }
            Ok(())
        };

        sec.fill(0);
        for &b in MAGIC { put(b, &mut sec, dev, &mut pos)?; }
        for b in 1u32.to_le_bytes() { put(b, &mut sec, dev, &mut pos)?; }
        for b in (MAX_FILES as u32).to_le_bytes() { put(b, &mut sec, dev, &mut pos)?; }

        for f in &self.files {
            put(if f.used { 1 } else { 0 }, &mut sec, dev, &mut pos)?;
            put(f.path_len, &mut sec, dev, &mut pos)?;
            for b in [0u8, 0u8] { put(b, &mut sec, dev, &mut pos)?; }
            for b in f.data_len.to_le_bytes() { put(b, &mut sec, dev, &mut pos)?; }
            for &b in &f.path { put(b, &mut sec, dev, &mut pos)?; }
            for &b in &f.data { put(b, &mut sec, dev, &mut pos)?; }
        }

        if (pos % BLOCK_SIZE) != 0 {
            let sidx = pos / BLOCK_SIZE;
            blockdev::write_sector(dev, sidx as u32, &sec)?;
        }
        Ok(())
    }

    fn load_from_device(&mut self, dev: DeviceId) -> Result<(), &'static str> {
        let mut raw = [0u8; (FS_SECTORS as usize) * BLOCK_SIZE];
        let mut sec = [0u8; BLOCK_SIZE];
        for s in 0..FS_SECTORS {
            blockdev::read_sector(dev, s, &mut sec)?;
            let off = (s as usize) * BLOCK_SIZE;
            raw[off..off + BLOCK_SIZE].copy_from_slice(&sec);
        }

        if &raw[0..8] != MAGIC {
            self.files = [FileEntry::empty(); MAX_FILES];
            self.sync_to_device()?;
            return Ok(());
        }

        let mut pos = 8 + 4 + 4;
        let mut files = [FileEntry::empty(); MAX_FILES];
        for slot in &mut files {
            if pos + 1 + 1 + 2 + 4 + PATH_CAP + DATA_CAP > raw.len() {
                return Err("corrupted fs image");
            }
            let used = raw[pos] != 0;
            pos += 1;
            let path_len = raw[pos];
            pos += 1;
            pos += 2;
            let data_len = u32::from_le_bytes([raw[pos], raw[pos + 1], raw[pos + 2], raw[pos + 3]]);
            pos += 4;

            let mut path = [0u8; PATH_CAP];
            path.copy_from_slice(&raw[pos..pos + PATH_CAP]);
            pos += PATH_CAP;
            let mut data = [0u8; DATA_CAP];
            data.copy_from_slice(&raw[pos..pos + DATA_CAP]);
            pos += DATA_CAP;

            slot.used = used;
            slot.path_len = min(path_len as usize, PATH_CAP) as u8;
            slot.data_len = min(data_len as usize, DATA_CAP) as u32;
            slot.path = path;
            slot.data = data;
        }

        self.files = files;
        Ok(())
    }

    fn load_legacy_image(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        self.files = [FileEntry::empty(); MAX_FILES];
        let mut off = LEGACY_IMG_MAGIC.len();
        while off < bytes.len() {
            let mut end = off;
            while end < bytes.len() && bytes[end] != b'\n' {
                end += 1;
            }
            let line = &bytes[off..end];
            if !line.is_empty() {
                let Some(eq) = line.iter().position(|b| *b == b'=') else {
                    return Err("invalid legacy image");
                };
                let path = core::str::from_utf8(&line[..eq]).map_err(|_| "invalid legacy path")?;
                let content = &line[eq + 1..];
                if self.find_index(path).is_none() {
                    let Some(slot) = self.files.iter_mut().find(|f| !f.used) else {
                        return Err("legacy image too big");
                    };
                    if path.len() >= PATH_CAP || content.len() > DATA_CAP {
                        return Err("legacy entry too large");
                    }
                    slot.used = true;
                    slot.path_len = path.len() as u8;
                    slot.data_len = content.len() as u32;
                    slot.path[..path.len()].copy_from_slice(path.as_bytes());
                    slot.data[..content.len()].copy_from_slice(content);
                }
            }
            off = end.saturating_add(1);
        }
        Ok(())
    }

    fn find_index(&self, path: &str) -> Option<usize> {
        self.files.iter().enumerate().find_map(|(i, f)| {
            if !f.used || f.path_len as usize != path.len() {
                return None;
            }
            if &f.path[..f.path_len as usize] == path.as_bytes() {
                Some(i)
            } else {
                None
            }
        })
    }

    pub fn resolve_path<'a>(cwd: &str, input: &str, out: &'a mut [u8; PATH_CAP]) -> Result<&'a str, &'static str> {
        if input.is_empty() {
            return Err("empty path");
        }
        let mut raw = [0u8; PATH_CAP];
        let mut raw_len = 0usize;

        if input.as_bytes()[0] == b'/' {
            if input.len() >= PATH_CAP {
                return Err("path too long");
            }
            raw[..input.len()].copy_from_slice(input.as_bytes());
            raw_len = input.len();
        } else {
            if cwd.len() + 1 + input.len() >= PATH_CAP {
                return Err("path too long");
            }
            raw[..cwd.len()].copy_from_slice(cwd.as_bytes());
            raw_len += cwd.len();
            if raw_len == 0 || raw[raw_len - 1] != b'/' {
                raw[raw_len] = b'/';
                raw_len += 1;
            }
            raw[raw_len..raw_len + input.len()].copy_from_slice(input.as_bytes());
            raw_len += input.len();
        }

        let mut seg_starts = [0usize; 16];
        let mut seg_lens = [0usize; 16];
        let mut seg_count = 0usize;
        let mut i = 0usize;
        while i < raw_len {
            while i < raw_len && raw[i] == b'/' {
                i += 1;
            }
            if i >= raw_len {
                break;
            }
            let start = i;
            while i < raw_len && raw[i] != b'/' {
                i += 1;
            }
            let len = i - start;
            let seg = &raw[start..start + len];
            if seg == b"." {
                continue;
            }
            if seg == b".." {
                seg_count = seg_count.saturating_sub(1);
                continue;
            }
            if seg_count >= seg_starts.len() {
                return Err("path too deep");
            }
            seg_starts[seg_count] = start;
            seg_lens[seg_count] = len;
            seg_count += 1;
        }

        let mut out_len = 0usize;
        out[out_len] = b'/';
        out_len += 1;
        for s in 0..seg_count {
            if out_len + seg_lens[s] + 1 >= PATH_CAP {
                return Err("path too long");
            }
            let st = seg_starts[s];
            let ln = seg_lens[s];
            out[out_len..out_len + ln].copy_from_slice(&raw[st..st + ln]);
            out_len += ln;
            if s + 1 != seg_count {
                out[out_len] = b'/';
                out_len += 1;
            }
        }
        str::from_utf8(&out[..out_len]).map_err(|_| "invalid path")
    }

    pub fn is_dir(&self, path: &str) -> bool {
        if self.kind == FsKind::Fat32 {
            if self.is_mount_scaffold_dir(path) {
                return true;
            }
            let Some(fs) = self.fat else {
                return false;
            };
            let mut mapped = [0u8; PATH_CAP];
            let Some(inner) = self.map_path_to_mounted(path, &mut mapped) else {
                return false;
            };
            return fat32::is_dir(fs, inner);
        }
        if path == "/" {
            return true;
        }
        if self.read_file(path).is_some() {
            return false;
        }
        let mut found = false;
        self.list_paths(|p| {
            if found {
                return;
            }
            if p.starts_with(path) && p.len() > path.len() && p.as_bytes()[path.len()] == b'/' {
                found = true;
            }
        });
        found
    }

    pub fn list_dir<F: FnMut(&str)>(&self, cwd: &str, arg: &str, mut cb: F) -> Result<(), &'static str> {
        let target = if arg.is_empty() { cwd } else { arg };
        let mut abs = [0u8; PATH_CAP];
        let path = Self::resolve_path(cwd, target, &mut abs)?;
        if self.kind == FsKind::Fat32 {
            let Some(fs) = self.fat else {
                return Err("fat32 not mounted");
            };
            if let Some(name) = self.mount_scaffold_child(path) {
                cb(name);
                return Ok(());
            }
            let mut mapped = [0u8; PATH_CAP];
            let inner = self.map_path_to_mounted(path, &mut mapped).ok_or("no such file or directory")?;
            return fat32::list_dir(fs, inner, |name| cb(name));
        }
        if self.read_file(path).is_some() {
            cb(path.rsplit('/').next().unwrap_or(path));
            return Ok(());
        }
        if !self.is_dir(path) {
            return Err("no such file or directory");
        }

        let mut seen = [[0u8; 32]; 24];
        let mut seen_len = [0usize; 24];
        let mut seen_count = 0usize;

        self.list_paths(|p| {
            let rel = if path == "/" {
                if p.len() > 1 { &p[1..] } else { "" }
            } else if p.starts_with(path) && p.len() > path.len() && p.as_bytes()[path.len()] == b'/' {
                &p[path.len() + 1..]
            } else {
                return;
            };
            if rel.is_empty() {
                return;
            }
            let name = rel.split('/').next().unwrap_or(rel);
            if name.is_empty() || name.len() > 31 {
                return;
            }
            for i in 0..seen_count {
                if seen_len[i] == name.len() && &seen[i][..seen_len[i]] == name.as_bytes() {
                    return;
                }
            }
            if seen_count < seen.len() {
                seen[seen_count][..name.len()].copy_from_slice(name.as_bytes());
                seen_len[seen_count] = name.len();
                seen_count += 1;
            }
            cb(name);
        });
        Ok(())
    }

    fn set_mount_point(&mut self, target: &str, cwd: &str) -> Result<(), &'static str> {
        let mut abs = [0u8; PATH_CAP];
        let p = Self::resolve_path(cwd, target, &mut abs)?;
        if !p.starts_with("/mnt/") && p != "/" {
            return Err("mount target must be / or /mnt/<name>");
        }
        self.mount_point_len = p.len();
        self.mount_point[..self.mount_point_len].copy_from_slice(p.as_bytes());
        Ok(())
    }

    fn map_path_to_mounted<'a>(&self, vis_path: &str, out: &'a mut [u8; PATH_CAP]) -> Option<&'a str> {
        let mnt = self.mount_point();
        if mnt == "/" {
            let n = vis_path.len();
            if n >= PATH_CAP {
                return None;
            }
            out[..n].copy_from_slice(vis_path.as_bytes());
            return str::from_utf8(&out[..n]).ok();
        }
        if vis_path == mnt {
            out[0] = b'/';
            return str::from_utf8(&out[..1]).ok();
        }
        if vis_path.starts_with(mnt) && vis_path.len() > mnt.len() && vis_path.as_bytes()[mnt.len()] == b'/' {
            let suffix = &vis_path[mnt.len()..];
            if suffix.len() >= PATH_CAP {
                return None;
            }
            out[..suffix.len()].copy_from_slice(suffix.as_bytes());
            return str::from_utf8(&out[..suffix.len()]).ok();
        }
        None
    }

    fn join_visible_mount_path<'a>(&self, inner: &str, out: &'a mut [u8; PATH_CAP]) -> Option<&'a str> {
        let mnt = self.mount_point();
        if mnt == "/" {
            let n = inner.len();
            if n >= PATH_CAP {
                return None;
            }
            out[..n].copy_from_slice(inner.as_bytes());
            return str::from_utf8(&out[..n]).ok();
        }
        let suffix = if inner == "/" { "" } else { inner };
        let total = mnt.len() + suffix.len();
        if total >= PATH_CAP {
            return None;
        }
        out[..mnt.len()].copy_from_slice(mnt.as_bytes());
        if !suffix.is_empty() {
            out[mnt.len()..total].copy_from_slice(suffix.as_bytes());
        }
        str::from_utf8(&out[..total]).ok()
    }

    fn is_mount_scaffold_dir(&self, path: &str) -> bool {
        let mnt = self.mount_point();
        if mnt == "/" {
            return path == "/";
        }
        if path == "/" || path == mnt {
            return true;
        }
        mnt.starts_with(path)
            && mnt.len() > path.len()
            && path != "/"
            && mnt.as_bytes()[path.len()] == b'/'
    }

    fn mount_scaffold_child<'a>(&'a self, path: &str) -> Option<&'a str> {
        let mnt = self.mount_point();
        if mnt == "/" {
            return None;
        }
        if path == "/" {
            return mnt.strip_prefix('/').and_then(|s| s.split('/').next());
        }
        if mnt.starts_with(path)
            && mnt.len() > path.len()
            && path != "/"
            && mnt.as_bytes()[path.len()] == b'/'
        {
            let rest = &mnt[path.len() + 1..];
            return rest.split('/').next();
        }
        None
    }
}
