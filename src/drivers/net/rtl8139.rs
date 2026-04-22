use crate::arch::{inb, inl, inw, io_wait, outb, outl, outw};

const PCI_ADDR: u16 = 0xCF8;
const PCI_DATA: u16 = 0xCFC;

const RTL_VENDOR: u16 = 0x10EC;
const RTL_DEVICE: u16 = 0x8139;

const REG_IDR0: u16 = 0x00;
const REG_TSD0: u16 = 0x10;
const REG_TSAD0: u16 = 0x20;
const REG_RBSTART: u16 = 0x30;
const REG_CAPR: u16 = 0x38;
const REG_CBR: u16 = 0x3A;
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_TCR: u16 = 0x40;
const REG_RCR: u16 = 0x44;
const REG_CMD: u16 = 0x37;
const REG_CONFIG1: u16 = 0x52;

const RX_BUF_SIZE: usize = 8192 + 16 + 1500;

#[repr(align(16))]
struct RxBuf([u8; RX_BUF_SIZE]);

#[repr(align(16))]
struct TxBuf([u8; 2048]);

static mut RX_BUF: RxBuf = RxBuf([0; RX_BUF_SIZE]);
static mut TX_BUFS: [TxBuf; 4] = [TxBuf([0; 2048]), TxBuf([0; 2048]), TxBuf([0; 2048]), TxBuf([0; 2048])];

#[derive(Clone, Copy)]
struct Driver {
    io_base: u16,
    irq: u8,
    mac: [u8; 6],
    rx_off: usize,
    tx_idx: usize,
    ready: bool,
}

static mut DRIVER: Driver = Driver {
    io_base: 0,
    irq: 0,
    mac: [0; 6],
    rx_off: 0,
    tx_idx: 0,
    ready: false,
};

pub fn init() -> Result<(), &'static str> {
    let Some((bus, dev, func)) = pci_find_device(RTL_VENDOR, RTL_DEVICE) else {
        return Err("rtl8139 pci device not found");
    };

    let bar0 = pci_read_u32(bus, dev, func, 0x10);
    if bar0 == 0 || (bar0 & 1) == 0 {
        return Err("rtl8139 invalid io bar");
    }
    let io_base = (bar0 & 0xFFFC) as u16;

    let mut cmd = pci_read_u16(bus, dev, func, 0x04);
    cmd |= 0x0005;
    pci_write_u16(bus, dev, func, 0x04, cmd);

    let irq = (pci_read_u32(bus, dev, func, 0x3C) & 0xFF) as u8;

    outb(io_base + REG_CONFIG1, 0x00);
    outb(io_base + REG_CMD, 0x10);
    for _ in 0..10000 {
        if (inb(io_base + REG_CMD) & 0x10) == 0 {
            break;
        }
        io_wait();
    }

    let rbstart = unsafe { RX_BUF.0.as_ptr() as usize as u32 };
    outl(io_base + REG_RBSTART, rbstart);

    outw(io_base + REG_IMR, 0x0000);
    outw(io_base + REG_ISR, 0xFFFF);

    outl(io_base + REG_RCR, 0x0000_8F0F);
    outl(io_base + REG_TCR, 0x0300_0700);

    outb(io_base + REG_CMD, 0x0C);

    let mut mac = [0u8; 6];
    for (i, b) in mac.iter_mut().enumerate() {
        *b = inb(io_base + REG_IDR0 + i as u16);
    }

    unsafe {
        DRIVER.io_base = io_base;
        DRIVER.irq = irq;
        DRIVER.mac = mac;
        DRIVER.rx_off = 0;
        DRIVER.tx_idx = 0;
        DRIVER.ready = true;
    }

    Ok(())
}

pub fn is_ready() -> bool {
    unsafe { DRIVER.ready }
}

pub fn mac_addr() -> [u8; 6] {
    unsafe { DRIVER.mac }
}

pub fn has_rx() -> bool {
    unsafe {
        if !DRIVER.ready {
            return false;
        }
        (inb(DRIVER.io_base + REG_CMD) & 0x01) == 0
    }
}

