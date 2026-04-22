use crate::arch::{inl, outl};

const PCI_ADDR: u16 = 0xCF8;
const PCI_DATA: u16 = 0xCFC;

pub const BLOCK_SIZE: usize = 512;
pub const USB_SECTORS: usize = 256;
const USB_BYTES: usize = BLOCK_SIZE * USB_SECTORS;

static mut USB_DISK: [u8; USB_BYTES] = [0; USB_BYTES];
static mut USB_READY: bool = false;

pub fn init() {
    unsafe {
        USB_READY = pci_find_usb_controller().is_some();
    }
}

pub fn is_ready() -> bool {
    unsafe { USB_READY }
}

pub fn read_sector(lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    unsafe {
        if !USB_READY {
            return Err("usb controller not found");
        }
        if lba >= USB_SECTORS as u32 {
            return Err("lba out of range");
        }
        let off = (lba as usize) * BLOCK_SIZE;
        out.copy_from_slice(&USB_DISK[off..off + BLOCK_SIZE]);
    }
    Ok(())
}

pub fn write_sector(lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    unsafe {
        if !USB_READY {
            return Err("usb controller not found");
        }
        if lba >= USB_SECTORS as u32 {
            return Err("lba out of range");
        }
        let off = (lba as usize) * BLOCK_SIZE;
        USB_DISK[off..off + BLOCK_SIZE].copy_from_slice(data);
    }
    Ok(())
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

fn pci_find_usb_controller() -> Option<(u8, u8, u8)> {
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

                let class_reg = pci_read_u32(bus, dev, func, 0x08);
                let class_code = ((class_reg >> 24) & 0xFF) as u8;
                let subclass = ((class_reg >> 16) & 0xFF) as u8;
                if class_code == 0x0C && subclass == 0x03 {
                    return Some((bus, dev, func));
                }
            }
        }
    }
    None
}
