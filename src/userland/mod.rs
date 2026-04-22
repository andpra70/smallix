pub mod blockdev;
pub mod commands;
pub mod dev;
pub mod exec;
pub mod fat32;
pub mod init;
pub mod net;
pub mod procfs;
pub mod sched;
pub mod shell;
pub mod syscall;
pub mod vfs;

use dev::DevFs;
use sched::Scheduler;
use vfs::Vfs;

pub struct Context {
    pub fs: Vfs,
    pub dev: DevFs,
    pub sched: Scheduler,
    pub hostname: &'static str,
    cwd: [u8; 64],
    cwd_len: usize,
    pub errno: i32,
    pub last_exit_code: i32,
}

impl Context {
    pub fn new() -> Self {
        let mut cwd = [0u8; 64];
        cwd[0] = b'/';
        let mut ctx = Self {
            fs: Vfs::new(),
            dev: DevFs::new(),
            sched: Scheduler::new(),
            hostname: "smallix",
            cwd,
            cwd_len: 1,
            errno: 0,
            last_exit_code: 0,
        };
        ctx.sched.bootstrap();
        ctx
    }

    pub fn cwd(&self) -> &str {
        core::str::from_utf8(&self.cwd[..self.cwd_len]).unwrap_or("/")
    }

    pub fn set_cwd(&mut self, path: &str) -> Result<(), &'static str> {
        if path.is_empty() || path.len() >= self.cwd.len() {
            return Err("invalid cwd");
        }
        self.cwd_len = path.len();
        self.cwd[..self.cwd_len].copy_from_slice(path.as_bytes());
        Ok(())
    }

    pub fn set_errno(&mut self, code: i32) {
        self.errno = code;
    }
}
