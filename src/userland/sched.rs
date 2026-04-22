#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Running,
    Ready,
    Zombie,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Running,
    Ready,
    Blocked,
    Zombie,
}

#[derive(Clone, Copy)]
pub struct Process {
    pub used: bool,
    pub pid: u16,
    pub ppid: u16,
    pub state: ProcState,
    pub pending_signals: u32,
    pub exit_code: i32,
    pub name: [u8; 16],
    pub name_len: usize,
}

#[derive(Clone, Copy)]
pub struct Thread {
    pub used: bool,
    pub tid: u16,
    pub pid: u16,
    pub state: ThreadState,
    pub runtime_ticks: u32,
    pub name: [u8; 16],
    pub name_len: usize,
}

const MAX_PROCS: usize = 64;
const MAX_THREADS: usize = 256;

pub struct Scheduler {
    procs: [Process; MAX_PROCS],
    threads: [Thread; MAX_THREADS],
    next_pid: u16,
    next_tid: u16,
    current_tid: Option<u16>,
    ticks: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        const EMPTY_PROC: Process = Process {
            used: false,
            pid: 0,
            ppid: 0,
            state: ProcState::Ready,
            pending_signals: 0,
            exit_code: 0,
            name: [0; 16],
            name_len: 0,
        };
        const EMPTY_THREAD: Thread = Thread {
            used: false,
            tid: 0,
            pid: 0,
            state: ThreadState::Ready,
            runtime_ticks: 0,
            name: [0; 16],
            name_len: 0,
        };

