pub mod x86_64;

pub use x86_64::{
    disable_pic_interrupts, halt_loop, inb, inl, inw, io_wait, outb, outl, outw, reboot,
};
