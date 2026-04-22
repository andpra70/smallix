use core::arch::asm;

#[inline]
pub fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
pub fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
pub fn outl(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
pub fn inb(port: u16) -> u8 {
    let mut value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
pub fn inw(port: u16) -> u16 {
    let mut value: u16;
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
pub fn inl(port: u16) -> u32 {
    let mut value: u32;
    unsafe {
        asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
pub fn io_wait() {
    outb(0x80, 0);
}

#[inline]
pub fn halt() {
    unsafe {
        asm!("hlt", options(nomem, nostack));
    }
}

pub fn halt_loop() -> ! {
    loop {
        halt();
    }
}

pub fn reboot() -> ! {
    loop {
        if inb(0x64) & 0x02 == 0 {
            break;
        }
        io_wait();
    }
    outb(0x64, 0xFE);
    halt_loop()
}

pub fn disable_pic_interrupts() {
    outb(0x21, 0xFF);
    outb(0xA1, 0xFF);
}