        Self {
            procs: [EMPTY_PROC; MAX_PROCS],
            threads: [EMPTY_THREAD; MAX_THREADS],
            next_pid: 1,
            next_tid: 1,
            current_tid: None,
            ticks: 0,
        }
    }

    pub fn bootstrap(&mut self) {
        if self.next_pid != 1 || self.next_tid != 1 {
            return;
        }
        let init_pid = self.spawn_process_internal(0, "init").unwrap_or(1);
        let _ = self.spawn_thread_internal(init_pid, "init-main");
        let _ = self.spawn_thread_internal(init_pid, "shell");
    }

    pub fn spawn_process(&mut self, parent: u16, name: &str) -> Result<u16, &'static str> {
        self.spawn_process_internal(parent, name)
    }

    pub fn spawn_thread(&mut self, pid: u16, name: &str) -> Result<u16, &'static str> {
        if self.find_proc_index(pid).is_none() {
            return Err("no such pid");
        }
        self.spawn_thread_internal(pid, name)
    }

    pub fn kill_process(&mut self, pid: u16) -> Result<(), &'static str> {
        let Some(pidx) = self.find_proc_index(pid) else {
            return Err("no such pid");
        };

        self.procs[pidx].state = ProcState::Zombie;

        for thr in &mut self.threads {
            if thr.used && thr.pid == pid {
                thr.state = ThreadState::Zombie;
            }
        }

        Ok(())
    }

    pub fn exit_process(&mut self, pid: u16, code: i32) -> Result<(), &'static str> {
        let Some(pidx) = self.find_proc_index(pid) else {
            return Err("no such pid");
        };
        self.procs[pidx].exit_code = code;
        self.kill_process(pid)
    }

    pub fn tick(&mut self) {
        self.ticks = self.ticks.saturating_add(1);

        if let Some(cur_tid) = self.current_tid {
            if let Some(idx) = self.find_thread_index(cur_tid) {
                if self.threads[idx].state == ThreadState::Running {
                    self.threads[idx].state = ThreadState::Ready;
                }
            }
        }

        let next = self.next_ready_thread();
        self.current_tid = next;

        if let Some(tid) = next {
            if let Some(tidx) = self.find_thread_index(tid) {
                self.threads[tidx].state = ThreadState::Running;
                self.threads[tidx].runtime_ticks =
                    self.threads[tidx].runtime_ticks.saturating_add(1);

                if let Some(pidx) = self.find_proc_index(self.threads[tidx].pid) {
                    self.procs[pidx].state = ProcState::Running;
                }
            }
        }

        self.recompute_proc_states();
    }

    pub fn run_ticks(&mut self, count: u32) {
        for _ in 0..count {
            self.tick();
        }
    }

    pub fn block_thread(&mut self, tid: u16) -> Result<(), &'static str> {
        let Some(idx) = self.find_thread_index(tid) else {
            return Err("no such tid");
        };
        self.threads[idx].state = ThreadState::Blocked;
        if self.current_tid == Some(tid) {
            self.current_tid = None;
        }
        Ok(())
    }

    pub fn wake_thread(&mut self, tid: u16) -> Result<(), &'static str> {
        let Some(idx) = self.find_thread_index(tid) else {
            return Err("no such tid");
        };
        if self.threads[idx].state == ThreadState::Blocked {
            self.threads[idx].state = ThreadState::Ready;
        }
        Ok(())
    }

    pub fn proc_iter(&self) -> impl Iterator<Item = Process> + '_ {
        self.procs.iter().copied().filter(|p| p.used)
    }

    pub fn thread_iter(&self) -> impl Iterator<Item = Thread> + '_ {
        self.threads.iter().copied().filter(|t| t.used)
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn self_test(&mut self) -> bool {
        let p = self.spawn_process(1, "worker").ok();
        let Some(pid) = p else {
            return false;
        };
        let t1 = self.spawn_thread(pid, "w1").ok();
        let t2 = self.spawn_thread(pid, "w2").ok();
        if t1.is_none() || t2.is_none() {
            return false;
        }

        self.run_ticks(10);

        let mut seen = 0usize;
        for t in self.thread_iter() {
            if t.pid == pid && t.runtime_ticks > 0 {
                seen += 1;
            }
        }

        if seen < 2 {
            return false;
        }

        self.kill_process(pid).is_ok()
    }

    fn spawn_process_internal(&mut self, parent: u16, name: &str) -> Result<u16, &'static str> {
        let Some(slot) = self.procs.iter_mut().find(|p| !p.used) else {
            return Err("proc table full");
        };

        let pid = self.next_pid;
        self.next_pid = self.next_pid.saturating_add(1);

        slot.used = true;
        slot.pid = pid;
        slot.ppid = parent;
        slot.state = ProcState::Ready;
        slot.pending_signals = 0;
        slot.exit_code = 0;
        copy_name(&mut slot.name, &mut slot.name_len, name);

        Ok(pid)
    }

    fn spawn_thread_internal(&mut self, pid: u16, name: &str) -> Result<u16, &'static str> {
        let Some(slot) = self.threads.iter_mut().find(|t| !t.used) else {
            return Err("thread table full");
        };

        let tid = self.next_tid;
        self.next_tid = self.next_tid.saturating_add(1);

        slot.used = true;
        slot.tid = tid;
        slot.pid = pid;
        slot.state = ThreadState::Ready;
        slot.runtime_ticks = 0;
        copy_name(&mut slot.name, &mut slot.name_len, name);

        Ok(tid)
    }

    fn find_proc_index(&self, pid: u16) -> Option<usize> {
        self.procs
            .iter()
            .enumerate()
            .find_map(|(i, p)| if p.used && p.pid == pid { Some(i) } else { None })
    }

    fn find_thread_index(&self, tid: u16) -> Option<usize> {
        self.threads
            .iter()
            .enumerate()
            .find_map(|(i, t)| if t.used && t.tid == tid { Some(i) } else { None })
    }

    fn next_ready_thread(&self) -> Option<u16> {
        let start = self.current_tid.unwrap_or(0);

        let mut best: Option<u16> = None;
        for t in self.thread_iter() {
            if t.state != ThreadState::Ready && t.state != ThreadState::Running {
                continue;
            }
            if t.tid > start {
                best = Some(t.tid);
                break;
            }
        }

        if best.is_some() {
            return best;
        }

        self.thread_iter()
            .find_map(|t| {
                if t.state == ThreadState::Ready || t.state == ThreadState::Running {
                    Some(t.tid)
                } else {
                    None
                }
            })
    }

    fn recompute_proc_states(&mut self) {
        for p in &mut self.procs {
            if !p.used || p.state == ProcState::Zombie {
                continue;
            }

            let mut has_running = false;
            let mut has_ready = false;
            let mut has_live = false;

            for t in &self.threads {
                if !t.used || t.pid != p.pid {
                    continue;
                }
                if t.state != ThreadState::Zombie {
                    has_live = true;
                }
                if t.state == ThreadState::Running {
                    has_running = true;
                } else if t.state == ThreadState::Ready || t.state == ThreadState::Blocked {
                    has_ready = true;
                }
            }

            p.state = if !has_live {
                ProcState::Zombie
            } else if has_running {
                ProcState::Running
            } else if has_ready {
                ProcState::Ready
            } else {
                ProcState::Zombie
            };
        }
    }

    pub fn send_signal(&mut self, pid: u16, signal: u8) -> Result<(), &'static str> {
        let Some(pidx) = self.find_proc_index(pid) else {
            return Err("no such pid");
        };

        if signal == 0 {
            return Ok(());
        }

        if signal > 31 {
            return Err("invalid signal");
        }

        let bit = 1u32 << (signal as u32);
        self.procs[pidx].pending_signals |= bit;

        // SIGTERM(15) and SIGKILL(9) terminate immediately in this minimal model.
        if signal == 15 || signal == 9 {
            self.procs[pidx].exit_code = 128 + signal as i32;
            self.kill_process(pid)?;
        }

        Ok(())
    }

    pub fn wait_child(
        &mut self,
        parent_pid: u16,
        wanted_pid: Option<u16>,
    ) -> Result<Option<(u16, i32)>, &'static str> {
        let mut found_child = false;

        for i in 0..self.procs.len() {
            let p = self.procs[i];
            if !p.used || p.ppid != parent_pid {
                continue;
            }
            found_child = true;

            if let Some(wpid) = wanted_pid {
                if p.pid != wpid {
                    continue;
                }
            }

            if p.state != ProcState::Zombie {
                continue;
            }

            let pid = p.pid;
            let status = p.exit_code;
            self.reap_process(pid);
            return Ok(Some((pid, status)));
        }

        if !found_child {
            return Err("no child processes");
        }

        Ok(None)
    }

    pub fn pending_signals(&self, pid: u16) -> Result<u32, &'static str> {
        let Some(i) = self.find_proc_index(pid) else {
            return Err("no such pid");
        };
        Ok(self.procs[i].pending_signals)
    }

    fn reap_process(&mut self, pid: u16) {
        for p in &mut self.procs {
            if p.used && p.pid == pid {
                p.used = false;
            }
        }

        for t in &mut self.threads {
            if t.used && t.pid == pid {
                t.used = false;
            }
        }
    }
}

fn copy_name(dst: &mut [u8; 16], dst_len: &mut usize, src: &str) {
    let b = src.as_bytes();
    let n = core::cmp::min(dst.len(), b.len());
    dst[..n].copy_from_slice(&b[..n]);
    *dst_len = n;
}

pub fn name_str(name: &[u8; 16], len: usize) -> &str {
    core::str::from_utf8(&name[..len]).unwrap_or("?")
}

pub fn proc_state_str(state: ProcState) -> &'static str {
    match state {
        ProcState::Running => "RUN",
        ProcState::Ready => "RDY",
        ProcState::Zombie => "ZMB",
    }
}

pub fn thread_state_str(state: ThreadState) -> &'static str {
    match state {
        ThreadState::Running => "RUN",
        ThreadState::Ready => "RDY",
        ThreadState::Blocked => "BLK",
        ThreadState::Zombie => "ZMB",
    }
}
