use crate::arch;
use crate::drivers::usb;
use crate::userland::{shell, Context};

pub fn uname(_ctx: &mut Context, _args: &str) {
    shell::println("Smallix 0.2.0 i686");
}

pub fn lsdev(ctx: &mut Context, _args: &str) {
    shell::println("driver bus:");
    shell::println(" - vga-text @ /dev/console");
    shell::println(" - uart16550 @ /dev/tty0");
    shell::println(" - kernel log @ /dev/kmsg");
    shell::println(" - null sink @ /dev/null");
    shell::println(" - net stack @ /dev/net0");
    shell::println(" - block ram @ /dev/ramfs");
    shell::println(" - block ata @ /dev/hda");
    shell::println(" - block loop @ /dev/loop0");
    shell::println(" - block usb @ /dev/usb0");
    if usb::is_ready() {
        shell::println("   usb controller detected");
    } else {
        shell::println("   usb controller not detected");
    }
    if let Some((mac, irq)) = ctx.dev.net_nic_info() {
        shell::println_fmt(format_args!(
            "   rtl8139 mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} irq={}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], irq
        ));
    } else {
        shell::println("   rtl8139 not initialized");
    }
}

pub fn cfg(_ctx: &mut Context, _args: &str) {
    shell::println("config files:");
    shell::println(" - /etc/system.cfg");
    shell::println(" - /etc/init.rc");
    shell::println(" - /etc/net.cfg");
}

pub fn halt(_ctx: &mut Context, _args: &str) {
    shell::println("halting cpu");
    arch::halt_loop();
}

pub fn reboot(_ctx: &mut Context, _args: &str) {
    shell::println("rebooting");
    arch::reboot();
}
