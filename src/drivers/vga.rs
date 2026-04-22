use core::fmt;
use core::fmt::Write;

const VGA_BUFFER: usize = 0xB8000;
const WIDTH: usize = 80;
const HEIGHT: usize = 25;

struct Writer {
    col: usize,
    row: usize,
    color: u8,
}

static mut WRITER: Writer = Writer {
    col: 0,
    row: 0,
    color: 0x0F,
};

pub fn init() {
    clear();
}

pub fn clear() {
    unsafe {
        for r in 0..HEIGHT {
            for c in 0..WIDTH {
                write_at(r, c, b' ', WRITER.color);
            }
        }
        WRITER.col = 0;
        WRITER.row = 0;
    }
}

fn write_at(row: usize, col: usize, ch: u8, color: u8) {
    let idx = (row * WIDTH + col) * 2;
    unsafe {
        let ptr = VGA_BUFFER as *mut u8;
        core::ptr::write_volatile(ptr.add(idx), ch);
        core::ptr::write_volatile(ptr.add(idx + 1), color);
    }
}

fn newline(writer: &mut Writer) {
    writer.col = 0;
    if writer.row < HEIGHT - 1 {
        writer.row += 1;
    } else {
        for r in 1..HEIGHT {
            for c in 0..WIDTH {
                let src_idx = (r * WIDTH + c) * 2;
                let dst_idx = ((r - 1) * WIDTH + c) * 2;
                unsafe {
                    let ptr = VGA_BUFFER as *mut u8;
                    let ch = core::ptr::read_volatile(ptr.add(src_idx));
                    let color = core::ptr::read_volatile(ptr.add(src_idx + 1));
                    core::ptr::write_volatile(ptr.add(dst_idx), ch);
                    core::ptr::write_volatile(ptr.add(dst_idx + 1), color);
                }
            }
        }
        for c in 0..WIDTH {
            write_at(HEIGHT - 1, c, b' ', writer.color);
        }
    }
}

fn write_byte(writer: &mut Writer, byte: u8) {
    match byte {
        b'\n' => newline(writer),
        0x08 => {
            if writer.col > 0 {
                writer.col -= 1;
                write_at(writer.row, writer.col, b' ', writer.color);
            }
        }
        _ => {
            write_at(writer.row, writer.col, byte, writer.color);
            writer.col += 1;
            if writer.col >= WIDTH {
                newline(writer);
            }
        }
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            write_byte(self, b);
        }
        Ok(())
    }
}

pub fn write_fmt(args: fmt::Arguments<'_>) {
    unsafe {
        let _ = WRITER.write_fmt(args);
    }
}

pub fn print(s: &str) {
    unsafe {
        let _ = WRITER.write_str(s);
    }
}

pub fn println(s: &str) {
    write_fmt(format_args!("{}\n", s));
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::drivers::vga::write_fmt(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::drivers::vga::write_fmt(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::drivers::vga::write_fmt(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}
