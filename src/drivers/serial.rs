use crate::arch::{inb, io_wait, outb};

const COM1: u16 = 0x3F8;

pub fn init() {
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x80);
    outb(COM1 + 0, 0x03);
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03);
    outb(COM1 + 2, 0xC7);
    outb(COM1 + 4, 0x0B);
    io_wait();
}

fn is_tx_empty() -> bool {
    (inb(COM1 + 5) & 0x20) != 0
}

pub fn write_byte(byte: u8) {
    while !is_tx_empty() {}
    outb(COM1, byte);
}

pub fn write_str(s: &str) {
    for b in s.bytes() {
        write_byte(b);
    }
}
