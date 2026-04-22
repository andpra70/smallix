use crate::arch::{inb, inw, io_wait, outb, outw};

const ATA_PRIMARY_IO: u16 = 0x1F0;
const ATA_PRIMARY_CTRL: u16 = 0x3F6;

const ATA_REG_DATA: u16 = 0x00;
const ATA_REG_ERROR: u16 = 0x01;
const ATA_REG_SECCOUNT0: u16 = 0x02;
const ATA_REG_LBA0: u16 = 0x03;
const ATA_REG_LBA1: u16 = 0x04;
const ATA_REG_LBA2: u16 = 0x05;
const ATA_REG_HDDEVSEL: u16 = 0x06;
const ATA_REG_COMMAND: u16 = 0x07;
const ATA_REG_STATUS: u16 = 0x07;

const ATA_CMD_READ_PIO: u8 = 0x20;
const ATA_CMD_WRITE_PIO: u8 = 0x30;
const ATA_CMD_CACHE_FLUSH: u8 = 0xE7;

const ATA_SR_BSY: u8 = 0x80;
const ATA_SR_DRDY: u8 = 0x40;
const ATA_SR_DRQ: u8 = 0x08;
const ATA_SR_ERR: u8 = 0x01;

pub fn read_sector(lba: u32, out: &mut [u8; 512]) -> Result<(), &'static str> {
    if lba > 0x0FFF_FFFF {
        return Err("lba out of range");
    }

    ata_select_lba28(lba, 1, ATA_CMD_READ_PIO)?;
    ata_wait_data_ready()?;

    for i in 0..256 {
        let w = inw(ATA_PRIMARY_IO + ATA_REG_DATA);
        out[i * 2] = (w & 0xFF) as u8;
        out[i * 2 + 1] = (w >> 8) as u8;
    }

    Ok(())
}

pub fn write_sector(lba: u32, data: &[u8; 512]) -> Result<(), &'static str> {
    if lba > 0x0FFF_FFFF {
        return Err("lba out of range");
    }

    ata_select_lba28(lba, 1, ATA_CMD_WRITE_PIO)?;
    ata_wait_data_ready()?;

    for i in 0..256 {
        let lo = data[i * 2] as u16;
        let hi = (data[i * 2 + 1] as u16) << 8;
        outw(ATA_PRIMARY_IO + ATA_REG_DATA, lo | hi);
    }

    outb(ATA_PRIMARY_IO + ATA_REG_COMMAND, ATA_CMD_CACHE_FLUSH);
    ata_wait_not_busy()?;
    Ok(())
}

fn ata_select_lba28(lba: u32, count: u8, cmd: u8) -> Result<(), &'static str> {
    ata_wait_not_busy()?;

    outb(ATA_PRIMARY_CTRL, 0x00);
    outb(
        ATA_PRIMARY_IO + ATA_REG_HDDEVSEL,
        0xE0 | (((lba >> 24) as u8) & 0x0F),
    );
    io_wait();

    outb(ATA_PRIMARY_IO + ATA_REG_SECCOUNT0, count);
    outb(ATA_PRIMARY_IO + ATA_REG_LBA0, (lba & 0xFF) as u8);
    outb(ATA_PRIMARY_IO + ATA_REG_LBA1, ((lba >> 8) & 0xFF) as u8);
    outb(ATA_PRIMARY_IO + ATA_REG_LBA2, ((lba >> 16) & 0xFF) as u8);
    outb(ATA_PRIMARY_IO + ATA_REG_COMMAND, cmd);
    Ok(())
}

fn ata_wait_not_busy() -> Result<(), &'static str> {
    for _ in 0..200_000 {
        let s = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
        if (s & ATA_SR_BSY) == 0 {
            return Ok(());
        }
        io_wait();
    }
    Err("ata timeout")
}

fn ata_wait_data_ready() -> Result<(), &'static str> {
    for _ in 0..200_000 {
        let s = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
        if (s & ATA_SR_ERR) != 0 {
            let _ = inb(ATA_PRIMARY_IO + ATA_REG_ERROR);
            return Err("ata io error");
        }
        if (s & ATA_SR_BSY) == 0 && (s & ATA_SR_DRQ) != 0 && (s & ATA_SR_DRDY) != 0 {
            return Ok(());
        }
        io_wait();
    }
    Err("ata data timeout")
}
