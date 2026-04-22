#![no_std]
#![no_main]

mod arch;
mod drivers;
mod userland;

use core::arch::global_asm;

use arch::{disable_pic_interrupts, halt_loop};
use drivers::{keyboard, net::rtl8139, serial, usb, vga};

#[repr(align(16))]
struct Stack([u8; 131072]);

#[no_mangle]
static mut BOOT_STACK: Stack = Stack([0; 131072]);

global_asm!(
    r#"
.section .multiboot, "a"
.align 8
.long 0xE85250D6
.long 0
.long 24
.long 0x17ADAF12
.short 0
.short 0
.long 8

.section .text
.global _start
.type _start, @function
_start:
    cli
    cld
    lea esp, [BOOT_STACK + 131072]
    and esp, 0xFFFFFFF0
    call rust_main
1:
    hlt
    jmp 1b
"#
);

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    vga::init();
    serial::init();
    keyboard::init();
    usb::init();
    disable_pic_interrupts();
    let _ = rtl8139::init();

    serial::write_str("smallix: rust_main entered\n");
    serial::write_str("smallix: clearing vga\n");
    vga::clear();
    serial::write_str("smallix: jumping to init\n");
    userland::init::start();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    vga::println("KERNEL PANIC");
    vga::print("message: ");
    vga::write_fmt(format_args!("{}\n", info.message()));
    serial::write_str("KERNEL PANIC: ");
    serial::write_str("see VGA message\n");
    halt_loop()
}
