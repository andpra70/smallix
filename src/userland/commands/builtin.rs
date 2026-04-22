use crate::drivers::vga;
use crate::userland::{commands, shell, Context};

pub fn help(_ctx: &mut Context, _args: &str) {
    shell::println("commands:");
    for cmd in commands::list_commands() {
        shell::print(" - ");
        shell::print(cmd.name);
        shell::print(": ");
        shell::println(cmd.help);
    }
}

pub fn echo(_ctx: &mut Context, args: &str) {
    shell::println(args);
}

pub fn clear(_ctx: &mut Context, _args: &str) {
    vga::clear();
}
