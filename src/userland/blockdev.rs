use crate::drivers::{ata, usb};

pub const BLOCK_SIZE: usize = 512;
pub const FS_BASE_LBA: u32 = 4096;

pub const RAM_SECTORS: usize = 256;
pub const LOOP_SECTORS: usize = 256;
const RAM_BYTES: usize = RAM_SECTORS * BLOCK_SIZE;
const LOOP_BYTES: usize = LOOP_SECTORS * BLOCK_SIZE;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceId {
    Ram,
    Hda,
    HdaRaw,
    Loop0,
    Usb0,
}

static mut RAM_DISK: [u8; RAM_BYTES] = [0; RAM_BYTES];
static mut LOOP_DISK: [u8; LOOP_BYTES] = [0; LOOP_BYTES];

pub fn read_sector(dev: DeviceId, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    match dev {
        DeviceId::Ram => {
            if lba >= RAM_SECTORS as u32 {
                return Err("lba out of range");
            }
            let off = (lba as usize) * BLOCK_SIZE;
            unsafe {
                out.copy_from_slice(&RAM_DISK[off..off + BLOCK_SIZE]);
            }
            Ok(())
        }
        DeviceId::Loop0 => {
            if lba >= LOOP_SECTORS as u32 {
                return Err("lba out of range");
            }
            let off = (lba as usize) * BLOCK_SIZE;
            unsafe {
                out.copy_from_slice(&LOOP_DISK[off..off + BLOCK_SIZE]);
            }
            Ok(())
        }
        DeviceId::Hda => ata::read_sector(FS_BASE_LBA.saturating_add(lba), out),
        DeviceId::HdaRaw => ata::read_sector(lba, out),
        DeviceId::Usb0 => usb::read_sector(lba, out),
    }
}

pub fn write_sector(dev: DeviceId, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    match dev {
        DeviceId::Ram => {
            if lba >= RAM_SECTORS as u32 {
                return Err("lba out of range");
            }
            let off = (lba as usize) * BLOCK_SIZE;
            unsafe {
                RAM_DISK[off..off + BLOCK_SIZE].copy_from_slice(data);
            }
            Ok(())
        }
        DeviceId::Loop0 => {
            if lba >= LOOP_SECTORS as u32 {
                return Err("lba out of range");
            }
            let off = (lba as usize) * BLOCK_SIZE;
            unsafe {
                LOOP_DISK[off..off + BLOCK_SIZE].copy_from_slice(data);
            }
            Ok(())
        }
        DeviceId::Hda => ata::write_sector(FS_BASE_LBA.saturating_add(lba), data),
        DeviceId::HdaRaw => ata::write_sector(lba, data),
        DeviceId::Usb0 => usb::write_sector(lba, data),
    }
}

pub fn load_loop_image(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() > LOOP_BYTES {
        return Err("loop image too large");
    }
    unsafe {
        LOOP_DISK.fill(0);
        LOOP_DISK[..bytes.len()].copy_from_slice(bytes);
    }
    Ok(())
}

pub fn load_ram_image(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() > RAM_BYTES {
        return Err("ram image too large");
    }
    unsafe {
        RAM_DISK.fill(0);
        RAM_DISK[..bytes.len()].copy_from_slice(bytes);
    }
    Ok(())
}

pub fn dump_loop_image(out: &mut [u8]) -> usize {
    let n = core::cmp::min(out.len(), LOOP_BYTES);
    unsafe {
        out[..n].copy_from_slice(&LOOP_DISK[..n]);
    }
    n
}