pub fn send_frame(frame: &[u8]) -> Result<(), &'static str> {
    unsafe {
        if !DRIVER.ready {
            return Err("rtl8139 not initialized");
        }
        if frame.len() > 1500 {
            return Err("frame too large");
        }

        let idx = DRIVER.tx_idx;
        DRIVER.tx_idx = (DRIVER.tx_idx + 1) & 0x03;

        TX_BUFS[idx].0[..frame.len()].copy_from_slice(frame);

        let buf_addr = TX_BUFS[idx].0.as_ptr() as usize as u32;
        outl(DRIVER.io_base + REG_TSAD0 + (idx as u16) * 4, buf_addr);
        outl(DRIVER.io_base + REG_TSD0 + (idx as u16) * 4, frame.len() as u32);

        Ok(())
    }
}

pub fn recv_frame(out: &mut [u8]) -> Option<usize> {
    unsafe {
        if !DRIVER.ready {
            return None;
        }

        if (inb(DRIVER.io_base + REG_CMD) & 0x01) != 0 {
            return None;
        }

        let cbr = inw(DRIVER.io_base + REG_CBR) as usize;
        let _ = cbr;

        let hdr0 = rx_read_u16(DRIVER.rx_off);
        let hdr1 = rx_read_u16(DRIVER.rx_off + 2);
        let status = hdr0;
        let length = hdr1 as usize;

        if status == 0 || length < 4 || length > 1800 {
            DRIVER.rx_off = 0;
            outw(DRIVER.io_base + REG_CAPR, 0);
            outw(DRIVER.io_base + REG_ISR, 0xFFFF);
            return None;
        }

        let payload_len = length.saturating_sub(4);
        let copy_len = core::cmp::min(out.len(), payload_len);
        rx_copy(DRIVER.rx_off + 4, &mut out[..copy_len]);

        DRIVER.rx_off = (DRIVER.rx_off + length + 4 + 3) & !3;
        DRIVER.rx_off %= 8192;

        let capr = DRIVER.rx_off.wrapping_sub(16) as u16;
        outw(DRIVER.io_base + REG_CAPR, capr);
        outw(DRIVER.io_base + REG_ISR, 0xFFFF);

        Some(copy_len)
    }
}

pub fn irq_line() -> Option<u8> {
    unsafe {
        if !DRIVER.ready {
            None
        } else {
            Some(DRIVER.irq)
        }
    }
}

fn rx_read_u16(off: usize) -> u16 {
    let b0 = rx_read_u8(off) as u16;
    let b1 = rx_read_u8(off + 1) as u16;
    b0 | (b1 << 8)
}

fn rx_read_u8(off: usize) -> u8 {
    unsafe { RX_BUF.0[off % 8192] }
}

fn rx_copy(off: usize, dst: &mut [u8]) {
    for (i, b) in dst.iter_mut().enumerate() {
        *b = rx_read_u8(off + i);
    }
}

fn pci_addr(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
}

fn pci_read_u32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    outl(PCI_ADDR, pci_addr(bus, dev, func, offset));
    inl(PCI_DATA)
}

fn pci_read_u16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let v = pci_read_u32(bus, dev, func, offset & !0x2);
    if (offset & 0x2) == 0 {
        (v & 0xFFFF) as u16
    } else {
        (v >> 16) as u16
    }
}

fn pci_write_u16(bus: u8, dev: u8, func: u8, offset: u8, val: u16) {
    let base = offset & !0x2;
    let old = pci_read_u32(bus, dev, func, base);
    let newv = if (offset & 0x2) == 0 {
        (old & 0xFFFF_0000) | (val as u32)
    } else {
        (old & 0x0000_FFFF) | ((val as u32) << 16)
    };
    outl(PCI_ADDR, pci_addr(bus, dev, func, base));
    outl(PCI_DATA, newv);
}

fn pci_find_device(vendor: u16, device: u16) -> Option<(u8, u8, u8)> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let vd = pci_read_u32(bus, dev, func, 0x00);
                let ven = (vd & 0xFFFF) as u16;
                if ven == 0xFFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let dev_id = (vd >> 16) as u16;
                if ven == vendor && dev_id == device {
                    return Some((bus, dev, func));
                }
            }
        }
    }
    None
}
